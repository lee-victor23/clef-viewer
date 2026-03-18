use std::path::PathBuf;
use std::sync::mpsc;

use chrono::Local;

use crate::filter::PropertyFilter;
use crate::parsing::{build_template_summary, load_file};
use crate::types::{DateFilter, LevelStats, LoadState, LogRecord, SortOrder, Tab, TemplateSummary};

// ── App ───────────────────────────────────────────────────────────────────────

pub struct App {
    pub records:          Vec<LogRecord>,
    pub filtered:         Vec<usize>,
    pub search:           String,
    pub level_filters:    [bool; 7],
    pub time_from:        DateFilter,
    pub time_to:          DateFilter,
    pub expanded:         Option<usize>,
    pub file_path:        Option<PathBuf>,
    pub status:           String,
    pub stats:            LevelStats,
    pub page:             usize,
    pub page_size:        usize,
    pub tab:              Tab,
    pub template_summary: Vec<TemplateSummary>,
    pub template_search:  String,
    pub template_filter:  Option<String>,
    pub property_filter:  String,
    pub compiled_pf:      Option<PropertyFilter>,
    pub pf_error:         Option<String>,
    pub sort_order:       SortOrder,
    pub load_rx:          Option<mpsc::Receiver<Vec<LogRecord>>>,
    pub load_state:       LoadState,
    pub pending_path:     Option<PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            records: vec![], filtered: vec![], search: String::new(),
            level_filters: [true; 7],
            time_from: DateFilter::empty(), time_to: DateFilter::empty(),
            expanded: None, file_path: None, status: "No file loaded.".into(),
            stats: LevelStats { counts: [0; 7], exception_count: 0 },
            page: 0, page_size: 100, tab: Tab::Logs,
            template_summary: vec![], template_search: String::new(), template_filter: None,
            property_filter: String::new(), compiled_pf: None, pf_error: None,
            sort_order: SortOrder::Asc,
            load_rx: None, load_state: LoadState::Idle, pending_path: None,
        }
    }
}

impl App {
    pub fn open_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CLEF / log", &["clef", "log", "txt", "json"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load(p);
        }
    }

    pub fn load(&mut self, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        let path_clone = path.clone();
        std::thread::spawn(move || {
            let records = load_file(&path_clone);
            let _ = tx.send(records);
        });
        self.load_rx = Some(rx);
        self.load_state = LoadState::Loading;
        self.pending_path = Some(path);
        self.status = "Loading…".into();
    }

    pub fn poll_load(&mut self) {
        let rx = match self.load_rx.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        match rx.try_recv() {
            Ok(records) => {
                self.records = records;
                self.expanded = None;
                self.page = 0;
                self.template_filter = None;
                if let Some(dt) = self.records.iter().find_map(|r| r.dt_utc) {
                    self.time_from = DateFilter::from_local_dt(dt.with_timezone(&Local));
                }
                if let Some(dt) = self.records.iter().rev().find_map(|r| r.dt_utc) {
                    self.time_to = DateFilter::from_local_dt(dt.with_timezone(&Local));
                }
                self.file_path = self.pending_path.take();
                self.status = format!("Loaded {} records", self.records.len());
                self.load_rx = None;
                self.load_state = LoadState::Idle;
                self.apply_filter();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = "Load failed.".into();
                self.load_rx = None;
                self.load_state = LoadState::Idle;
                self.pending_path = None;
            }
        }
    }

    pub fn close_file(&mut self) {
        self.records.clear();
        self.filtered.clear();
        self.search.clear();
        self.level_filters = [true; 7];
        self.time_from = DateFilter::empty();
        self.time_to = DateFilter::empty();
        self.expanded = None;
        self.file_path = None;
        self.status = "No file loaded.".into();
        self.stats = LevelStats { counts: [0; 7], exception_count: 0 };
        self.page = 0;
        self.tab = Tab::Logs;
        self.template_summary.clear();
        self.template_search.clear();
        self.template_filter = None;
        self.property_filter.clear();
        self.compiled_pf = None;
        self.pf_error = None;
        self.sort_order = SortOrder::Asc;
    }

    pub fn apply_filter(&mut self) {
        let sl = self.search.to_lowercase();
        let uf = self.time_from.to_utc();
        let ut = self.time_to.to_utc();
        let tf = self.template_filter.clone();
        self.filtered = self.records.iter().enumerate().filter(|(_, r)| {
            if !self.level_filters[r.level as usize] { return false; }
            if let Some(f) = uf { if r.dt_utc.map_or(false, |dt| dt < f) { return false; } }
            if let Some(t) = ut { if r.dt_utc.map_or(false, |dt| dt > t) { return false; } }
            if let Some(ref k) = tf {
                let key = if r.template.is_empty() { r.message.chars().take(80).collect::<String>() } else { r.template.clone() };
                if &key != k { return false; }
            }
            if !sl.is_empty() {
                let hit = r.message.to_lowercase().contains(&sl)
                    || r.template.to_lowercase().contains(&sl)
                    || r.timestamp_local.to_lowercase().contains(&sl)
                    || r.exception.to_lowercase().contains(&sl)
                    || r.raw.to_string().to_lowercase().contains(&sl);
                if !hit { return false; }
            }
            if let Some(ref pf) = self.compiled_pf {
                if !pf.matches(&r.raw) { return false; }
            }
            true
        }).map(|(i, _)| i).collect();

        if self.sort_order == SortOrder::Desc {
            self.filtered.reverse();
        }
        self.stats = LevelStats::from_filtered(&self.records, &self.filtered);
        self.template_summary = build_template_summary(&self.records, &self.filtered);
        let pages = self.total_pages();
        if pages > 0 && self.page >= pages { self.page = pages - 1; }
    }

    pub fn recompile_property_filter(&mut self) {
        let expr = self.property_filter.trim();
        if expr.is_empty() {
            self.compiled_pf = None;
            self.pf_error = None;
        } else {
            match PropertyFilter::compile(expr) {
                Ok(pf) => { self.compiled_pf = Some(pf); self.pf_error = None; }
                Err(e) => { self.compiled_pf = None; self.pf_error = Some(e.to_string()); }
            }
        }
    }

    pub fn total_pages(&self) -> usize {
        if self.filtered.is_empty() { return 1; }
        (self.filtered.len() + self.page_size - 1) / self.page_size
    }

    pub fn page_records(&self) -> &[usize] {
        let len = self.filtered.len();
        let s = (self.page * self.page_size).min(len);
        let e = (s + self.page_size).min(len);
        &self.filtered[s..e]
    }
}

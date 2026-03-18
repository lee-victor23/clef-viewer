use std::path::PathBuf;

use chrono::Local;

use crate::parsing::{build_template_summary, load_file};
use crate::types::{DateFilter, LevelStats, LogRecord, Tab, TemplateSummary};

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
}

impl Default for App {
    fn default() -> Self {
        Self {
            records: vec![], filtered: vec![], search: String::new(),
            level_filters: [true; 7],
            time_from: DateFilter::empty(), time_to: DateFilter::empty(),
            expanded: None, file_path: None, status: "No file loaded.".into(),
            stats: LevelStats { counts: [0; 7] },
            page: 0, page_size: 100, tab: Tab::Logs,
            template_summary: vec![], template_search: String::new(), template_filter: None,
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
        self.records = load_file(&path);
        self.expanded = None; self.page = 0; self.template_filter = None;
        if let Some(dt) = self.records.iter().find_map(|r| r.dt_utc) {
            self.time_from = DateFilter::from_local_dt(dt.with_timezone(&Local));
        }
        if let Some(dt) = self.records.iter().rev().find_map(|r| r.dt_utc) {
            self.time_to = DateFilter::from_local_dt(dt.with_timezone(&Local));
        }
        self.file_path = Some(path);
        self.status = format!("Loaded {} records", self.records.len());
        self.apply_filter();
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
            if sl.is_empty() { return true; }
            r.timestamp_local.to_lowercase().contains(&sl)
            || r.message.to_lowercase().contains(&sl)
            || r.template.to_lowercase().contains(&sl)
            || r.raw.to_string().to_lowercase().contains(&sl)
        }).map(|(i, _)| i).collect();

        self.stats = LevelStats::from_filtered(&self.records, &self.filtered);
        self.template_summary = build_template_summary(&self.records, &self.filtered);
        let pages = self.total_pages();
        if pages > 0 && self.page >= pages { self.page = pages - 1; }
    }

    pub fn total_pages(&self) -> usize {
        (self.filtered.len() + self.page_size - 1).max(1) / self.page_size
    }

    pub fn page_records(&self) -> &[usize] {
        let s = self.page * self.page_size;
        let e = (s + self.page_size).min(self.filtered.len());
        &self.filtered[s..e]
    }
}

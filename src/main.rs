#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use eframe::egui;
use egui::{Color32, FontId, RichText, ScrollArea, TextEdit, Vec2};
use egui_extras::DatePickerButton;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Serilog template rendering ────────────────────────────────────────────────

fn render_template(template: &str, obj: &serde_json::Map<String, Value>) -> String {
    let mut out = String::with_capacity(template.len() * 2);
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' if i + 1 < chars.len() && chars[i + 1] == '{' => { out.push('{'); i += 2; }
            '}' if i + 1 < chars.len() && chars[i + 1] == '}' => { out.push('}'); i += 2; }
            '{' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != '}' { j += 1; }
                if j < chars.len() {
                    let token: String = chars[start..j].iter().collect();
                    let name = token.trim_start_matches('@').split(':').next().unwrap_or(&token);
                    let val = obj.get(name)
                        .map(value_to_display)
                        .unwrap_or_else(|| format!("{{{}}}", token));
                    out.push_str(&val);
                    i = j + 1;
                } else {
                    out.push('{');
                    i += 1;
                }
            }
            c => { out.push(c); i += 1; }
        }
    }
    out
}

fn value_to_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

// ── Level ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Hash)]
enum Level {
    Verbose = 0,
    Debug   = 1,
    Info    = 2,
    Warning = 3,
    Error   = 4,
    Fatal   = 5,
    Unknown = 6,
}

impl Level {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "verbose" | "v"                => Level::Verbose,
            "debug"   | "d"                => Level::Debug,
            "information" | "info" | "i"   => Level::Info,
            "warning" | "warn" | "w"       => Level::Warning,
            "error"   | "e"                => Level::Error,
            "fatal"   | "f"                => Level::Fatal,
            _                              => Level::Unknown,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Level::Verbose => "Verbose",
            Level::Debug   => "Debug",
            Level::Info    => "Information",
            Level::Warning => "Warning",
            Level::Error   => "Error",
            Level::Fatal   => "Fatal",
            Level::Unknown => "Unknown",
        }
    }

    fn short(&self) -> &'static str {
        match self { Level::Verbose=>"VRB", Level::Debug=>"DBG", Level::Info=>"INF",
                     Level::Warning=>"WRN", Level::Error=>"ERR", Level::Fatal=>"FTL",
                     Level::Unknown=>"???" }
    }

    // Colours from the reference app
    fn color(&self) -> Color32 {
        match self {
            Level::Verbose => Color32::from_rgb(108, 117, 125), // #6c757d
            Level::Debug   => Color32::from_rgb( 32, 201, 151), // #20c997
            Level::Info    => Color32::from_rgb( 23, 162, 184), // #17a2b8
            Level::Warning => Color32::from_rgb(255, 193,   7), // #ffc107
            Level::Error   => Color32::from_rgb(253, 126,  20), // #fd7e14
            Level::Fatal   => Color32::from_rgb(220,  53,  69), // #dc3545
            Level::Unknown => Color32::from_rgb(173, 181, 189),
        }
    }

    fn bg_color(&self) -> Color32 {
        // Darkened version of fg for badge background
        match self {
            Level::Verbose => Color32::from_rgb( 35,  38,  41),
            Level::Debug   => Color32::from_rgb( 10,  60,  45),
            Level::Info    => Color32::from_rgb(  8,  52,  60),
            Level::Warning => Color32::from_rgb( 80,  60,   2),
            Level::Error   => Color32::from_rgb( 80,  38,   5),
            Level::Fatal   => Color32::from_rgb( 70,  15,  20),
            Level::Unknown => Color32::from_rgb( 44,  47,  51),
        }
    }
}

const ALL_LEVELS: [Level; 6] = [
    Level::Verbose, Level::Debug, Level::Info,
    Level::Warning, Level::Error, Level::Fatal,
];

// ── DateTime picker ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct DateFilter {
    date:    NaiveDate,
    enabled: bool,
}

impl DateFilter {
    fn empty() -> Self {
        Self { date: Local::now().date_naive(), enabled: false }
    }
    fn from_local_dt(dt: DateTime<Local>) -> Self {
        Self { date: dt.date_naive(), enabled: false }
    }
    fn to_utc_start(&self) -> Option<DateTime<Utc>> {
        if !self.enabled { return None; }
        self.date.and_hms_opt(0, 0, 0)
            .and_then(|ndt| ndt.and_local_timezone(Local).single())
            .map(|l| l.with_timezone(&Utc))
    }
    fn to_utc_end(&self) -> Option<DateTime<Utc>> {
        if !self.enabled { return None; }
        self.date.and_hms_opt(23, 59, 59)
            .and_then(|ndt| ndt.and_local_timezone(Local).single())
            .map(|l| l.with_timezone(&Utc))
    }
}

fn date_filter_ui(ui: &mut egui::Ui, f: &mut DateFilter, id: &str) -> bool {
    let mut ch = false;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut f.enabled, "").changed() { ch = true; }
        ui.add_enabled_ui(f.enabled, |ui| {
            if ui.add(DatePickerButton::new(&mut f.date).id_salt(id)).changed() {
                ch = true;
            }
        });
    });
    ch
}

// ── LogRecord ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct LogRecord {
    timestamp_utc:   String,
    dt_utc:          Option<DateTime<Utc>>,
    timestamp_local: String,
    level:           Level,
    message:         String,   // rendered
    template:        String,   // @mt
    exception:       String,   // @x
    raw:             Value,
    line_no:         usize,
}

fn parse_utc(ts: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) { return Some(dt.with_timezone(&Utc)); }
    for fmt in &["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(ts, fmt) {
            return Some(Utc.from_utc_datetime(&n));
        }
    }
    None
}

fn fmt_local(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_clef_line(line: &str, line_no: usize) -> Option<LogRecord> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let obj = v.as_object()?;

    let timestamp_utc = obj.get("@t").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let dt_utc = parse_utc(&timestamp_utc);
    let timestamp_local = dt_utc.as_ref().map(fmt_local).unwrap_or_else(|| timestamp_utc.clone());

    let level = obj.get("@l").and_then(|l| l.as_str()).map(Level::from_str).unwrap_or(Level::Info);
    let template = obj.get("@mt").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let exception = obj.get("@x").and_then(|x| x.as_str()).unwrap_or("").to_string();

    let raw_m = obj.get("@m").and_then(|m| m.as_str()).unwrap_or("");
    let message = if !raw_m.is_empty() {
        raw_m.to_string()
    } else if !template.is_empty() {
        render_template(&template, obj)
    } else {
        String::new()
    };

    Some(LogRecord { timestamp_utc, dt_utc, timestamp_local, level, message, template, exception, raw: v, line_no })
}

fn load_file(path: &PathBuf) -> Vec<LogRecord> {
    let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return vec![] };
    content.lines().enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .filter_map(|(i, l)| parse_clef_line(l, i + 1))
        .collect()
}

// ── Template summary ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct TemplateSummary { template: String, count: usize, level: Level }

fn build_template_summary(records: &[LogRecord], filtered: &[usize]) -> Vec<TemplateSummary> {
    let mut map: HashMap<String, (usize, HashMap<Level, usize>)> = HashMap::new();
    for &i in filtered {
        let r = &records[i];
        let key = if r.template.is_empty() { r.message.chars().take(80).collect() } else { r.template.clone() };
        let e = map.entry(key).or_insert((0, HashMap::new()));
        e.0 += 1;
        *e.1.entry(r.level).or_insert(0) += 1;
    }
    let mut list: Vec<TemplateSummary> = map.into_iter().map(|(template, (count, lm))| {
        let level = lm.into_iter().max_by_key(|(_, c)| *c).map(|(l,_)| l).unwrap_or(Level::Info);
        TemplateSummary { template, count, level }
    }).collect();
    list.sort_by(|a, b| b.count.cmp(&a.count));
    list
}

// ── Stats ─────────────────────────────────────────────────────────────────────

struct LevelStats { counts: [usize; 7] }
impl LevelStats {
    fn from_filtered(records: &[LogRecord], filtered: &[usize]) -> Self {
        let mut counts = [0usize; 7];
        for &i in filtered { counts[records[i].level as usize] += 1; }
        LevelStats { counts }
    }
    fn total(&self) -> usize { self.counts.iter().sum() }
    fn count(&self, l: Level) -> usize { self.counts[l as usize] }
}

// ── Tab ───────────────────────────────────────────────────────────────────────

#[derive(PartialEq)] enum Tab { Logs, Templates }

// ── App ───────────────────────────────────────────────────────────────────────

struct App {
    records:          Vec<LogRecord>,
    filtered:         Vec<usize>,
    search:           String,
    level_filters:    [bool; 7],
    time_from:        DateFilter,
    time_to:          DateFilter,
    expanded:         Option<usize>,   // index into records; detail shown inline
    file_path:        Option<PathBuf>,
    status:           String,
    stats:            LevelStats,
    page:             usize,
    page_size:        usize,
    tab:              Tab,
    template_summary: Vec<TemplateSummary>,
    template_search:  String,
    template_filter:  Option<String>,
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
    fn open_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CLEF / log", &["clef","log","txt","json"])
            .add_filter("All files", &["*"])
            .pick_file()
        { self.load(p); }
    }

    fn load(&mut self, path: PathBuf) {
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

    fn apply_filter(&mut self) {
        let sl = self.search.to_lowercase();
        let uf = self.time_from.to_utc_start();
        let ut = self.time_to.to_utc_end();
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
        }).map(|(i,_)| i).collect();

        self.stats = LevelStats::from_filtered(&self.records, &self.filtered);
        self.template_summary = build_template_summary(&self.records, &self.filtered);
        let pages = self.total_pages();
        if pages > 0 && self.page >= pages { self.page = pages - 1; }
    }

    fn total_pages(&self) -> usize { (self.filtered.len() + self.page_size - 1).max(1) / self.page_size }

    fn page_records(&self) -> &[usize] {
        let s = self.page * self.page_size;
        let e = (s + self.page_size).min(self.filtered.len());
        &self.filtered[s..e]
    }
}

// ── Rendering helpers ─────────────────────────────────────────────────────────

fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::none().fill(bg).rounding(4.0)
        .inner_margin(egui::Margin::symmetric(7.0, 3.0))
        .show(ui, |ui| { ui.label(RichText::new(text).color(fg).strong().size(13.0)); });
}

fn body(text: impl Into<String>) -> RichText { RichText::new(text).size(14.0) }
fn mono(text: impl Into<String>) -> RichText { RichText::new(text).size(13.0).monospace() }
fn small_gray(text: impl Into<String>) -> RichText { RichText::new(text).size(12.0).color(Color32::from_gray(140)) }

// ── Inline detail panel ───────────────────────────────────────────────────────

fn show_detail(ui: &mut egui::Ui, record: &LogRecord) {
    let bg = Color32::from_rgb(28, 30, 38);
    egui::Frame::none()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
            // Exception block
            if !record.exception.is_empty() {
                egui::Frame::none()
                    .fill(Color32::from_rgb(40, 15, 15))
                    .stroke(egui::Stroke::new(3.0, Color32::from_rgb(220, 53, 69)))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Exception").color(Color32::from_rgb(220, 53, 69)).strong().size(15.0));
                        ui.add_space(4.0);
                        ui.label(mono(&record.exception).color(Color32::from_rgb(255, 180, 180)));
                    });
                ui.add_space(8.0);
            }

            // Properties table
            egui::Grid::new(format!("detail_{}", record.line_no))
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 5.0])
                .min_col_width(160.0)
                .show(ui, |ui| {
                    // Timestamp row first
                    ui.label(mono("@t").color(Color32::from_rgb(140, 170, 255)));
                    ui.label(body(format!("{} (local)  /  {} (UTC)", record.timestamp_local, record.timestamp_utc)));
                    ui.end_row();

                    if let Some(obj) = record.raw.as_object() {
                        for (k, v) in obj {
                            if k == "@t" || k == "@l" { continue; } // already shown / redundant
                            let key_color = if k.starts_with('@') {
                                Color32::from_rgb(140, 170, 255)
                            } else {
                                Color32::from_rgb(100, 210, 180)
                            };
                            ui.label(mono(k).color(key_color));
                            let val_str = match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            // Exception already shown above
                            if k == "@x" {
                                ui.label(small_gray("[see exception above]"));
                            } else {
                                ui.add(
                                    TextEdit::multiline(&mut val_str.as_str())
                                        .desired_width(f32::INFINITY)
                                        .font(FontId::monospace(13.0)),
                                );
                            }
                            ui.end_row();
                        }
                    }
                });
        });
}

// ── Main UI ───────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // ── Toolbar ───────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").exact_height(106.0).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("  Open file…  ").size(14.0)).clicked() { self.open_file(); }
                ui.separator();
                if ui.selectable_label(self.tab == Tab::Logs,      RichText::new("Logs").size(14.0)).clicked() { self.tab = Tab::Logs; }
                if ui.selectable_label(self.tab == Tab::Templates,  RichText::new("Message Templates").size(14.0)).clicked() { self.tab = Tab::Templates; }
                ui.separator();
                ui.label(body("Search:"));
                let r = ui.add(TextEdit::singleline(&mut self.search).desired_width(300.0).hint_text("Search logs…").font(egui::TextStyle::Body));
                if r.changed() { self.page = 0; self.apply_filter(); }
                if ui.button("Clear").clicked() { self.search.clear(); self.template_filter = None; self.page = 0; self.apply_filter(); }

                if let Some(ref tf) = self.template_filter.clone() {
                    ui.separator();
                    let short: String = tf.chars().take(50).collect();
                    ui.label(RichText::new(format!("Template: {}…", short)).color(Color32::from_rgb(255, 200, 80)).size(13.0));
                    if ui.button("✕").clicked() { self.template_filter = None; self.page = 0; self.apply_filter(); }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(small_gray(&self.status));
                    ui.label(small_gray(format!("{} / {} |", self.filtered.len(), self.records.len())));
                });
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(small_gray("From (local):"));
                let c1 = date_filter_ui(ui, &mut self.time_from, "tf");
                ui.add_space(8.0);
                ui.label(small_gray("To (local):"));
                let c2 = date_filter_ui(ui, &mut self.time_to, "tt");
                if c1 || c2 { self.page = 0; self.apply_filter(); }

                ui.add_space(12.0); ui.separator(); ui.add_space(4.0);
                ui.label(body("Level:"));
                let mut ch = false;
                for lvl in &ALL_LEVELS {
                    let idx = *lvl as usize;
                    let active = self.level_filters[idx];
                    let cnt = self.stats.count(*lvl);
                    let text = RichText::new(format!("{} ({})", lvl.short(), cnt))
                        .color(if active { lvl.color() } else { Color32::from_gray(55) })
                        .strong().size(13.0);
                    if ui.selectable_label(active, text).clicked() { self.level_filters[idx] = !active; ch = true; }
                }
                if ch { self.page = 0; self.apply_filter(); }
            });
        });

        // ── Stats sidebar ─────────────────────────────────────────────────────
        egui::SidePanel::right("stats").resizable(false).exact_width(230.0).show(ctx, |ui| {
            ui.add_space(10.0);
            let errs  = self.stats.count(Level::Error);
            let fatal = self.stats.count(Level::Fatal);
            let total_ef = errs + fatal;
            let lbl = if fatal > 0 { "Errors + Fatal" } else { "Errors" };
            let (bg, fg) = if total_ef > 0 { (Color32::from_rgb(180,50,50), Color32::WHITE) }
                           else             { (Color32::from_rgb(35,110,55),  Color32::WHITE) };
            egui::Frame::none().fill(bg).rounding(8.0).inner_margin(egui::Margin::symmetric(12.0,10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(lbl).color(fg).size(13.0));
                    ui.label(RichText::new(total_ef.to_string()).color(fg).size(36.0).strong());
                });
            });

            ui.add_space(12.0); ui.separator(); ui.add_space(6.0);
            ui.label(RichText::new("Log Levels").strong().size(14.0));
            ui.add_space(6.0);

            let total = self.stats.total().max(1);
            let bar_w = ui.available_width() - 8.0;
            let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 16.0), egui::Sense::hover());
            let p = ui.painter();
            let mut cx = bar_rect.left();
            for lvl in &ALL_LEVELS {
                let c = self.stats.count(*lvl);
                if c == 0 { continue; }
                let w = (c as f32 / total as f32) * bar_w;
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(cx, bar_rect.top()), Vec2::new(w,16.0)), 0.0, lvl.color());
                cx += w;
            }
            ui.add_space(10.0);

            egui::Grid::new("lstats").num_columns(3).spacing([6.0,5.0]).show(ui, |ui| {
                ui.label(RichText::new("Level").strong().size(13.0));
                ui.label(RichText::new("Count").strong().size(13.0));
                ui.label(RichText::new("%").strong().size(13.0));
                ui.end_row();
                for lvl in &ALL_LEVELS {
                    let c = self.stats.count(*lvl);
                    let pct = c as f32 / total as f32 * 100.0;
                    ui.horizontal(|ui| {
                        let (dr, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                        ui.painter().circle_filled(dr.center(), 6.0, lvl.color());
                        ui.label(RichText::new(lvl.label()).size(13.0));
                    });
                    ui.label(RichText::new(c.to_string()).size(13.0).monospace());
                    if c > 0 { ui.label(RichText::new(format!("{:.1}%",pct)).size(12.0).color(Color32::from_gray(160))); }
                    else      { ui.label(RichText::new("-").size(12.0).color(Color32::from_gray(70))); }
                    ui.end_row();
                }
            });

            ui.add_space(8.0); ui.separator(); ui.add_space(4.0);
            ui.label(small_gray(format!("Visible: {}", self.filtered.len())));
            ui.label(small_gray(format!("UTC{} local", Local::now().offset())));
        });

        // ── Central ───────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.records.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Open a .clef file to view logs").size(20.0).color(Color32::from_gray(80)));
                });
                return;
            }
            match self.tab {
                Tab::Logs      => self.show_logs_tab(ui),
                Tab::Templates => self.show_templates_tab(ui),
            }
        });
    }
}

impl App {
    fn show_logs_tab(&mut self, ui: &mut egui::Ui) {
        let total_pages = self.total_pages();

        // Pagination bar
        ui.horizontal(|ui| {
            ui.label(body(format!("Page {} / {}  ({} records)", self.page+1, total_pages, self.filtered.len())));
            ui.add_space(6.0);
            if ui.add_enabled(self.page > 0, egui::Button::new(body("< Prev"))).clicked() {
                self.page -= 1; self.expanded = None;
            }
            let ws = self.page.saturating_sub(4);
            let we = (ws + 10).min(total_pages);
            for p in ws..we {
                let active = p == self.page;
                if ui.selectable_label(active, body((p+1).to_string())).clicked() && !active {
                    self.page = p; self.expanded = None;
                }
            }
            if ui.add_enabled(self.page+1 < total_pages, egui::Button::new(body("Next >"))).clicked() {
                self.page += 1; self.expanded = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for &sz in &[50usize,100,200,500] {
                    if ui.selectable_label(self.page_size==sz, body(sz.to_string())).clicked() {
                        self.page_size = sz; self.page = 0; self.expanded = None;
                    }
                }
                ui.label(body("Per page:"));
            });
        });
        ui.separator();

        // Column widths
        let avail_w = ui.available_width();
        let ts_w  = 170.0f32;
        let lvl_w = 110.0f32;
        let msg_w = avail_w - ts_w - lvl_w - 16.0;

        // Header
        ui.horizontal(|ui| {
            ui.add_sized([ts_w,  20.0], egui::Label::new(RichText::new("Timestamp (local)").strong().size(13.0)));
            ui.add_sized([lvl_w, 20.0], egui::Label::new(RichText::new("Level").strong().size(13.0)));
            ui.label(RichText::new("Message").strong().size(13.0));
        });
        ui.separator();

        let page_indices: Vec<usize> = self.page_records().to_vec();
        let expanded = self.expanded;

        ScrollArea::vertical().id_salt("log_scroll").auto_shrink([false;2]).show(ui, |ui| {
            for (row_i, &rec_idx) in page_indices.iter().enumerate() {
                let record = &self.records[rec_idx];
                let is_expanded = expanded == Some(rec_idx);

                let row_bg = if is_expanded {
                    Color32::from_rgb(30, 45, 80)
                } else if row_i % 2 == 0 {
                    Color32::from_rgb(26, 26, 30)
                } else {
                    Color32::from_rgb(20, 20, 24)
                };

                // ── Main row ──
                let row_resp = egui::Frame::none()
                    .fill(row_bg)
                    .inner_margin(egui::Margin::symmetric(6.0, 5.0))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            // Timestamp — fixed width, no wrap
                            ui.add_sized([ts_w, 20.0], egui::Label::new(
                                mono(&record.timestamp_local).color(Color32::from_gray(180))
                            ));

                            // Level badge
                            ui.scope(|ui| {
                                ui.set_width(lvl_w);
                                badge(ui, record.level.label(), record.level.color(), record.level.bg_color());
                            });

                            // Message — full text, left-aligned wrapping (no justify)
                            let display = if !record.message.is_empty() { &record.message } else { &record.template };
                            ui.allocate_ui(Vec2::new(msg_w, 0.0), |ui| {
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true), |ui| {
                                    ui.label(RichText::new(display).size(14.0).color(Color32::WHITE));
                                });
                            });
                        });
                    });

                if row_resp.response.interact(egui::Sense::click()).clicked() {
                    self.expanded = if is_expanded { None } else { Some(rec_idx) };
                }

                // ── Inline detail (expands in place below the row) ──
                if is_expanded {
                    let record = self.records[rec_idx].clone();
                    show_detail(ui, &record);
                    if ui.horizontal(|ui| ui.button(body("▲  Close")).clicked()).inner {
                        self.expanded = None;
                    }
                    ui.separator();
                }
            }
        });
    }

    fn show_templates_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(body("Filter:"));
            let r = ui.add(TextEdit::singleline(&mut self.template_search).desired_width(300.0).hint_text("Search templates…"));
            if r.changed() {}
            if ui.button("Clear").clicked() { self.template_search.clear(); }
            ui.label(small_gray(format!("{} unique templates", self.template_summary.len())));
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.add_sized([ 70.0,20.0], egui::Label::new(RichText::new("Count").strong().size(13.0)));
            ui.add_sized([110.0,20.0], egui::Label::new(RichText::new("Level").strong().size(13.0)));
            ui.label(RichText::new("Message Template").strong().size(13.0));
        });
        ui.separator();

        let search = self.template_search.to_lowercase();
        let summary = self.template_summary.clone();
        let active_filter = self.template_filter.clone();

        ScrollArea::vertical().id_salt("tmpl_scroll").auto_shrink([false;2]).show(ui, |ui| {
            for (row_i, ts) in summary.iter()
                .filter(|t| search.is_empty() || t.template.to_lowercase().contains(&search))
                .enumerate()
            {
                let is_active = active_filter.as_deref() == Some(&ts.template);
                let bg = if is_active { Color32::from_rgb(35,55,95) }
                         else if row_i%2==0 { Color32::from_rgb(26,26,30) }
                         else { Color32::from_rgb(20,20,24) };

                let resp = egui::Frame::none().fill(bg).inner_margin(egui::Margin::symmetric(6.0,5.0)).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.add_sized([70.0,20.0], egui::Label::new(
                            RichText::new(ts.count.to_string()).size(14.0).strong().monospace()
                        ));
                        ui.scope(|ui| { ui.set_width(110.0); badge(ui, ts.level.label(), ts.level.color(), ts.level.bg_color()); });
                        ui.add(egui::Label::new(RichText::new(&ts.template).size(14.0).color(Color32::from_gray(210))).wrap());
                    });
                });

                if resp.response.interact(egui::Sense::click()).clicked() {
                    if is_active {
                        self.template_filter = None;
                    } else {
                        self.template_filter = Some(ts.template.clone());
                        self.tab = Tab::Logs;
                    }
                    self.page = 0;
                    self.apply_filter();
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CLEF Viewer")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native("CLEF Viewer", options, Box::new(|cc| {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        // Slightly larger default font
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
        cc.egui_ctx.set_style(style);
        Ok(Box::new(App::default()))
    }))
}

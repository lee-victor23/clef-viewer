use eframe::egui;
use egui::{Color32, FontId, RichText, ScrollArea, TextEdit, Vec2};
use egui_extras::DatePickerButton;

use crate::app::App;
use crate::types::{DateFilter, Level, LogRecord, Tab, ALL_LEVELS};

// ── Widget helpers ────────────────────────────────────────────────────────────

pub fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::none().fill(bg).rounding(4.0)
        .inner_margin(egui::Margin::symmetric(7.0, 3.0))
        .show(ui, |ui| { ui.label(RichText::new(text).color(fg).strong().size(13.0)); });
}

pub fn body(text: impl Into<String>) -> RichText { RichText::new(text).size(14.0) }
pub fn mono(text: impl Into<String>) -> RichText { RichText::new(text).size(13.0).monospace() }
pub fn small_gray(text: impl Into<String>) -> RichText { RichText::new(text).size(12.0).color(Color32::from_gray(140)) }

// ── Date filter widget ────────────────────────────────────────────────────────

pub fn date_filter_ui(ui: &mut egui::Ui, f: &mut DateFilter, id: &str) -> bool {
    let mut ch = false;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut f.enabled, "").changed() { ch = true; }
        ui.add_enabled_ui(f.enabled, |ui| {
            if ui.add(DatePickerButton::new(&mut f.date).id_salt(id)).changed() { ch = true; }
            ui.add_space(4.0);
            let mut h = f.hour;
            let mut m = f.minute;
            let mut s = f.second;
            if ui.add(egui::DragValue::new(&mut h).range(0..=23).prefix("").suffix("h")).changed() { f.hour   = h; ch = true; }
            ui.label(RichText::new(":").color(Color32::from_gray(110)));
            if ui.add(egui::DragValue::new(&mut m).range(0..=59).custom_formatter(|v, _| format!("{:02}", v as u32))).changed() { f.minute = m; ch = true; }
            ui.label(RichText::new(":").color(Color32::from_gray(110)));
            if ui.add(egui::DragValue::new(&mut s).range(0..=59).custom_formatter(|v, _| format!("{:02}", v as u32))).changed() { f.second = s; ch = true; }
        });
    });
    ch
}

// ── Inline detail panel ───────────────────────────────────────────────────────

pub fn show_detail(ui: &mut egui::Ui, record: &LogRecord) {
    let bg = Color32::from_rgb(28, 30, 38);
    egui::Frame::none()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
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

            egui::Grid::new(format!("detail_{}", record.line_no))
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 2.0])
                .min_col_width(160.0)
                .show(ui, |ui| {
                    ui.label(mono("@t").color(Color32::from_rgb(140, 170, 255)));
                    ui.label(body(&record.timestamp_local));
                    ui.end_row();

                    if let Some(obj) = record.raw.as_object() {
                        for (k, v) in obj {
                            if k == "@t" || k == "@l" { continue; }
                            let key_color = if k.starts_with('@') {
                                Color32::from_rgb(140, 170, 255)
                            } else {
                                Color32::from_rgb(100, 210, 180)
                            };
                            ui.label(mono(k).color(key_color));
                            let val_str = match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            if k == "@x" {
                                ui.label(small_gray("[see exception above]"));
                            } else if val_str.contains('\n') || val_str.len() > 120 {
                                ui.add(
                                    TextEdit::multiline(&mut val_str.as_str())
                                        .desired_width(f32::INFINITY)
                                        .font(FontId::monospace(13.0)),
                                );
                            } else {
                                ui.label(mono(&val_str));
                            }
                            ui.end_row();
                        }
                    }
                });
        });
}

// ── Main render loop ──────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // ── Toolbar ───────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").exact_height(136.0).show(ctx, |ui| {
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

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(small_gray("Property filter:"));
                let pf_resp = ui.add(
                    TextEdit::singleline(&mut self.property_filter)
                        .desired_width(400.0)
                        .hint_text("e.g. Contains(SourceContext, \"Api\") && Duration > 100")
                        .font(egui::TextStyle::Body),
                );
                if pf_resp.changed() {
                    self.recompile_property_filter();
                    self.page = 0;
                    self.apply_filter();
                }
                if !self.property_filter.is_empty() {
                    if ui.button("Clear").clicked() {
                        self.property_filter.clear();
                        self.compiled_pf = None;
                        self.pf_error = None;
                        self.page = 0;
                        self.apply_filter();
                    }
                }
                if let Some(ref err) = self.pf_error {
                    ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)).size(12.0));
                }
            });
        });

        // ── Stats sidebar ─────────────────────────────────────────────────────
        egui::SidePanel::right("stats").resizable(false).exact_width(230.0).show(ctx, |ui| {
            ui.add_space(10.0);
            let errs  = self.stats.count(Level::Error);
            let fatal = self.stats.count(Level::Fatal);
            let total_ef = errs + fatal;
            let lbl = if fatal > 0 { "Errors + Fatal" } else { "Errors" };
            let (bg, fg) = if total_ef > 0 { (Color32::from_rgb(180, 50, 50), Color32::WHITE) }
                           else             { (Color32::from_rgb( 35, 110, 55), Color32::WHITE) };
            egui::Frame::none().fill(bg).rounding(8.0).inner_margin(egui::Margin::symmetric(12.0, 10.0)).show(ui, |ui| {
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
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(cx, bar_rect.top()), Vec2::new(w, 16.0)), 0.0, lvl.color());
                cx += w;
            }
            ui.add_space(10.0);

            egui::Grid::new("lstats").num_columns(3).spacing([6.0, 5.0]).show(ui, |ui| {
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
                    if c > 0 { ui.label(RichText::new(format!("{:.1}%", pct)).size(12.0).color(Color32::from_gray(160))); }
                    else      { ui.label(RichText::new("-").size(12.0).color(Color32::from_gray(70))); }
                    ui.end_row();
                }
            });

            ui.add_space(8.0); ui.separator(); ui.add_space(4.0);
            ui.label(small_gray(format!("Visible: {}", self.filtered.len())));
            ui.label(small_gray(format!("UTC{} local", chrono::Local::now().offset())));
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

// ── Tab rendering ─────────────────────────────────────────────────────────────

impl App {
    pub fn show_logs_tab(&mut self, ui: &mut egui::Ui) {
        let total_pages = self.total_pages();

        ui.horizontal(|ui| {
            ui.label(body(format!("Page {} / {}  ({} records)", self.page + 1, total_pages, self.filtered.len())));
            ui.add_space(6.0);
            if ui.add_enabled(self.page > 0, egui::Button::new(body("< Prev"))).clicked() {
                self.page -= 1; self.expanded = None;
            }
            let ws = self.page.saturating_sub(4);
            let we = (ws + 10).min(total_pages);
            for p in ws..we {
                let active = p == self.page;
                if ui.selectable_label(active, body((p + 1).to_string())).clicked() && !active {
                    self.page = p; self.expanded = None;
                }
            }
            if ui.add_enabled(self.page + 1 < total_pages, egui::Button::new(body("Next >"))).clicked() {
                self.page += 1; self.expanded = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for &sz in &[50usize, 100, 200, 500] {
                    if ui.selectable_label(self.page_size == sz, body(sz.to_string())).clicked() {
                        self.page_size = sz; self.page = 0; self.expanded = None;
                    }
                }
                ui.label(body("Per page:"));
            });
        });
        ui.separator();

        let avail_w = ui.available_width();
        let ts_w  = 170.0f32;
        let lvl_w = 110.0f32;
        let msg_w = avail_w - ts_w - lvl_w - 16.0;

        ui.horizontal(|ui| {
            ui.add_sized([ts_w,  20.0], egui::Label::new(RichText::new("Timestamp (local)").strong().size(13.0)));
            ui.add_sized([lvl_w, 20.0], egui::Label::new(RichText::new("Level").strong().size(13.0)));
            ui.label(RichText::new("Message").strong().size(13.0));
        });
        ui.separator();

        let page_indices: Vec<usize> = self.page_records().to_vec();
        let expanded = self.expanded;

        ScrollArea::vertical().id_salt("log_scroll").auto_shrink([false; 2]).show(ui, |ui| {
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

                let row_resp = egui::Frame::none()
                    .fill(row_bg)
                    .inner_margin(egui::Margin::symmetric(6.0, 5.0))
                    .show(ui, |ui| {
                        ui.set_min_width(avail_w);
                        ui.horizontal_top(|ui| {
                            ui.add_sized([ts_w, 20.0], egui::Label::new(
                                mono(&record.timestamp_local).color(Color32::from_gray(180))
                            ));
                            ui.scope(|ui| {
                                ui.set_width(lvl_w);
                                badge(ui, record.level.label(), record.level.color(), record.level.bg_color());
                            });
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

    pub fn show_templates_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(body("Filter:"));
            let r = ui.add(TextEdit::singleline(&mut self.template_search).desired_width(300.0).hint_text("Search templates…"));
            if r.changed() {}
            if ui.button("Clear").clicked() { self.template_search.clear(); }
            ui.label(small_gray(format!("{} unique templates", self.template_summary.len())));
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.add_sized([ 70.0, 20.0], egui::Label::new(RichText::new("Count").strong().size(13.0)));
            ui.add_sized([110.0, 20.0], egui::Label::new(RichText::new("Level").strong().size(13.0)));
            ui.label(RichText::new("Message Template").strong().size(13.0));
        });
        ui.separator();

        let search = self.template_search.to_lowercase();
        let summary = self.template_summary.clone();
        let active_filter = self.template_filter.clone();

        ScrollArea::vertical().id_salt("tmpl_scroll").auto_shrink([false; 2]).show(ui, |ui| {
            for (row_i, ts) in summary.iter()
                .filter(|t| search.is_empty() || t.template.to_lowercase().contains(&search))
                .enumerate()
            {
                let is_active = active_filter.as_deref() == Some(&ts.template);
                let bg = if is_active { Color32::from_rgb(35, 55, 95) }
                         else if row_i % 2 == 0 { Color32::from_rgb(26, 26, 30) }
                         else { Color32::from_rgb(20, 20, 24) };

                let resp = egui::Frame::none().fill(bg).inner_margin(egui::Margin::symmetric(6.0, 5.0)).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.add_sized([70.0, 20.0], egui::Label::new(
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

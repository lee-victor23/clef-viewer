#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod filter;
mod parsing;
mod types;
mod ui;

use app::App;
use eframe::egui;
use egui::FontId;

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
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Body,   FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(egui::TextStyle::Small,  FontId::proportional(12.0));
        cc.egui_ctx.set_style(style);
        Ok(Box::new(App::default()))
    }))
}

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod book;
mod comics;
mod epub;
mod files;
mod fonts;
mod job;
mod pdfimg;
mod pdfout;
mod theme;

/// Icona della finestra: 128x128 RGBA incorporata.
fn app_icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: include_bytes!("../assets/icon.rgba").to_vec(),
        width: 128,
        height: 128,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("{} v{}", app::APP_TITLE, app::APP_VERSION))
            .with_inner_size([980.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(app_icon())
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        app::APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

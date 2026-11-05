//! Finestra principale di PdfTool.

use crate::files::scan_dir;
use crate::job::{run_job, JobParams, Level, Mode, Msg};
use crate::pdfimg::ImgFormat;
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

pub const APP_TITLE: &str = "PdfTool";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct App {
    mode: Mode,
    inputs: Vec<PathBuf>,
    out_dir: String,
    name: String,
    format: ImgFormat,
    dpi: u32,
    log: Vec<(Color32, String)>,
    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    global: (usize, usize),
    item: (usize, usize),
    busy: bool,
    summary: Option<String>,
}

/// Aggiunge file e cartelle (cercati ricorsivamente) alla lista, senza doppioni
/// e scartando le estensioni non adatte alla modalità. Restituisce quanti ne ha aggiunti.
pub fn add_paths(list: &mut Vec<PathBuf>, paths: &[PathBuf], mode: Mode) -> usize {
    let exts = mode.input_exts();
    let mut added = 0;
    for p in paths {
        let found = if p.is_dir() {
            scan_dir(p, exts)
        } else if crate::files::has_ext(p, exts) {
            vec![p.clone()]
        } else {
            Vec::new()
        };
        for f in found {
            if !list.contains(&f) {
                list.push(f);
                added += 1;
            }
        }
    }
    added
}

/// Cartella di uscita: quella scelta, altrimenti quella del primo file.
pub fn effective_out_dir(chosen: &str, inputs: &[PathBuf]) -> Option<PathBuf> {
    let c = chosen.trim();
    if !c.is_empty() {
        return Some(PathBuf::from(c));
    }
    inputs
        .first()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
}

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Info => theme::LOG_INFO,
        Level::Ok => theme::LOG_OK,
        Level::Warn => theme::LOG_WARN,
        Level::Err => theme::LOG_ERR,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        App {
            mode: Mode::Books,
            inputs: Vec::new(),
            out_dir: String::new(),
            name: "output".into(),
            format: ImgFormat::Jpeg,
            dpi: 150,
            log: vec![(
                theme::LOG_INFO,
                "Pronto. Aggiungi dei file o trascinali qui sopra.".into(),
            )],
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            global: (0, 0),
            item: (0, 0),
            busy: false,
            summary: None,
        }
    }

    fn apply(&mut self, m: Msg) {
        match m {
            Msg::Log(l, t) => self.log.push((level_color(l), t)),
            Msg::Global(d, t) => self.global = (d, t),
            Msg::Item(d, t) => self.item = (d, t),
            Msg::Finished {
                ok,
                failed,
                cancelled,
            } => {
                self.busy = false;
                self.rx = None;
                let text = if cancelled {
                    format!("Annullato: {ok} completati, {failed} con errori")
                } else {
                    format!("Finito: {ok} completati, {failed} con errori")
                };
                let color = if failed > 0 {
                    theme::LOG_WARN
                } else {
                    theme::LOG_OK
                };
                self.log.push((color, text.clone()));
                self.summary = Some(text);
            }
        }
    }

    fn poll(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            self.apply(m);
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(out_dir) = effective_out_dir(&self.out_dir, &self.inputs) else {
            self.log
                .push((theme::LOG_ERR, "Scegli la cartella di destinazione.".into()));
            return;
        };
        let params = JobParams {
            mode: self.mode,
            inputs: self.inputs.clone(),
            out_dir,
            name: self.name.clone(),
            format: self.format,
            dpi: self.dpi,
        };
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.summary = None;
        self.global = (0, params.inputs.len());
        self.item = (0, 0);
        self.log
            .push((theme::LOG_INFO, format!("— {} —", self.mode.label())));
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            run_job(
                &params,
                &mut |m| {
                    let _ = tx2.send(m);
                    ctx.request_repaint();
                },
                &cancel,
            );
        });
    }

    fn big_button(ui: &mut egui::Ui, text: &str, color: Color32, enabled: bool) -> bool {
        let b = egui::Button::new(
            RichText::new(text)
                .size(17.0)
                .strong()
                .color(theme::ON_COLOR),
        )
        .fill(color)
        .min_size(egui::vec2(190.0, 40.0));
        ui.add_enabled(enabled, b).clicked()
    }

    fn add_files(&mut self) {
        let exts = self.mode.input_exts();
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("File supportati", exts)
            .pick_files()
        {
            let n = add_paths(&mut self.inputs, &files, self.mode);
            self.log
                .push((theme::LOG_INFO, format!("{n} file aggiunti")));
        }
    }

    fn add_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let n = add_paths(&mut self.inputs, &[dir], self.mode);
            self.log
                .push((theme::LOG_INFO, format!("{n} file trovati nella cartella")));
        }
    }

    fn mode_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for m in Mode::ALL {
                let sel = self.mode == m;
                let b = egui::Button::new(
                    RichText::new(m.label())
                        .size(15.0)
                        .strong()
                        .color(theme::ON_COLOR),
                )
                .fill(if sel {
                    theme::SCAN
                } else {
                    theme::PRIMARY_LIGHT
                })
                .stroke(egui::Stroke::new(
                    if sel { 2.0 } else { 1.0 },
                    if sel { theme::PRIMARY } else { theme::BORDER },
                ));
                if ui.add_enabled(!self.busy, b).clicked() && !sel {
                    self.mode = m;
                    // Cambiando modalità si tengono solo i file adatti.
                    let exts = m.input_exts();
                    self.inputs.retain(|p| crate::files::has_ext(p, exts));
                }
            }
        });
    }

    fn files_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let idle = !self.busy;
            if Self::big_button(ui, "Aggiungi file…", theme::SCAN, idle) {
                self.add_files();
            }
            if Self::big_button(ui, "Aggiungi cartella…", theme::DOWNLOAD, idle) {
                self.add_folder();
            }
            if ui
                .add_enabled(
                    idle && !self.inputs.is_empty(),
                    egui::Button::new("Svuota elenco"),
                )
                .clicked()
            {
                self.inputs.clear();
            }
            ui.label(RichText::new(format!("{} file", self.inputs.len())).color(theme::MUTED));
        });
        ui.add_space(4.0);
        let mut remove = None;
        egui::Frame::new()
            .fill(theme::SURFACE)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(9)
            .inner_margin(6.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(230.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.inputs.is_empty() {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Trascina qui i file o le cartelle")
                                        .size(16.0)
                                        .color(theme::MUTED),
                                );
                            });
                            ui.add_space(24.0);
                        }
                        for (i, p) in self.inputs.iter().enumerate() {
                            let row = egui::Frame::new()
                                .fill(if i % 2 == 1 {
                                    theme::STRIPE
                                } else {
                                    theme::SURFACE
                                })
                                .inner_margin(3.0);
                            row.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!self.busy, egui::Button::new("×").small())
                                        .clicked()
                                    {
                                        remove = Some(i);
                                    }
                                    ui.label(p.display().to_string());
                                });
                            });
                        }
                    });
            });
        if let Some(i) = remove {
            self.inputs.remove(i);
        }
    }

    fn options_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Cartella di destinazione:");
                ui.add(egui::TextEdit::singleline(&mut self.out_dir).desired_width(380.0).hint_text("come la cartella del primo file"));
                if ui.button("Sfoglia…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = d.display().to_string();
                    }
                }
            });
            match self.mode {
                Mode::ImagesToOne => {
                    ui.horizontal(|ui| {
                        ui.label("Nome del PDF:");
                        ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(240.0));
                        ui.label(RichText::new(".pdf").color(theme::MUTED));
                    });
                }
                Mode::PdfToImages => {
                    ui.horizontal(|ui| {
                        ui.label("Formato:");
                        for f in ImgFormat::ALL {
                            ui.selectable_value(&mut self.format, f, f.label());
                        }
                        ui.add_space(16.0);
                        ui.label("Risoluzione:");
                        ui.add(egui::DragValue::new(&mut self.dpi).range(36..=600).suffix(" dpi"));
                    });
                }
                Mode::Books => {
                    ui.label(
                        RichText::new("EPUB: copertina + capitoli su A4. Fumetti CBZ/CBR/CB7: una pagina per immagine. I CBR richiedono 7-Zip installato.")
                            .color(theme::MUTED),
                    );
                }
                Mode::ImagesToMany => {}
            }
        });
    }

    fn progress_panel(&mut self, ui: &mut egui::Ui) {
        let frac = |(d, t): (usize, usize)| if t == 0 { 0.0 } else { d as f32 / t as f32 };
        ui.horizontal(|ui| {
            ui.label("Totale:  ");
            ui.add(
                egui::ProgressBar::new(frac(self.global))
                    .fill(theme::PRIMARY)
                    .text(format!("{}/{}", self.global.0, self.global.1))
                    .desired_width(ui.available_width()),
            );
        });
        ui.horizontal(|ui| {
            ui.label("File:      ");
            ui.add(
                egui::ProgressBar::new(frac(self.item))
                    .fill(theme::DOWNLOAD)
                    .text(format!("{}/{}", self.item.0, self.item.1))
                    .desired_width(ui.available_width()),
            );
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if !self.busy {
            let dropped: Vec<PathBuf> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect()
            });
            if !dropped.is_empty() {
                let n = add_paths(&mut self.inputs, &dropped, self.mode);
                self.log.push((
                    theme::LOG_INFO,
                    format!("{n} file aggiunti dal trascinamento"),
                ));
            }
        }
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("{APP_TITLE}  ·  libri, fumetti, immagini e PDF"))
                    .size(20.0)
                    .strong()
                    .color(theme::PRIMARY),
            );
            ui.add_space(4.0);
            self.mode_bar(ui);
            ui.add_space(6.0);
        });
        egui::Panel::bottom("log").show(ui, |ui| {
            egui::Frame::new()
                .fill(theme::LOG_BG)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .auto_shrink([false, true])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for (c, m) in &self.log {
                                ui.label(RichText::new(m).color(*c).monospace());
                            }
                        });
                });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.files_panel(ui);
            ui.add_space(8.0);
            self.options_panel(ui);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if self.busy {
                    if Self::big_button(ui, "Annulla", theme::INSTALL, true) {
                        self.cancel.store(true, Ordering::Relaxed);
                    }
                } else if Self::big_button(ui, "Converti", theme::DOWNLOAD, !self.inputs.is_empty())
                {
                    self.start(&ctx);
                }
                if let Some(s) = &self.summary {
                    ui.label(RichText::new(s).strong().color(theme::OK));
                }
            });
            ui.add_space(8.0);
            self.progress_panel(ui);
        });
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_paths_filters_dedups_and_scans_folders() {
        let d = std::env::temp_dir().join(format!("pdftool_a_{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.epub"), b"x").unwrap();
        std::fs::write(d.join("sub/b.CBZ"), b"x").unwrap();
        std::fs::write(d.join("c.txt"), b"x").unwrap();
        let mut list = Vec::new();
        assert_eq!(
            add_paths(&mut list, std::slice::from_ref(&d), Mode::Books),
            2
        );
        assert_eq!(
            add_paths(&mut list, &[d.join("a.epub"), d.join("c.txt")], Mode::Books),
            0
        );
        assert_eq!(
            add_paths(&mut list, &[d.join("c.txt")], Mode::ImagesToOne),
            0
        );
        assert_eq!(list.len(), 2);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn out_dir_defaults_to_first_input_folder() {
        let inputs = vec![PathBuf::from("/x/y/a.epub")];
        assert_eq!(
            effective_out_dir("  ", &inputs),
            Some(PathBuf::from("/x/y"))
        );
        assert_eq!(effective_out_dir("/o", &inputs), Some(PathBuf::from("/o")));
        assert_eq!(effective_out_dir("", &[]), None);
    }

    #[test]
    fn modes_cover_their_inputs() {
        assert!(Mode::Books.input_exts().contains(&"cb7"));
        assert!(Mode::PdfToImages.input_exts().contains(&"pdf"));
        assert!(Mode::ImagesToMany.input_exts().contains(&"webp"));
    }
}

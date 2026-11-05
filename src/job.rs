//! Lavori di conversione: eseguiti in un thread, comunicano con messaggi.

use crate::book::epub_to_pdf;
use crate::comics::Comic;
use crate::epub::Epub;
use crate::files::{has_ext, safe_name, unique_path, IMAGE_EXTS};
use crate::fonts::FontSet;
use crate::pdfimg::{pdf_to_images, ImgFormat};
use crate::pdfout::{prepare_image, PdfWriter, CM};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Books,
    ImagesToOne,
    ImagesToMany,
    PdfToImages,
}

impl Mode {
    pub const ALL: [Mode; 4] = [
        Mode::Books,
        Mode::ImagesToOne,
        Mode::ImagesToMany,
        Mode::PdfToImages,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Books => "Libri e fumetti in PDF",
            Mode::ImagesToOne => "Immagini in un PDF unico",
            Mode::ImagesToMany => "Immagini in un PDF per immagine",
            Mode::PdfToImages => "PDF in immagini",
        }
    }

    /// Estensioni accettate in ingresso.
    pub fn input_exts(self) -> &'static [&'static str] {
        match self {
            Mode::Books => &["epub", "cbz", "cbr", "cb7"],
            Mode::ImagesToOne | Mode::ImagesToMany => IMAGE_EXTS,
            Mode::PdfToImages => &["pdf"],
        }
    }
}

#[derive(Clone, Debug)]
pub struct JobParams {
    pub mode: Mode,
    pub inputs: Vec<PathBuf>,
    pub out_dir: PathBuf,
    /// Nome del PDF unico (solo `ImagesToOne`).
    pub name: String,
    pub format: ImgFormat,
    pub dpi: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Err,
}

#[derive(Clone, Debug)]
pub enum Msg {
    Log(Level, String),
    /// Avanzamento globale: elementi finiti / totali.
    Global(usize, usize),
    /// Avanzamento del file corrente.
    Item(usize, usize),
    Finished {
        ok: usize,
        failed: usize,
        cancelled: bool,
    },
}

fn file_label(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn write_pdf(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("scrittura {}: {e}", path.display()))
}

/// Esegue il lavoro. `sink` riceve i messaggi; `cancel` lo interrompe tra un passo e l'altro.
pub fn run_job(p: &JobParams, sink: &mut dyn FnMut(Msg), cancel: &AtomicBool) {
    let (mut ok, mut failed) = (0usize, 0usize);
    if let Err(e) = std::fs::create_dir_all(&p.out_dir) {
        sink(Msg::Log(
            Level::Err,
            format!("Cartella di uscita non utilizzabile: {e}"),
        ));
        sink(Msg::Finished {
            ok: 0,
            failed: 1,
            cancelled: false,
        });
        return;
    }
    if p.mode == Mode::ImagesToOne {
        match images_to_one(p, sink, cancel) {
            Ok((good, bad)) => {
                ok += good;
                failed += bad;
            }
            Err(e) => {
                if e == "Annullato" {
                    sink(Msg::Log(Level::Warn, "Operazione annullata".into()));
                    sink(Msg::Finished {
                        ok,
                        failed,
                        cancelled: true,
                    });
                    return;
                }
                sink(Msg::Log(Level::Err, e));
                failed += 1;
            }
        }
        sink(Msg::Finished {
            ok,
            failed,
            cancelled: false,
        });
        return;
    }
    let total = p.inputs.len();
    let mut fonts: Option<FontSet> = None;
    sink(Msg::Global(0, total));
    for (i, input) in p.inputs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            sink(Msg::Log(Level::Warn, "Operazione annullata".into()));
            sink(Msg::Finished {
                ok,
                failed,
                cancelled: true,
            });
            return;
        }
        let label = file_label(input);
        sink(Msg::Item(0, 1));
        sink(Msg::Log(Level::Info, format!("… {label}")));
        let r = match p.mode {
            Mode::Books => book_file(p, input, &mut fonts, sink, cancel),
            Mode::ImagesToMany => image_file(p, input),
            Mode::PdfToImages => pdf_to_images(
                input,
                &p.out_dir,
                p.format,
                p.dpi,
                &mut |d, t| sink(Msg::Item(d, t)),
                cancel,
            )
            .map(|n| format!("{n} pagine")),
            Mode::ImagesToOne => Ok(String::new()),
        };
        match r {
            Ok(info) => {
                ok += 1;
                sink(Msg::Log(Level::Ok, format!("OK  {label}: {info}")));
            }
            Err(e) if e == "Annullato" => {
                sink(Msg::Log(Level::Warn, "Operazione annullata".into()));
                sink(Msg::Finished {
                    ok,
                    failed,
                    cancelled: true,
                });
                return;
            }
            Err(e) => {
                failed += 1;
                sink(Msg::Log(Level::Err, format!("ERRORE  {label}: {e}")));
            }
        }
        sink(Msg::Item(1, 1));
        sink(Msg::Global(i + 1, total));
    }
    sink(Msg::Finished {
        ok,
        failed,
        cancelled: false,
    });
}

fn book_file(
    p: &JobParams,
    input: &Path,
    fonts: &mut Option<FontSet>,
    sink: &mut dyn FnMut(Msg),
    cancel: &AtomicBool,
) -> Result<String, String> {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = unique_path(&p.out_dir, &safe_name(&stem), "pdf");
    if has_ext(input, &["epub"]) {
        if fonts.is_none() {
            *fonts = Some(FontSet::load()?);
        }
        let fs = fonts.as_mut().ok_or("caratteri non disponibili")?;
        let mut epub = Epub::open(input)?;
        let bytes = epub_to_pdf(&mut epub, fs, &mut |d, t| sink(Msg::Item(d, t)), cancel)?;
        write_pdf(&target, &bytes)?;
        Ok(format!(
            "{} capitoli, creato {}",
            epub.chapter_count(),
            file_label(&target)
        ))
    } else {
        let mut comic = Comic::open(input)?;
        let n = comic.len();
        let mut w = PdfWriter::new();
        w.set_info(&stem, &[]);
        let mut skipped = 0;
        for i in 0..n {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            let page = comic.page(i).and_then(|b| prepare_image(&b));
            match page {
                Ok(img) => w.add_full_page_image(&img, 150.0)?,
                Err(_) => skipped += 1, // immagine non valida: saltata, come nell'originale
            }
            sink(Msg::Item(i + 1, n));
        }
        if skipped == n {
            return Err("nessuna immagine valida".into());
        }
        write_pdf(&target, &w.finish()?)?;
        let extra = if skipped > 0 {
            format!(", {skipped} immagini saltate")
        } else {
            String::new()
        };
        Ok(format!(
            "{} pagine{extra}, creato {}",
            n - skipped,
            file_label(&target)
        ))
    }
}

fn image_file(p: &JobParams, input: &Path) -> Result<String, String> {
    let bytes = std::fs::read(input).map_err(|e| format!("lettura: {e}"))?;
    let img = prepare_image(&bytes)?;
    let mut w = PdfWriter::new();
    w.add_a4_image(&img, 1.5 * CM)?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = unique_path(&p.out_dir, &safe_name(&stem), "pdf");
    write_pdf(&target, &w.finish()?)?;
    Ok(file_label(&target))
}

/// Restituisce (immagini inserite, immagini saltate).
fn images_to_one(
    p: &JobParams,
    sink: &mut dyn FnMut(Msg),
    cancel: &AtomicBool,
) -> Result<(usize, usize), String> {
    let total = p.inputs.len();
    let mut w = PdfWriter::new();
    let name = safe_name(&p.name);
    w.set_info(&name, &[]);
    let (mut good, mut bad) = (0, 0);
    sink(Msg::Global(0, total));
    for (i, input) in p.inputs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Annullato".into());
        }
        let r = std::fs::read(input)
            .map_err(|e| e.to_string())
            .and_then(|b| prepare_image(&b))
            .and_then(|img| w.add_a4_image(&img, 1.5 * CM));
        match r {
            Ok(()) => good += 1,
            Err(e) => {
                bad += 1;
                sink(Msg::Log(
                    Level::Warn,
                    format!("Saltata {}: {e}", file_label(input)),
                ));
            }
        }
        sink(Msg::Global(i + 1, total));
    }
    if good == 0 {
        return Err("nessuna immagine valida da inserire nel PDF".into());
    }
    let target = unique_path(&p.out_dir, &name, "pdf");
    write_pdf(&target, &w.finish()?)?;
    sink(Msg::Log(
        Level::Ok,
        format!("OK  {}: {good} immagini", file_label(&target)),
    ));
    Ok((good, bad))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    struct Env(PathBuf);
    impl Env {
        fn new(tag: &str) -> Env {
            let d = std::env::temp_dir().join(format!("pdftool_j_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Env(d)
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn png_bytes(c: u8) -> Vec<u8> {
        let mut v = Vec::new();
        image::RgbImage::from_pixel(30, 20, image::Rgb([c, 50, 50]))
            .write_to(&mut Cursor::new(&mut v), image::ImageFormat::Png)
            .unwrap();
        v
    }

    fn params(env: &Env, mode: Mode, inputs: Vec<PathBuf>) -> JobParams {
        JobParams {
            mode,
            inputs,
            out_dir: env.0.join("out"),
            name: "unico".into(),
            format: ImgFormat::Png,
            dpi: 72,
        }
    }

    fn run(p: &JobParams, cancel: bool) -> Vec<Msg> {
        let mut msgs = Vec::new();
        run_job(p, &mut |m| msgs.push(m), &AtomicBool::new(cancel));
        msgs
    }

    fn pages(path: &Path) -> usize {
        hayro::Pdf::new(std::sync::Arc::new(std::fs::read(path).unwrap()))
            .unwrap()
            .pages()
            .len()
    }

    fn write_images(env: &Env, n: u8) -> Vec<PathBuf> {
        (0..n)
            .map(|i| {
                let p = env.0.join(format!("i{i}.png"));
                std::fs::write(&p, png_bytes(i * 40)).unwrap();
                p
            })
            .collect()
    }

    #[test]
    fn images_to_one_skips_bad_files() {
        let env = Env::new("one");
        let mut files = write_images(&env, 3);
        let bad = env.0.join("rotta.png");
        std::fs::write(&bad, b"no").unwrap();
        files.push(bad);
        let msgs = run(&params(&env, Mode::ImagesToOne, files), false);
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 3,
                failed: 1,
                cancelled: false
            })
        ));
        assert_eq!(pages(&env.0.join("out/unico.pdf")), 3);
    }

    #[test]
    fn images_to_one_all_bad_is_an_error() {
        let env = Env::new("bad");
        let bad = env.0.join("x.png");
        std::fs::write(&bad, b"no").unwrap();
        let msgs = run(&params(&env, Mode::ImagesToOne, vec![bad]), false);
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 0,
                failed: 1,
                ..
            })
        ));
    }

    #[test]
    fn images_to_many_makes_one_pdf_each_without_overwriting() {
        let env = Env::new("many");
        let files = write_images(&env, 2);
        let p = params(&env, Mode::ImagesToMany, files);
        run(&p, false);
        let msgs = run(&p, false); // secondo giro: nomi con (2)
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 2,
                failed: 0,
                ..
            })
        ));
        assert_eq!(pages(&env.0.join("out/i0.pdf")), 1);
        assert_eq!(pages(&env.0.join("out/i0 (2).pdf")), 1);
    }

    #[test]
    fn comic_to_pdf_and_back_to_images() {
        let env = Env::new("comic");
        let cbz = env.0.join("fumetto.cbz");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for (n, c) in [("2.png", 80u8), ("1.png", 10)] {
                z.start_file(n, o).unwrap();
                z.write_all(&png_bytes(c)).unwrap();
            }
            z.start_file("3.png", o).unwrap();
            z.write_all(b"corrotta").unwrap();
            z.finish().unwrap();
        }
        let msgs = run(&params(&env, Mode::Books, vec![cbz]), false);
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 1,
                failed: 0,
                ..
            })
        ));
        let pdf = env.0.join("out/fumetto.pdf");
        assert_eq!(pages(&pdf), 2);
        let back = JobParams {
            out_dir: env.0.join("img"),
            ..params(&env, Mode::PdfToImages, vec![pdf])
        };
        let msgs = run(&back, false);
        assert!(matches!(msgs.last(), Some(Msg::Finished { ok: 1, .. })));
        assert!(env.0.join("img/fumetto_pag002.png").exists());
    }

    #[test]
    fn epub_to_pdf_job() {
        if FontSet::load().is_err() {
            return;
        }
        let env = Env::new("epub");
        let f = env.0.join("libro.epub");
        std::fs::write(&f, crate::epub::tests::sample_epub()).unwrap();
        let msgs = run(&params(&env, Mode::Books, vec![f]), false);
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 1,
                failed: 0,
                ..
            })
        ));
        assert_eq!(pages(&env.0.join("out/libro.pdf")), 3);
    }

    #[test]
    fn errors_do_not_stop_the_batch_and_cancel_works() {
        let env = Env::new("batch");
        let junk = env.0.join("a.cbz");
        std::fs::write(&junk, b"x").unwrap();
        let junk2 = env.0.join("b.epub");
        std::fs::write(&junk2, b"x").unwrap();
        let msgs = run(
            &params(&env, Mode::Books, vec![junk.clone(), junk2.clone()]),
            false,
        );
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                ok: 0,
                failed: 2,
                cancelled: false
            })
        ));
        let msgs = run(&params(&env, Mode::Books, vec![junk, junk2]), true);
        assert!(matches!(
            msgs.last(),
            Some(Msg::Finished {
                cancelled: true,
                ..
            })
        ));
    }

    #[test]
    fn unusable_output_dir() {
        let env = Env::new("outdir");
        let file = env.0.join("file");
        std::fs::write(&file, b"x").unwrap();
        let p = JobParams {
            out_dir: file.join("sotto"),
            ..params(&env, Mode::Books, vec![])
        };
        let msgs = run(&p, false);
        assert!(matches!(msgs.last(), Some(Msg::Finished { failed: 1, .. })));
    }
}

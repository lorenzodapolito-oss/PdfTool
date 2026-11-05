//! EPUB → PDF: copertina, capitoli con titoli, paragrafi, citazioni, elenchi e immagini.

use crate::epub::{Block, Epub};
use crate::fonts::{FontSet, Style};
use crate::pdfout::{fit_scale, prepare_image, PageOp, PdfWriter, A4_H, A4_W, CM};
use std::sync::atomic::{AtomicBool, Ordering};

const MARGIN: f32 = 2.5 * CM;
const MAX_IMG_W: f32 = 14.0 * CM;
const MAX_IMG_H: f32 = 19.0 * CM;

struct Layout<'a> {
    fonts: &'a mut FontSet,
    ops: Vec<PageOp>,
    y: f32,
}

impl Layout<'_> {
    fn bottom() -> f32 {
        A4_H - MARGIN
    }

    fn flush_page(&mut self, w: &mut PdfWriter) -> Result<(), String> {
        if !self.ops.is_empty() {
            w.add_ops_page(&self.ops)?;
            self.ops.clear();
        }
        self.y = MARGIN;
        Ok(())
    }

    /// Spezza il testo in righe che stanno nella larghezza `width`.
    fn wrap(&mut self, text: &str, style: Style, size: f32, width: f32) -> Vec<String> {
        let face = self.fonts.face(style);
        let clean = face.sanitize(text);
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in clean.split_whitespace() {
            let cand = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if face.width(&cand, size) <= width {
                line = cand;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // Parola più larga di una riga: la spezza per caratteri.
            let mut piece = String::new();
            for c in word.chars() {
                let mut t = piece.clone();
                t.push(c);
                if face.width(&t, size) > width && !piece.is_empty() {
                    lines.push(std::mem::take(&mut piece));
                }
                piece.push(c);
            }
            line = piece;
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    fn paragraph(
        &mut self,
        w: &mut PdfWriter,
        text: &str,
        style: Style,
        size: f32,
        indent: f32,
        after: f32,
    ) -> Result<(), String> {
        let leading = size * 1.3;
        let width = A4_W - 2.0 * MARGIN - indent;
        for line in self.wrap(text, style, size, width) {
            if self.y + leading > Self::bottom() {
                self.flush_page(w)?;
            }
            self.y += leading;
            let font = self.fonts.face(style).font.clone();
            self.ops.push(PageOp::Text {
                x: MARGIN + indent,
                y: self.y - size * 0.25,
                font,
                size,
                text: line,
            });
        }
        self.y += after;
        Ok(())
    }

    fn block(&mut self, w: &mut PdfWriter, b: &Block) -> Result<(), String> {
        match b {
            Block::Heading(1, t) => self.paragraph(w, t, Style::Bold, 22.0, 0.0, 12.0),
            Block::Heading(2, t) => self.paragraph(w, t, Style::Bold, 17.0, 0.0, 10.0),
            Block::Heading(_, t) => self.paragraph(w, t, Style::Bold, 13.0, 0.0, 8.0),
            Block::Para(t) => self.paragraph(w, t, Style::Regular, 11.0, 0.0, 6.0),
            Block::Quote(t) => self.paragraph(w, t, Style::Italic, 10.0, CM, 6.0),
            Block::Item(t) => self.paragraph(
                w,
                &format!("\u{2022} {t}"),
                Style::Regular,
                11.0,
                0.6 * CM,
                3.0,
            ),
            Block::Rule => {
                if self.y + 8.0 > Self::bottom() {
                    self.flush_page(w)?;
                }
                self.ops.push(PageOp::Rule {
                    x: MARGIN,
                    y: self.y + 4.0,
                    w: A4_W - 2.0 * MARGIN,
                });
                self.y += 10.0;
                Ok(())
            }
            Block::Image(bytes) => {
                // Un'immagine illeggibile viene saltata senza fermare il libro.
                let Ok(img) = prepare_image(bytes) else {
                    return Ok(());
                };
                let s = fit_scale(img.width as f32, img.height as f32, MAX_IMG_W, MAX_IMG_H);
                let (dw, dh) = (img.width as f32 * s, img.height as f32 * s);
                if self.y + dh > Self::bottom() {
                    self.flush_page(w)?;
                }
                self.ops.push(PageOp::Image {
                    img,
                    x: (A4_W - dw) / 2.0,
                    y: self.y,
                    w: dw,
                    h: dh,
                });
                self.y += dh + 8.0;
                Ok(())
            }
        }
    }
}

/// Converte l'EPUB e restituisce i byte del PDF.
/// `progress(fatti, totali)` è chiamato a ogni capitolo; `cancel` interrompe.
pub fn epub_to_pdf(
    epub: &mut Epub,
    fonts: &mut FontSet,
    progress: &mut dyn FnMut(usize, usize),
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut w = PdfWriter::new();
    w.set_info(&epub.title, &epub.authors);
    let mut lay = Layout {
        fonts,
        ops: Vec::new(),
        y: MARGIN,
    };
    // Copertina con titolo e autori.
    lay.y = A4_H * 0.3;
    let title = if epub.title.is_empty() {
        "Senza titolo".to_string()
    } else {
        epub.title.clone()
    };
    lay.paragraph(&mut w, &title, Style::Bold, 26.0, 0.0, 18.0)?;
    if !epub.authors.is_empty() {
        let a = epub.authors.join(", ");
        lay.paragraph(&mut w, &a, Style::Regular, 14.0, 0.0, 0.0)?;
    }
    lay.flush_page(&mut w)?;
    let total = epub.chapter_count();
    for i in 0..total {
        if cancel.load(Ordering::Relaxed) {
            return Err("Annullato".into());
        }
        for b in epub.chapter(i)? {
            lay.block(&mut w, &b)?;
        }
        lay.flush_page(&mut w)?; // un capitolo per volta, ognuno su pagina nuova
        progress(i + 1, total);
    }
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::tests::sample_epub;

    #[test]
    fn converts_sample_epub() {
        let Ok(mut fonts) = FontSet::load() else {
            return;
        };
        let mut e = Epub::from_bytes(sample_epub()).unwrap();
        let mut steps = Vec::new();
        let pdf = epub_to_pdf(
            &mut e,
            &mut fonts,
            &mut |d, t| steps.push((d, t)),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(steps, vec![(1, 2), (2, 2)]);
        let doc = hayro::Pdf::new(std::sync::Arc::new(pdf)).unwrap();
        assert_eq!(doc.pages().len(), 3); // copertina + 2 capitoli
    }

    #[test]
    fn long_text_makes_several_pages_and_cancel_works() {
        let Ok(mut fonts) = FontSet::load() else {
            return;
        };
        let mut e = Epub::from_bytes(sample_epub()).unwrap();
        assert!(epub_to_pdf(&mut e, &mut fonts, &mut |_, _| {}, &AtomicBool::new(true)).is_err());
        let mut lay = Layout {
            fonts: &mut fonts,
            ops: Vec::new(),
            y: MARGIN,
        };
        let mut w = PdfWriter::new();
        let long = "parola ".repeat(3000);
        lay.block(&mut w, &Block::Para(long)).unwrap();
        lay.flush_page(&mut w).unwrap();
        let pdf = w.finish().unwrap();
        let doc = hayro::Pdf::new(std::sync::Arc::new(pdf)).unwrap();
        assert!(doc.pages().len() >= 4);
    }

    #[test]
    fn wraps_giant_word() {
        let Ok(mut fonts) = FontSet::load() else {
            return;
        };
        let mut lay = Layout {
            fonts: &mut fonts,
            ops: Vec::new(),
            y: MARGIN,
        };
        let lines = lay.wrap(&"x".repeat(500), Style::Regular, 11.0, 200.0);
        assert!(lines.len() > 1);
    }
}

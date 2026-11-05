//! Scrittura dei PDF con krilla: pagine di immagini e pagine di testo.

use krilla::color::rgb;
use krilla::geom::{Point, Size, Transform};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, TextDirection};
use krilla::Data;
use krilla::Document;

/// Larghezza e altezza A4 in punti.
pub const A4_W: f32 = 595.28;
pub const A4_H: f32 = 841.89;
/// Punti per centimetro.
pub const CM: f32 = 28.3465;

/// Immagine pronta da inserire nel PDF (JPEG o PNG codificato).
pub struct PdfImage {
    pub data: Vec<u8>,
    pub jpeg: bool,
    pub width: u32,
    pub height: u32,
}

/// Decodifica un'immagine qualsiasi e la ricodifica per il PDF:
/// JPEG qualità 92 se opaca, PNG se ha trasparenza.
pub fn prepare_image(bytes: &[u8]) -> Result<PdfImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("immagine non valida: {e}"))?;
    let (width, height) = (img.width(), img.height());
    if width == 0 || height == 0 {
        return Err("immagine vuota".into());
    }
    let mut out = Vec::new();
    let has_alpha = img.color().has_alpha();
    if has_alpha {
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| format!("codifica PNG: {e}"))?;
    } else {
        let rgb_img = img.to_rgb8();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
        rgb_img
            .write_with_encoder(enc)
            .map_err(|e| format!("codifica JPEG: {e}"))?;
    }
    Ok(PdfImage {
        data: out,
        jpeg: !has_alpha,
        width,
        height,
    })
}

fn to_krilla(img: &PdfImage) -> Result<krilla::image::Image, String> {
    let data: Data = img.data.clone().into();
    if img.jpeg {
        krilla::image::Image::from_jpeg(data, true)
    } else {
        krilla::image::Image::from_png(data, true)
    }
}

/// Scala `iw`x`ih` per stare in `max_w`x`max_h`, senza mai ingrandire.
pub fn fit_scale(iw: f32, ih: f32, max_w: f32, max_h: f32) -> f32 {
    if iw <= 0.0 || ih <= 0.0 {
        return 1.0;
    }
    (max_w / iw).min(max_h / ih).min(1.0)
}

/// Dimensione in punti di una pagina fumetto: pixel / dpi * 72.
pub fn comic_page_size(w: u32, h: u32, dpi: f32) -> (f32, f32) {
    (w as f32 / dpi * 72.0, h as f32 / dpi * 72.0)
}

/// Generatore di PDF con pagine in sequenza.
pub struct PdfWriter {
    doc: Document,
}

impl Default for PdfWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfWriter {
    pub fn new() -> Self {
        Self {
            doc: Document::new(),
        }
    }

    pub fn set_info(&mut self, title: &str, authors: &[String]) {
        let mut m = Metadata::new().creator("PdfTool".to_string());
        if !title.is_empty() {
            m = m.title(title.to_string());
        }
        if !authors.is_empty() {
            m = m.authors(authors.to_vec());
        }
        self.doc.set_metadata(m);
    }

    /// Pagina grande quanto l'immagine (fumetti).
    pub fn add_full_page_image(&mut self, img: &PdfImage, dpi: f32) -> Result<(), String> {
        let (w, h) = comic_page_size(img.width, img.height, dpi);
        self.place_image(img, w, h, 0.0, 0.0, w, h)
    }

    /// Pagina A4 con immagine centrata e margine.
    pub fn add_a4_image(&mut self, img: &PdfImage, margin: f32) -> Result<(), String> {
        let (max_w, max_h) = (A4_W - 2.0 * margin, A4_H - 2.0 * margin);
        let s = fit_scale(img.width as f32, img.height as f32, max_w, max_h);
        let (dw, dh) = (img.width as f32 * s, img.height as f32 * s);
        let (x, y) = ((A4_W - dw) / 2.0, (A4_H - dh) / 2.0);
        self.place_image(img, A4_W, A4_H, x, y, dw, dh)
    }

    #[allow(clippy::too_many_arguments)]
    fn place_image(
        &mut self,
        img: &PdfImage,
        page_w: f32,
        page_h: f32,
        x: f32,
        y: f32,
        dw: f32,
        dh: f32,
    ) -> Result<(), String> {
        let settings =
            PageSettings::from_wh(page_w, page_h).ok_or("dimensione pagina non valida")?;
        let size = Size::from_wh(dw, dh).ok_or("dimensione immagine non valida")?;
        let kimg = to_krilla(img)?;
        let mut page = self.doc.start_page_with(settings);
        let mut surface = page.surface();
        surface.push_transform(&Transform::from_translate(x, y));
        surface.draw_image(kimg, size);
        surface.pop();
        surface.finish();
        page.finish();
        Ok(())
    }

    /// Pagina A4 composta da operazioni di testo e immagini (EPUB).
    pub fn add_ops_page(&mut self, ops: &[PageOp]) -> Result<(), String> {
        let settings = PageSettings::from_wh(A4_W, A4_H).ok_or("dimensione pagina non valida")?;
        let mut page = self.doc.start_page_with(settings);
        let mut s = page.surface();
        for op in ops {
            match op {
                PageOp::Text {
                    x,
                    y,
                    font,
                    size,
                    text,
                } => {
                    s.set_fill(Some(Fill {
                        paint: rgb::Color::new(0, 0, 0).into(),
                        ..Fill::default()
                    }));
                    s.draw_text(
                        Point::from_xy(*x, *y),
                        font.clone(),
                        *size,
                        text,
                        false,
                        TextDirection::Auto,
                    );
                }
                PageOp::Image { img, x, y, w, h } => {
                    let size = Size::from_wh(*w, *h).ok_or("dimensione immagine non valida")?;
                    let kimg = to_krilla(img)?;
                    s.push_transform(&Transform::from_translate(*x, *y));
                    s.draw_image(kimg, size);
                    s.pop();
                }
                PageOp::Rule { x, y, w } => {
                    let img = prepare_image(&gray_pixel())?;
                    let size = Size::from_wh(*w, 0.6).ok_or("linea non valida")?;
                    let kimg = to_krilla(&img)?;
                    s.push_transform(&Transform::from_translate(*x, *y));
                    s.draw_image(kimg, size);
                    s.pop();
                }
            }
        }
        s.finish();
        page.finish();
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        self.doc
            .finish()
            .map_err(|e| format!("scrittura PDF: {e:?}"))
    }
}

/// Operazione di disegno su una pagina di testo (coordinate dall'alto a sinistra).
pub enum PageOp {
    Text {
        x: f32,
        y: f32,
        font: Font,
        size: f32,
        text: String,
    },
    Image {
        img: PdfImage,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Rule {
        x: f32,
        y: f32,
        w: f32,
    },
}

fn gray_pixel() -> Vec<u8> {
    let mut out = Vec::new();
    let buf = image::RgbImage::from_pixel(1, 1, image::Rgb([150, 150, 150]));
    let _ = buf.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if alpha {
            image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 128]))
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
        } else {
            image::RgbImage::from_pixel(w, h, image::Rgb([10, 20, 30]))
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
        }
        out
    }

    #[test]
    fn fit_never_enlarges() {
        assert_eq!(fit_scale(100.0, 100.0, 500.0, 500.0), 1.0);
        assert!((fit_scale(1000.0, 500.0, 500.0, 500.0) - 0.5).abs() < 1e-6);
        assert_eq!(fit_scale(0.0, 10.0, 5.0, 5.0), 1.0);
    }

    #[test]
    fn comic_size_uses_150_dpi() {
        let (w, h) = comic_page_size(1500, 750, 150.0);
        assert!((w - 720.0).abs() < 1e-3 && (h - 360.0).abs() < 1e-3);
    }

    #[test]
    fn prepare_picks_format() {
        assert!(prepare_image(&png(4, 4, false)).unwrap().jpeg);
        assert!(!prepare_image(&png(4, 4, true)).unwrap().jpeg);
        assert!(prepare_image(b"non e' un'immagine").is_err());
    }

    #[test]
    fn writes_valid_pdf_with_pages() {
        let mut w = PdfWriter::new();
        w.set_info("Prova", &["Io".to_string()]);
        let a = prepare_image(&png(40, 20, false)).unwrap();
        w.add_a4_image(&a, 1.5 * CM).unwrap();
        w.add_full_page_image(&a, 150.0).unwrap();
        w.add_ops_page(&[PageOp::Rule {
            x: 10.0,
            y: 10.0,
            w: 100.0,
        }])
        .unwrap();
        let bytes = w.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let pdf = hayro::Pdf::new(std::sync::Arc::new(bytes)).unwrap();
        assert_eq!(pdf.pages().len(), 3);
    }
}

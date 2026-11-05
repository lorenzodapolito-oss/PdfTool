//! PDF → immagini (una per pagina) con il rasterizzatore hayro.

use crate::files::{safe_name, unique_path};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImgFormat {
    Jpeg,
    Png,
    Tiff,
}

impl ImgFormat {
    pub const ALL: [ImgFormat; 3] = [ImgFormat::Jpeg, ImgFormat::Png, ImgFormat::Tiff];

    pub fn ext(self) -> &'static str {
        match self {
            ImgFormat::Jpeg => "jpg",
            ImgFormat::Png => "png",
            ImgFormat::Tiff => "tif",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ImgFormat::Jpeg => "JPEG",
            ImgFormat::Png => "PNG",
            ImgFormat::Tiff => "TIFF",
        }
    }
}

/// Lato massimo in pixel (hayro usa interi a 16 bit).
const MAX_SIDE: f32 = 16_000.0;

/// Nome del file di una pagina: `<stem>_pag001.jpg`.
pub fn page_file_name(stem: &str, page: usize, fmt: ImgFormat) -> String {
    format!("{}_pag{:03}.{}", safe_name(stem), page, fmt.ext())
}

/// Disegna tutte le pagine; `progress(fatte, totali)` dopo ogni pagina. Restituisce le pagine scritte.
pub fn pdf_to_images(
    pdf_path: &Path,
    out_dir: &Path,
    fmt: ImgFormat,
    dpi: u32,
    progress: &mut dyn FnMut(usize, usize),
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let data = std::fs::read(pdf_path).map_err(|e| format!("lettura: {e}"))?;
    let pdf = hayro::Pdf::new(Arc::new(data))
        .map_err(|e| format!("PDF non leggibile (protetto da password?): {e:?}"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("cartella di uscita: {e}"))?;
    let stem = pdf_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pages = pdf.pages();
    let total = pages.len();
    let settings = hayro::InterpreterSettings::default();
    let dpi = dpi.clamp(36, 600) as f32;
    for (i, page) in pages.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Annullato".into());
        }
        let (pw, ph) = page.render_dimensions();
        let mut scale = dpi / 72.0;
        let longest = pw.max(ph) * scale;
        if longest > MAX_SIDE {
            scale *= MAX_SIDE / longest;
        }
        let pix = hayro::render(
            page,
            &settings,
            &hayro::RenderSettings {
                x_scale: scale,
                y_scale: scale,
                ..Default::default()
            },
        );
        let (w, h) = (u32::from(pix.width()), u32::from(pix.height()));
        let rgba = pix.take_u8();
        let img =
            image::RgbaImage::from_raw(w, h, rgba).ok_or("pagina vuota o dimensioni non valide")?;
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let name = page_file_name(&stem, i + 1, fmt);
        let path = unique_path(
            out_dir,
            name.trim_end_matches(&format!(".{}", fmt.ext())),
            fmt.ext(),
        );
        match fmt {
            ImgFormat::Jpeg => {
                let mut f =
                    std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 92);
                rgb.write_with_encoder(enc)
                    .map_err(|e| format!("JPEG: {e}"))?;
            }
            ImgFormat::Png => rgb
                .save_with_format(&path, image::ImageFormat::Png)
                .map_err(|e| format!("PNG: {e}"))?,
            ImgFormat::Tiff => rgb
                .save_with_format(&path, image::ImageFormat::Tiff)
                .map_err(|e| format!("TIFF: {e}"))?,
        }
        progress(i + 1, total);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdfout::{prepare_image, PdfWriter, CM};

    fn sample_pdf(path: &Path, pages: usize) {
        let mut png = Vec::new();
        image::RgbImage::from_pixel(60, 40, image::Rgb([200, 30, 30]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let img = prepare_image(&png).unwrap();
        let mut w = PdfWriter::new();
        for _ in 0..pages {
            w.add_a4_image(&img, 1.5 * CM).unwrap();
        }
        std::fs::write(path, w.finish().unwrap()).unwrap();
    }

    #[test]
    fn names() {
        assert_eq!(page_file_name("doc", 7, ImgFormat::Jpeg), "doc_pag007.jpg");
        assert_eq!(
            page_file_name("a/b", 1234, ImgFormat::Tiff),
            "a_b_pag1234.tif"
        );
    }

    #[test]
    fn renders_every_page_in_each_format() {
        let d = std::env::temp_dir().join(format!("pdftool_r_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pdf = d.join("doc.pdf");
        sample_pdf(&pdf, 2);
        for fmt in ImgFormat::ALL {
            let out = d.join(fmt.label());
            let mut steps = Vec::new();
            let n = pdf_to_images(
                &pdf,
                &out,
                fmt,
                72,
                &mut |a, b| steps.push((a, b)),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(n, 2);
            assert_eq!(steps, vec![(1, 2), (2, 2)]);
            let f = out.join(page_file_name("doc", 1, fmt));
            let img = image::open(&f).unwrap();
            assert_eq!((img.width(), img.height()), (595, 841));
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cancel_and_bad_pdf() {
        let d = std::env::temp_dir().join(format!("pdftool_x_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let bad = d.join("bad.pdf");
        std::fs::write(&bad, b"non un pdf").unwrap();
        assert!(pdf_to_images(
            &bad,
            &d,
            ImgFormat::Png,
            72,
            &mut |_, _| {},
            &AtomicBool::new(false)
        )
        .is_err());
        let ok = d.join("ok.pdf");
        sample_pdf(&ok, 1);
        assert!(pdf_to_images(
            &ok,
            &d,
            ImgFormat::Png,
            72,
            &mut |_, _| {},
            &AtomicBool::new(true)
        )
        .is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}

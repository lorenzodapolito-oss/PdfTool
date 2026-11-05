//! Caratteri di sistema per il testo degli EPUB, con le misure per l'a capo.

use krilla::text::Font;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Regular,
    Bold,
    Italic,
}

const REGULAR: &[&str] = &[
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/Library/Fonts/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];
const BOLD: &[&str] = &[
    "C:\\Windows\\Fonts\\arialbd.ttf",
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
];
const ITALIC: &[&str] = &[
    "C:\\Windows\\Fonts\\ariali.ttf",
    "C:\\Windows\\Fonts\\segoeuii.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Oblique.ttf",
    "/Library/Fonts/Arial Italic.ttf",
    "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
];

/// Un carattere con le sue misure.
pub struct Face {
    pub font: Font,
    bytes: Vec<u8>,
    units: f32,
    cache: HashMap<char, Option<f32>>,
}

impl Face {
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Face> {
        let parsed = ttf_parser::Face::parse(&bytes, 0).ok()?;
        let units = f32::from(parsed.units_per_em()).max(1.0);
        let font = Font::new(bytes.clone().into(), 0)?;
        Some(Face {
            font,
            bytes,
            units,
            cache: HashMap::new(),
        })
    }

    /// Avanzamento del carattere in em; `None` se il carattere non esiste nel font.
    fn advance_em(&mut self, c: char) -> Option<f32> {
        if let Some(v) = self.cache.get(&c) {
            return *v;
        }
        let v = ttf_parser::Face::parse(&self.bytes, 0).ok().and_then(|f| {
            let g = f.glyph_index(c)?;
            Some(f32::from(f.glyph_hor_advance(g)?) / self.units)
        });
        self.cache.insert(c, v);
        v
    }

    /// Sostituisce i caratteri mancanti nel font (o di controllo) per non stampare riquadri.
    pub fn sanitize(&mut self, s: &str) -> String {
        s.chars()
            .map(|c| {
                if c == '\u{a0}' || c == '\t' {
                    ' '
                } else if c.is_control() || self.advance_em(c).is_none() {
                    '?'
                } else {
                    c
                }
            })
            .collect()
    }

    /// Larghezza in punti del testo alla dimensione data.
    pub fn width(&mut self, s: &str, size: f32) -> f32 {
        s.chars()
            .map(|c| self.advance_em(c).unwrap_or(0.5))
            .sum::<f32>()
            * size
    }
}

pub struct FontSet {
    pub regular: Face,
    pub bold: Face,
    pub italic: Face,
}

fn first_face(paths: &[&str]) -> Option<Face> {
    paths
        .iter()
        .find_map(|p| std::fs::read(p).ok().and_then(Face::from_bytes))
}

impl FontSet {
    /// Cerca i caratteri di sistema; grassetto e corsivo ricadono sul normale se mancano.
    pub fn load() -> Result<FontSet, String> {
        let regular = first_face(REGULAR).ok_or(
            "nessun carattere di sistema trovato (serve Arial o simile) per scrivere il testo",
        )?;
        let bold = first_face(BOLD)
            .or_else(|| first_face(REGULAR))
            .ok_or("carattere grassetto mancante")?;
        let italic = first_face(ITALIC)
            .or_else(|| first_face(REGULAR))
            .ok_or("carattere corsivo mancante")?;
        Ok(FontSet {
            regular,
            bold,
            italic,
        })
    }

    pub fn face(&mut self, style: Style) -> &mut Face {
        match style {
            Style::Regular => &mut self.regular,
            Style::Bold => &mut self.bold,
            Style::Italic => &mut self.italic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_sanitizes() {
        let Ok(mut set) = FontSet::load() else {
            return; // nessun font nel sistema di test
        };
        let w1 = set.regular.width("a", 10.0);
        let w2 = set.regular.width("aa", 10.0);
        assert!(w1 > 0.0 && (w2 - 2.0 * w1).abs() < 1e-3);
        assert_eq!(set.regular.sanitize("a\u{a0}b\tc\u{7}"), "a b c?");
    }

    #[test]
    fn garbage_is_not_a_font() {
        assert!(Face::from_bytes(vec![1, 2, 3]).is_none());
    }
}

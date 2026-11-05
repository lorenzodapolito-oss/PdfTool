//! Lettura di un EPUB: metadati, capitoli in ordine di lettura e blocchi di contenuto.

use scraper::{ElementRef, Html, Node, Selector};
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Blocco di contenuto di un capitolo.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Quote(String),
    Item(String),
    Image(Vec<u8>),
    Rule,
}

pub struct Epub {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
    pub title: String,
    pub authors: Vec<String>,
    /// Percorsi (nello zip) dei documenti in ordine di lettura.
    spine: Vec<String>,
}

/// Risolve `rel` rispetto alla cartella di `base` ("a/b.xhtml" + "../c.png" → "c.png").
pub fn resolve(base: &str, rel: &str) -> String {
    let rel = rel.split(['#', '?']).next().unwrap_or("");
    let rel = percent_decode(rel);
    let mut parts: Vec<&str> = if rel.starts_with('/') {
        Vec::new()
    } else {
        let mut p: Vec<&str> = base.split('/').collect();
        p.pop();
        p
    };
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn sel(s: &str) -> Result<Selector, String> {
    Selector::parse(s).map_err(|e| format!("selettore {s}: {e:?}"))
}

fn text_of(e: ElementRef<'_>) -> String {
    e.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Epub {
    pub fn open(path: &std::path::Path) -> Result<Epub, String> {
        let data = std::fs::read(path).map_err(|e| format!("lettura: {e}"))?;
        Epub::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Epub, String> {
        let zip =
            zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("EPUB non valido: {e}"))?;
        let mut epub = Epub {
            zip,
            title: String::new(),
            authors: Vec::new(),
            spine: Vec::new(),
        };
        let container = epub.read_text("META-INF/container.xml")?;
        let doc = Html::parse_document(&container);
        let root = doc
            .select(&sel("rootfile")?)
            .find_map(|r| r.value().attr("full-path").map(str::to_string))
            .ok_or("container.xml senza rootfile")?;
        let opf = epub.read_text(&root)?;
        let doc = Html::parse_document(&opf);
        epub.title = doc
            .select(&sel("dc\\:title")?)
            .next()
            .map(text_of)
            .unwrap_or_default();
        epub.authors = doc
            .select(&sel("dc\\:creator")?)
            .map(text_of)
            .filter(|a| !a.is_empty())
            .collect();
        let mut manifest: HashMap<String, String> = HashMap::new();
        for it in doc.select(&sel("item")?) {
            if let (Some(id), Some(href)) = (it.value().attr("id"), it.value().attr("href")) {
                manifest.insert(id.to_string(), resolve(&root, href));
            }
        }
        for r in doc.select(&sel("itemref")?) {
            if let Some(p) = r.value().attr("idref").and_then(|id| manifest.get(id)) {
                if !epub.spine.contains(p) {
                    epub.spine.push(p.clone());
                }
            }
        }
        if epub.spine.is_empty() {
            return Err("EPUB senza capitoli".into());
        }
        Ok(epub)
    }

    pub fn chapter_count(&self) -> usize {
        self.spine.len()
    }

    fn read_bytes(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let idx = (0..self.zip.len()).find(|&i| {
            self.zip
                .by_index_raw(i)
                .ok()
                .and_then(|f| f.name().ok().map(|n| n.into_owned()))
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
        let idx = idx.ok_or_else(|| format!("{name} non trovato nell'EPUB"))?;
        let mut f = self.zip.by_index(idx).map_err(|e| format!("{name}: {e}"))?;
        let mut buf = Vec::new();
        f.by_ref()
            .take(256 * 1024 * 1024)
            .read_to_end(&mut buf)
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(buf)
    }

    fn read_text(&mut self, name: &str) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&self.read_bytes(name)?).into_owned())
    }

    /// Blocchi del capitolo `i` (le immagini sono già lette dall'archivio).
    pub fn chapter(&mut self, i: usize) -> Result<Vec<Block>, String> {
        let path = self.spine.get(i).cloned().ok_or("capitolo inesistente")?;
        let html = self.read_text(&path)?;
        let doc = Html::parse_document(&html);
        let mut blocks = Vec::new();
        let mut cur = String::new();
        let body = doc
            .select(&sel("body")?)
            .next()
            .unwrap_or_else(|| doc.root_element());
        let mut srcs = Vec::new();
        walk(body, &mut blocks, &mut cur, &mut srcs, 0);
        flush(&mut blocks, &mut cur);
        // Sostituisce i segnaposto delle immagini con i dati reali.
        let mut out = Vec::new();
        let mut it = srcs.into_iter();
        for b in blocks {
            match b {
                Block::Image(_) => {
                    if let Some(src) = it.next() {
                        if let Ok(bytes) = self.read_bytes(&resolve(&path, &src)) {
                            out.push(Block::Image(bytes));
                        }
                    }
                }
                other => out.push(other),
            }
        }
        Ok(out)
    }
}

fn flush(blocks: &mut Vec<Block>, cur: &mut String) {
    let t = cur.split_whitespace().collect::<Vec<_>>().join(" ");
    cur.clear();
    if !t.is_empty() {
        blocks.push(Block::Para(t));
    }
}

const MAX_DEPTH: usize = 200;

fn walk(
    el: ElementRef<'_>,
    blocks: &mut Vec<Block>,
    cur: &mut String,
    srcs: &mut Vec<String>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in el.children() {
        match child.value() {
            Node::Text(t) => {
                cur.push_str(t);
            }
            Node::Element(e) => {
                let Some(ce) = ElementRef::wrap(child) else {
                    continue;
                };
                let name = e.name();
                match name {
                    "script" | "style" | "head" | "title" => {}
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        flush(blocks, cur);
                        let t = text_of(ce);
                        if !t.is_empty() {
                            let lvl = name.as_bytes().get(1).map_or(3, |c| c - b'0');
                            blocks.push(Block::Heading(lvl, t));
                        }
                    }
                    "blockquote" => {
                        flush(blocks, cur);
                        let t = text_of(ce);
                        if !t.is_empty() {
                            blocks.push(Block::Quote(t));
                        }
                    }
                    "li" => {
                        flush(blocks, cur);
                        let t = text_of(ce);
                        if !t.is_empty() {
                            blocks.push(Block::Item(t));
                        }
                    }
                    "hr" => {
                        flush(blocks, cur);
                        blocks.push(Block::Rule);
                    }
                    "br" => flush(blocks, cur),
                    "img" | "image" => {
                        flush(blocks, cur);
                        if let Some(src) = e.attr("src").or_else(|| e.attr("href")) {
                            srcs.push(src.to_string());
                            blocks.push(Block::Image(Vec::new()));
                        }
                    }
                    "p" | "div" | "section" | "article" | "body" | "ul" | "ol" | "table" | "tr"
                    | "figure" | "figcaption" | "pre" | "svg" | "dl" | "dt" | "dd" | "header"
                    | "footer" | "nav" | "aside" | "td" | "th" => {
                        flush(blocks, cur);
                        walk(ce, blocks, cur, srcs, depth + 1);
                        flush(blocks, cur);
                    }
                    _ => walk(ce, blocks, cur, srcs, depth + 1),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;

    pub fn sample_epub() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default();
            let mut put = |n: &str, d: &[u8]| {
                z.start_file(n, o).unwrap();
                z.write_all(d).unwrap();
            };
            put("mimetype", b"application/epub+zip");
            put(
                "META-INF/container.xml",
                br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
            );
            put(
                "OEBPS/content.opf",
                br#"<?xml version="1.0"?><package><metadata><dc:title>Il libro</dc:title><dc:creator>Anna</dc:creator><dc:creator>Bruno</dc:creator></metadata>
<manifest><item id="c1" href="text/c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/c2.xhtml" media-type="application/xhtml+xml"/><item id="img" href="img/a%20b.png" media-type="image/png"/></manifest>
<spine><itemref idref="c2"/><itemref idref="c1"/></spine></package>"#,
            );
            put(
                "OEBPS/text/c1.xhtml",
                br#"<html><body><h1>Uno</h1><p>Primo <b>paragrafo</b> qui.</p><blockquote>Citazione</blockquote><ul><li>voce</li></ul><hr/><img src="../img/a%20b.png"/></body></html>"#,
            );
            put(
                "OEBPS/text/c2.xhtml",
                b"<html><body><h2>Due</h2><p>Testo</p></body></html>",
            );
            let png = {
                let mut v = Vec::new();
                image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
                    .write_to(&mut Cursor::new(&mut v), image::ImageFormat::Png)
                    .unwrap();
                v
            };
            put("OEBPS/img/a b.png", &png);
            z.finish().unwrap();
        }
        buf
    }

    #[test]
    fn resolves_paths() {
        assert_eq!(
            resolve("OEBPS/text/c1.xhtml", "../img/a%20b.png"),
            "OEBPS/img/a b.png"
        );
        assert_eq!(resolve("a.opf", "x/y.html#frag"), "x/y.html");
        assert_eq!(resolve("a/b.html", "/z.png"), "z.png");
        assert_eq!(resolve("a.html", "../../../x"), "x");
    }

    #[test]
    fn reads_metadata_and_spine_order() {
        let mut e = Epub::from_bytes(sample_epub()).unwrap();
        assert_eq!(e.title, "Il libro");
        assert_eq!(e.authors, vec!["Anna", "Bruno"]);
        assert_eq!(e.chapter_count(), 2);
        assert_eq!(e.chapter(0).unwrap()[0], Block::Heading(2, "Due".into()));
    }

    #[test]
    fn parses_blocks_and_images() {
        let mut e = Epub::from_bytes(sample_epub()).unwrap();
        let b = e.chapter(1).unwrap();
        assert_eq!(b[0], Block::Heading(1, "Uno".into()));
        assert_eq!(b[1], Block::Para("Primo paragrafo qui.".into()));
        assert_eq!(b[2], Block::Quote("Citazione".into()));
        assert_eq!(b[3], Block::Item("voce".into()));
        assert_eq!(b[4], Block::Rule);
        assert!(matches!(&b[5], Block::Image(d) if d.starts_with(&[0x89, b'P'])));
        assert!(e.chapter(9).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(Epub::from_bytes(b"niente".to_vec()).is_err());
    }
}

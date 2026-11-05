//! Utilità sui file: scansione di cartelle, ordine naturale, nomi di uscita senza collisioni.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];

pub fn ext_lower(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn has_ext(p: &Path, exts: &[&str]) -> bool {
    let e = ext_lower(p);
    exts.contains(&e.as_str())
}

/// Confronto "naturale": pagina2 < pagina10, senza distinguere maiuscole.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut ia, mut ib) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                let mut nb = String::new();
                while let Some(c) = ia.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ia.next();
                }
                while let Some(c) = ib.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    ib.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                ia.next();
                ib.next();
            }
        }
    }
}

/// Tutti i file con le estensioni date, anche nelle sottocartelle, in ordine naturale.
pub fn scan_dir(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_into(dir, exts, &mut out, 0);
    out.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    out
}

fn scan_into(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_into(&p, exts, out, depth + 1);
        } else if has_ext(&p, exts) {
            out.push(p);
        }
    }
}

/// Toglie i caratteri non ammessi nei nomi di file.
pub fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() {
        "output".into()
    } else {
        t
    }
}

/// `dir/stem.ext`, oppure `dir/stem (2).ext` se esiste già.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = dir.join(format!("{stem} ({n}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["p10.jpg", "p2.jpg", "P1.jpg", "p02a.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["P1.jpg", "p2.jpg", "p02a.jpg", "p10.jpg"]);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
    }

    #[test]
    fn safe_and_unique_names() {
        assert_eq!(safe_name("a/b:c?"), "a_b_c_");
        assert_eq!(safe_name("  .. "), "output");
        let d = std::env::temp_dir().join(format!("pdftool_t_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(unique_path(&d, "x", "pdf"), d.join("x.pdf"));
        std::fs::write(d.join("x.pdf"), b"1").unwrap();
        assert_eq!(unique_path(&d, "x", "pdf"), d.join("x (2).pdf"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn scan_is_recursive_and_filtered() {
        let d = std::env::temp_dir().join(format!("pdftool_s_{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("b.PNG"), b"1").unwrap();
        std::fs::write(d.join("sub/a.jpg"), b"1").unwrap();
        std::fs::write(d.join("note.txt"), b"1").unwrap();
        let found = scan_dir(&d, IMAGE_EXTS);
        assert_eq!(found.len(), 2);
        assert!(scan_dir(&d.join("nope"), IMAGE_EXTS).is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}

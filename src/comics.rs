//! Fumetti CBZ / CBR / CB7: estrazione delle immagini in ordine di pagina.

use crate::files::{has_ext, natural_cmp, scan_dir, IMAGE_EXTS};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pagine di un fumetto; per i formati senza accesso casuale sono estratte in una cartella temporanea.
pub struct Comic {
    kind: Kind,
}

enum Kind {
    Zip(zip::ZipArchive<std::fs::File>, Vec<usize>),
    Dir(PathBuf, Vec<PathBuf>),
}

impl Drop for Comic {
    fn drop(&mut self) {
        if let Kind::Dir(tmp, _) = &self.kind {
            let _ = std::fs::remove_dir_all(tmp);
        }
    }
}

#[derive(PartialEq, Debug)]
enum Magic {
    Zip,
    SevenZ,
    Rar,
    Unknown,
}

fn sniff(path: &Path) -> Magic {
    let mut head = [0u8; 8];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let h = &head[..n];
    if h.starts_with(b"PK") {
        Magic::Zip
    } else if h.starts_with(&[b'7', b'z', 0xBC, 0xAF]) {
        Magic::SevenZ
    } else if h.starts_with(b"Rar!") {
        Magic::Rar
    } else {
        Magic::Unknown
    }
}

fn temp_dir() -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "pdftool_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).map_err(|e| format!("cartella temporanea: {e}"))?;
    Ok(d)
}

fn safe_entry(name: &str) -> bool {
    !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains(':')
        && !name.split(['/', '\\']).any(|p| p == "..")
}

fn run_quiet(cmd: &mut Command) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Estrae un RAR con uno degli strumenti installati (7-Zip, unrar, tar di Windows 10+).
fn extract_rar(src: &Path, dest: &Path) -> Result<(), String> {
    let seven = [
        "7z",
        "7za",
        "7zz",
        "C:\\Program Files\\7-Zip\\7z.exe",
        "C:\\Program Files (x86)\\7-Zip\\7z.exe",
    ];
    for exe in seven {
        let mut out_arg = std::ffi::OsString::from("-o");
        out_arg.push(dest);
        if run_quiet(Command::new(exe).arg("x").arg("-y").arg(&out_arg).arg(src)) {
            return Ok(());
        }
    }
    let mut dest_slash = dest.as_os_str().to_owned();
    dest_slash.push(std::path::MAIN_SEPARATOR_STR);
    if run_quiet(
        Command::new("unrar")
            .arg("x")
            .arg("-y")
            .arg(src)
            .arg(&dest_slash),
    ) {
        return Ok(());
    }
    if run_quiet(Command::new("tar").arg("-xf").arg(src).arg("-C").arg(dest)) {
        return Ok(());
    }
    Err("per i file CBR serve 7-Zip (o unrar) installato: non l'ho trovato".into())
}

fn extract_7z(src: &Path, dest: &Path) -> Result<(), String> {
    sevenz_rust::decompress_file_with_extract_fn(src, dest, |entry, reader, path| {
        if !safe_entry(entry.name()) {
            return Ok(true); // voce con percorso pericoloso: ignorata
        }
        sevenz_rust::default_entry_extract_fn(entry, reader, path)
    })
    .map_err(|e| format!("7z: {e}"))
}

impl Comic {
    pub fn open(path: &Path) -> Result<Comic, String> {
        let kind = match sniff(path) {
            Magic::Zip => {
                let f = std::fs::File::open(path).map_err(|e| format!("apertura: {e}"))?;
                let mut z =
                    zip::ZipArchive::new(f).map_err(|e| format!("archivio non valido: {e}"))?;
                let mut names: Vec<(String, usize)> = Vec::new();
                for i in 0..z.len() {
                    if let Ok(e) = z.by_index_raw(i) {
                        let Ok(name) = e.name().map(|n| n.into_owned()) else {
                            continue;
                        };
                        if !e.is_dir() && has_ext(Path::new(&name), IMAGE_EXTS) {
                            names.push((name, i));
                        }
                    }
                }
                names.sort_by(|a, b| natural_cmp(&a.0, &b.0));
                Kind::Zip(z, names.into_iter().map(|(_, i)| i).collect())
            }
            m @ (Magic::SevenZ | Magic::Rar) => {
                let tmp = temp_dir()?;
                let r = if m == Magic::SevenZ {
                    extract_7z(path, &tmp)
                } else {
                    extract_rar(path, &tmp)
                };
                if let Err(e) = r {
                    let _ = std::fs::remove_dir_all(&tmp);
                    return Err(e);
                }
                let files = scan_dir(&tmp, IMAGE_EXTS);
                Kind::Dir(tmp, files)
            }
            Magic::Unknown => return Err("formato non riconosciuto (atteso CBZ, CBR o CB7)".into()),
        };
        let c = Comic { kind };
        if c.len() == 0 {
            return Err("nessuna immagine trovata nel fumetto".into());
        }
        Ok(c)
    }

    pub fn len(&self) -> usize {
        match &self.kind {
            Kind::Zip(_, v) => v.len(),
            Kind::Dir(_, v) => v.len(),
        }
    }

    /// Byte dell'immagine della pagina `i`.
    pub fn page(&mut self, i: usize) -> Result<Vec<u8>, String> {
        match &mut self.kind {
            Kind::Zip(z, idx) => {
                let &zi = idx.get(i).ok_or("pagina inesistente")?;
                let f = z
                    .by_index(zi)
                    .map_err(|e| format!("pagina {}: {e}", i + 1))?;
                let mut buf = Vec::new();
                f.take(256 * 1024 * 1024)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("pagina {}: {e}", i + 1))?;
                Ok(buf)
            }
            Kind::Dir(_, files) => {
                let p = files.get(i).ok_or("pagina inesistente")?;
                std::fs::read(p).map_err(|e| format!("pagina {}: {e}", i + 1))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn png(c: u8) -> Vec<u8> {
        let mut v = Vec::new();
        image::RgbImage::from_pixel(3, 2, image::Rgb([c, c, c]))
            .write_to(&mut Cursor::new(&mut v), image::ImageFormat::Png)
            .unwrap();
        v
    }

    fn make_cbz(path: &Path) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        for (n, c) in [("p10.png", 10u8), ("p2.png", 2), ("p1.PNG", 1)] {
            z.start_file(n, o).unwrap();
            z.write_all(&png(c)).unwrap();
        }
        z.start_file("note.txt", o).unwrap();
        z.write_all(b"x").unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn cbz_pages_are_naturally_sorted() {
        let p = std::env::temp_dir().join(format!("pdftool_c_{}.cbz", std::process::id()));
        make_cbz(&p);
        let mut c = Comic::open(&p).unwrap();
        assert_eq!(c.len(), 3);
        for (i, want) in [1u8, 2, 10].iter().enumerate() {
            let img = image::load_from_memory(&c.page(i).unwrap())
                .unwrap()
                .to_rgb8();
            assert_eq!(img.get_pixel(0, 0).0[0], *want);
        }
        assert!(c.page(3).is_err());
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn rejects_unknown_and_empty() {
        let d = std::env::temp_dir();
        let p = d.join(format!("pdftool_u_{}.cbz", std::process::id()));
        std::fs::write(&p, b"non un archivio").unwrap();
        assert!(Comic::open(&p).is_err());
        std::fs::remove_file(&p).unwrap();
        assert!(Comic::open(&d.join("non_esiste_pdftool.cbz")).is_err());
    }

    #[test]
    fn entry_safety() {
        assert!(safe_entry("a/b.png"));
        assert!(!safe_entry("../x.png"));
        assert!(!safe_entry("a/../../x"));
        assert!(!safe_entry("/etc/x"));
        assert!(!safe_entry("C:\\x"));
    }
}

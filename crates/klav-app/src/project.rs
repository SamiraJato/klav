//! Descoberta de arquivos do projeto: varre uma pasta por `.klav`.

use std::path::{Path, PathBuf};

/// Profundidade máxima da varredura de subpastas.
const MAX_DEPTH: usize = 4;

/// Varre uma pasta (até [`MAX_DEPTH`] níveis) por arquivos `.klav`, ordenados.
pub fn scan_klav(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_into(dir, &mut out, 0);
    out.sort();
    out
}

fn scan_into(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            scan_into(&p, out, depth + 1);
        } else if p.extension().map(|e| e.eq_ignore_ascii_case("klav")).unwrap_or(false) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_klav_e_ignora_o_resto() {
        let base = std::env::temp_dir().join(format!("klav_scan_{}", std::process::id()));
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(base.join("a.klav"), b"{}").unwrap();
        std::fs::write(base.join("nota.txt"), b"x").unwrap();
        std::fs::write(sub.join("b.KLAV"), b"{}").unwrap(); // extensão case-insensitive

        let found = scan_klav(&base);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(found.len(), 2, "achou {names:?}");
        assert!(names.contains(&"a.klav".to_string()));
        assert!(names.contains(&"b.KLAV".to_string()));
        assert!(!names.iter().any(|n| n.ends_with(".txt")));

        let _ = std::fs::remove_dir_all(&base);
    }
}

use std::path::{Path, PathBuf};
use super::metadata;

// Una pista encontrada en disco. Sin tipos de UI: la conversión a la fila
// de Slint vive en el puente de reproducción.
pub struct LibraryTrack {
    pub title: String,
    pub artist: String,
    pub file_name: String,
    pub path: PathBuf,
    pub cover_bytes: Option<Vec<u8>>,
}

const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "flac", "ogg", "oga", "opus", "wav", "aac", "wma", "aiff", "aif"];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

// Escanea una carpeta (no recursivo) y entrega cada pista ya ordenada por
// nombre de archivo. Callback en vez de Vec: retener las carátulas de todas
// las pistas a la vez eran cientos de MB de pico de RAM.
pub fn scan_folder(dir: &Path, mut on_track: impl FnMut(LibraryTrack)) {
    let Ok(read_dir) = std::fs::read_dir(dir) else { return; };
    let mut files: Vec<PathBuf> = read_dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_audio_file(p))
        .collect();
    files.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());
    for path in files {
        let info = metadata::read_metadata(&path);
        on_track(LibraryTrack {
            title: info.title,
            artist: info.artist,
            file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            path,
            cover_bytes: info.cover_bytes,
        });
    }
}

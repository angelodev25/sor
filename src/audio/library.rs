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

// Escanea una carpeta (no recursivo), un archivo de audio por entrada.
pub fn scan_folder(dir: &Path) -> Vec<LibraryTrack> {
    let mut tracks = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else { return tracks; };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_audio_file(&path) {
            continue;
        }
        let info = metadata::read_metadata(&path);
        tracks.push(LibraryTrack {
            title: info.title,
            artist: info.artist,
            file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            path,
            cover_bytes: info.cover_bytes,
        });
    }
    tracks.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    tracks
}

use std::path::{Path, PathBuf};
use super::metadata;
use dirs;

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

// Devuelve la carpeta de música del sistema (~/Music, ~/Música, etc.)
// Fallback: $HOME o directorio actual.
pub fn get_music_dir() -> PathBuf {
    // Primero intenta audio_dir (estándar XDG)
    if let Some(dir) = dirs::audio_dir() {
        return dir;
    }
    // En Linux, audio_dir a menudo no está configurado; intenta home_dir + Music/Música
    if let Some(home) = dirs::home_dir() {
        let music = home.join("Music");
        let musica = home.join("Música");
        if music.exists() {
            return music;
        }
        if musica.exists() {
            return musica;
        }
        // Si no existe ninguna, devuelve ~/Music para que la cree el usuario
        return music;
    }
    // Último fallback: directorio actual
    PathBuf::from(".")
}

// Lista subdirectorios inmediatos de una carpeta, ordenados por nombre.
pub fn list_subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            let mut dirs: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());
            dirs
        })
        .unwrap_or_default()
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

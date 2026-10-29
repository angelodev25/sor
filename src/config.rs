// Configuración persistente de la biblioteca (última carpeta visitada).
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use dirs;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct LibraryConfig {
    pub last_folder: String,
    pub version: u32,
}

impl LibraryConfig {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|mut p| {
            p.push("sor");
            std::fs::create_dir_all(&p).ok();
            p.push("library.json");
            p
        })
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            let _ = std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default());
        }
    }

    pub fn get_last_folder(&self) -> Option<PathBuf> {
        if self.last_folder.is_empty() { return None; }
        let path = PathBuf::from(&self.last_folder);
        if path.exists() && path.is_dir() { Some(path) } else { None }
    }

    pub fn set_last_folder(&mut self, path: &Path) {
        self.last_folder = path.to_string_lossy().into_owned();
        self.version = 1;
        self.save();
    }
}
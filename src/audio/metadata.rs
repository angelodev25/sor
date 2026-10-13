use std::path::Path;
use lofty::prelude::*;
use lofty::probe::Probe;

pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub duration_secs: f32,
    pub cover_bytes: Option<Vec<u8>>,
}

fn fallback_title(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Pista desconocida".into())
}

pub fn read_metadata(path: &Path) -> TrackInfo {
    let fallback = fallback_title(path);
    let tagged = Probe::open(path).and_then(|p| p.read());
    let Ok(tagged) = tagged else {
        return TrackInfo {
            title: fallback,
            artist: "Artista desconocido".into(),
            duration_secs: 0.0,
            cover_bytes: None,
        };
    };
    let tag = tagged.primary_tag();
    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback.clone());
    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Artista desconocido".into());
    TrackInfo {
        title,
        artist,
        duration_secs: tagged.properties().duration().as_secs_f32(),
        cover_bytes: tag.and_then(|t| t.pictures().first().map(|p| p.data().to_vec())),
    }
}

pub fn fmt_mmss(secs: f32) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "00:00".into();
    }
    let total = secs as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

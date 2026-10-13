// Puente entre la UI y el motor: convierte pistas a filas de Slint,
// sincroniza metadatos con el Player y gestiona el hilo de reproducción.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;

use crate::audio::{
    engine::AudioEngine,
    eq::ThreeBandEQ,
    library,
    metadata,
    Controls, EqParams, LimiterParams, OutputParams,
};
use crate::ui::{AppWindow, TrackEntry};

pub fn cover_to_image(bytes: &[u8]) -> Option<slint::Image> {
    let img = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (w, h) = (img.width(), img.height());
    // shortcut: carátulas gigantes se reescalan a 512px, ampliar a caché en disco si hace falta.
    let img = if w.max(h) > 512 {
        image::imageops::thumbnail(&img, 512, 512)
    } else {
        img
    };
    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(img.width(), img.height());
    buf.make_mut_bytes().copy_from_slice(&img);
    Some(slint::Image::from_rgb8(buf))
}

// Escanea la carpeta y la deja lista para `ui.set_tracks`.
pub fn track_entries(dir: &Path) -> Vec<TrackEntry> {
    library::scan_folder(dir)
        .iter()
        .map(|t| {
            let (cover, has_cover) = t.cover_bytes.as_deref().and_then(cover_to_image)
                .map(|img| (img, true))
                .unwrap_or_default();
            TrackEntry {
                title: t.title.clone().into(),
                artist: t.artist.clone().into(),
                file_name: t.file_name.clone().into(),
                path: t.path.to_string_lossy().into_owned().into(),
                cover,
                has_cover,
            }
        })
        .collect()
}

// Muestra los metadatos de una pista en el Player. Devuelve la duración.
fn show_track_info(ui: &AppWindow, path: &Path) -> f32 {
    let info = metadata::read_metadata(path);
    ui.set_track_title(info.title.into());
    ui.set_track_artist(info.artist.into());
    ui.set_duration_label(metadata::fmt_mmss(info.duration_secs).into());
    ui.set_elapsed_label("00:00".into());
    ui.set_progress(0.0);
    match info.cover_bytes.as_deref().and_then(cover_to_image) {
        Some(img) => { ui.set_cover(img); ui.set_has_cover(true); }
        None => ui.set_has_cover(false),
    }
    info.duration_secs
}

#[derive(Clone)]
pub struct PlaybackCtx {
    pub controls: Arc<Controls>,
    pub eq_params: Arc<EqParams>,
    pub limiter_params: Arc<LimiterParams>,
    pub output_params: Arc<OutputParams>,
    pub generation: Arc<AtomicU64>,
    // Serializa los motores: el hilo nuevo no abre su stream hasta que el
    // anterior soltó el dispositivo. Sin esto, dos streams concurrentes en
    // el cambio de pista hacen fallar el arranque y hay que pulsar dos veces.
    pub engine_lock: Arc<Mutex<()>>,
}

// Arranca la reproducción en un hilo. `play_id` invalida hilos anteriores:
// solo el último toca la UI al terminar, así cambiar de pista no deja
// estados cruzados.
fn spawn_playback(path: PathBuf, ui_weak: slint::Weak<AppWindow>, ctx: PlaybackCtx, play_id: u64) {
    thread::spawn(move || {
        let current = || ctx.generation.load(Ordering::Relaxed) == play_id;
        if !current() {
            return;
        }
        // Espera (sin bloquear la UI) a que el motor anterior muera del todo.
        let _guard = ctx.engine_lock.lock().unwrap_or_else(|e| e.into_inner());
        if !current() {
            return;
        }
        let mut engine = match AudioEngine::new() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error al crear el motor: {e}");
                return;
            }
        };
        println!("Dispositivo reconocido. Sample rate: {} Hz, {} canales", engine.sample_rate(), engine.channels());

        let eq = ThreeBandEQ::new(engine.sample_rate() as f32, 180.0, 3500.0);
        let sample_rate = engine.sample_rate();

        if let Err(e) = engine.play(
            &path,
            eq,
            ctx.controls.clone(),
            ctx.eq_params,
            ctx.limiter_params,
            ctx.output_params
        ) {
            eprintln!("Error al arrancar stream: {e}");
            if current() {
                let _ = ui_weak.upgrade_in_event_loop(|ui| {
                    ui.set_is_loading(false);
                    ui.set_track_title("Error al cargar".into());
                });
            }
            return;
        }

        if current() {
            let _ = ui_weak.upgrade_in_event_loop(|ui| {
                ui.set_is_loading(false);
                ui.set_is_playing(true);
            });
        }

        // Se bloquea el hilo hasta que la cancion termine, lo sustituya otra
        // pista o el usuario pida salir, actualizando el progreso por el camino.
        while current()
            && !ctx.controls.song_ended.load(Ordering::Relaxed)
            && !ctx.controls.quit.load(Ordering::Relaxed)
        {
            let (ratio, elapsed) = ctx.controls.progress(sample_rate);
            let label = metadata::fmt_mmss(elapsed);
            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                ui.set_progress(ratio);
                ui.set_elapsed_label(label.into());
            });
            thread::sleep(std::time::Duration::from_millis(250));
        }

        if current() {
            let _ = ui_weak.upgrade_in_event_loop(|ui| {
                ui.set_is_playing(false);
            });
        }

        engine.stop();
    });
}

// Prepara la UI y lanza la reproducción de una pista (hilo de UI).
pub fn start_track(path: PathBuf, ui: &AppWindow, ui_weak: slint::Weak<AppWindow>, ctx: &PlaybackCtx) {
    let duration = show_track_info(ui, &path);
    ctx.controls.reset_progress(duration);
    ctx.controls.paused.store(false, Ordering::Relaxed);
    // song_ended en false para que la UI no crea que terminó antes de tiempo.
    ctx.controls.song_ended.store(false, Ordering::Relaxed);
    ui.set_is_loading(true);
    spawn_playback(path, ui_weak, ctx.clone(), ctx.generation.fetch_add(1, Ordering::Relaxed) + 1);
}

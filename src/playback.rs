// Puente entre la UI y el motor: convierte pistas a filas de Slint,
// sincroniza metadatos con el Player y sigue el progreso por pista.
//
// El AudioEngine vive en un hilo gestor propio (cpal::Stream no es Send)
// y recibe órdenes por canal; aquí solo viajan tipos Send.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::audio::{
    engine::AudioEngine,
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

enum EngineCmd {
    PlayTrack { path: PathBuf, track_id: u64 },
}

#[derive(Clone)]
pub struct PlaybackCtx {
    engine_cmd: Sender<EngineCmd>,
    sample_rate: u32,
    pub controls: Arc<Controls>,
    pub generation: Arc<AtomicU64>,
}

// Crea el motor en su hilo gestor y abre el stream una vez para toda la
// sesión. Devuelve el contexto para pedir pistas desde la UI.
pub fn spawn_engine(
    controls: Arc<Controls>,
    eq_params: Arc<EqParams>,
    limiter_params: Arc<LimiterParams>,
    output_params: Arc<OutputParams>,
) -> Result<PlaybackCtx, Box<dyn std::error::Error>> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCmd>();
    let (ack_tx, ack_rx) = mpsc::channel::<Result<u32, String>>();

    let thread_controls = controls.clone();
    let thread_eq = eq_params.clone();
    let thread_lim = limiter_params.clone();
    let thread_out = output_params.clone();
    thread::spawn(move || {
        let mut engine = match AudioEngine::new() {
            Ok(e) => e,
            Err(e) => {
                let _ = ack_tx.send(Err(e.to_string()));
                return;
            }
        };
        println!("Dispositivo reconocido. Sample rate: {} Hz, {} canales", engine.sample_rate(), engine.channels());
        if let Err(e) = engine.start(thread_controls.clone(), thread_eq, thread_lim, thread_out) {
            let _ = ack_tx.send(Err(e.to_string()));
            return;
        }
        let _ = ack_tx.send(Ok(engine.sample_rate()));
        // Dueño único del motor: atiende pistas de una en una, en orden.
        // Los cambios rápidos se serializan aquí solos, sin mutex.
        for cmd in cmd_rx {
            match cmd {
                EngineCmd::PlayTrack { path, track_id } => {
                    if let Err(e) = engine.play_track(&path, track_id, &thread_controls) {
                        eprintln!("Error al arrancar pista: {e}");
                    }
                }
            }
        }
    });

    let sample_rate = ack_rx.recv().map_err(|_| "el hilo del motor murió al arrancar")?
        .map_err(|e| format!("Sin dispositivo de audio: {e}"))?;
    Ok(PlaybackCtx {
        engine_cmd: cmd_tx,
        sample_rate,
        controls,
        generation: Arc::new(AtomicU64::new(0)),
    })
}

// Sigue el progreso de una pista en un hilo. `play_id` invalida hilos
// anteriores: solo el último toca la UI, así cambiar de pista no deja
// estados cruzados.
fn spawn_progress(ui_weak: slint::Weak<AppWindow>, ctx: PlaybackCtx, play_id: u64) {
    thread::spawn(move || {
        let current = || ctx.generation.load(Ordering::Relaxed) == play_id;

        // Actualiza la barra hasta que la cancion termine, la sustituya otra
        // pista o el usuario pida salir.
        while current()
            && !ctx.controls.song_ended.load(Ordering::Relaxed)
            && !ctx.controls.quit.load(Ordering::Relaxed)
        {
            let (ratio, elapsed) = ctx.controls.progress(ctx.sample_rate);
            let label = metadata::fmt_mmss(elapsed);
            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                ui.set_progress(ratio);
                ui.set_elapsed_label(label.into());
            });
            thread::sleep(std::time::Duration::from_millis(250));
        }

        if !current() {
            return;
        }
        if ctx.controls.song_ended.load(Ordering::Relaxed) {
            // Fin natural: rebobina al principio y queda en pausa. Se marca
            // como no cargada porque la fuente está agotada: el próximo play
            // la relanza desde cero.
            ctx.controls.rewind();
            ctx.controls.paused.store(true, Ordering::Relaxed);
            ctx.controls.song_loaded.store(false, Ordering::Relaxed);
            let _ = ui_weak.upgrade_in_event_loop(|ui| {
                ui.set_is_playing(false);
                ui.set_progress(0.0);
                ui.set_elapsed_label("00:00".into());
            });
        } else {
            let _ = ui_weak.upgrade_in_event_loop(|ui| {
                ui.set_is_playing(false);
            });
        }
    });
}

// Prepara la UI y pide la pista al hilo gestor (hilo de UI).
pub fn start_track(path: PathBuf, ui: &AppWindow, ui_weak: slint::Weak<AppWindow>, ctx: &PlaybackCtx) {
    let play_id = ctx.generation.fetch_add(1, Ordering::Relaxed) + 1;
    let duration = show_track_info(ui, &path);
    ctx.controls.reset_progress(duration);
    ctx.controls.paused.store(false, Ordering::Relaxed);
    // song_ended en false para que la UI no crea que terminó antes de tiempo.
    ctx.controls.song_ended.store(false, Ordering::Relaxed);
    ui.set_is_loading(true);

    let ctx_thread = ctx.clone();
    thread::spawn(move || {
        if ctx_thread.generation.load(Ordering::Relaxed) != play_id {
            return;
        }
        // La petición viaja al gestor; si la cola está rota, informa en la UI.
        if ctx_thread.engine_cmd.send(EngineCmd::PlayTrack { path, track_id: play_id }).is_err() {
            if ctx_thread.generation.load(Ordering::Relaxed) == play_id {
                let _ = ui_weak.upgrade_in_event_loop(|ui| {
                    ui.set_is_loading(false);
                    ui.set_track_title("Error al cargar".into());
                });
            }
            return;
        }
        if ctx_thread.generation.load(Ordering::Relaxed) == play_id {
            let _ = ui_weak.upgrade_in_event_loop(|ui| {
                ui.set_is_loading(false);
                ui.set_is_playing(true);
            });
        }
        spawn_progress(ui_weak, ctx_thread, play_id);
    });
}

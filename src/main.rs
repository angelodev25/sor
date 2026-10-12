use std::path::Path;
use std::error::Error as StdError;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

mod audio;
mod ui;

use slint::ComponentHandle;
use ui::AppWindow;

use crate::audio::{
    engine::AudioEngine,
    eq::ThreeBandEQ,
    Controls,
    OutputParams,
    EqParams,
    LimiterParams
};

fn main() -> Result<(), Box<dyn StdError>> {
    let ui = AppWindow::new()?;
    ui.set_track_title("Titulo".into());

    // Estado compartido
    let controls = Arc::new(Controls::new());
    let eq_params = Arc::new(EqParams::new());
    let limiter_params = Arc::new(LimiterParams::new());
    let output_params = Arc::new(OutputParams::new());
    output_params.set_volume(0.75);          // volumen de escucha inicial
    output_params.soft_clip_enabled.store(true, Ordering::Relaxed);
    output_params.soft_clip_coeff.store(f32::to_bits(-1.2), Ordering::Relaxed);

    // Establecer valores inciales del EQ
    //
    // Esta es la configuracion que me dio el mejor sonido en mi equipo hasta ahora.
    // Nota, mi equipo de sonido es bastante modesto, para ti puede ser una configuracion muy diferente.
    eq_params.set(&eq_params.low_gain, 1.9);
    eq_params.set(&eq_params.low_resonance, 6.0);
    eq_params.set(&eq_params.low_shelf, 4.0);
    eq_params.set(&eq_params.mid_gain, 1.0);
    eq_params.set(&eq_params.mid_presence, 2.0);
    eq_params.set(&eq_params.mid_body, 1.0);
    eq_params.set(&eq_params.high_gain, 2.8);
    eq_params.set(&eq_params.high_shelf, 4.0);
    eq_params.set(&eq_params.high_air, 6.0);
    eq_params.set(&eq_params.master_gain, 0.72);

    // Sincronizar valores iniciales a la UI.
    ui.set_eq_low_gain(1.9);
    ui.set_eq_low_shelf(4.0);
    ui.set_eq_low_resonance(6.0);
    ui.set_eq_mid_gain(1.0);
    ui.set_eq_mid_body(1.0);
    ui.set_eq_mid_presence(2.0);
    ui.set_eq_high_gain(2.8);
    ui.set_eq_high_shelf(4.0);
    ui.set_eq_high_air(6.0);
    ui.set_eq_master_gain(0.72);
    ui.set_soft_clip_enabled(true);
    ui.set_soft_clipping(-1.2);
    ui.set_eq_limiter_enabled(true);
    ui.set_eq_limiter_ceiling(1.5);
    ui.set_eq_volume(0.75);

    
    {
        let ui_handle_eq = ui.as_weak();
        let eq_params_eq = eq_params.clone();
        ui.on_eq_changed(move || {
            if let Some(ui) = ui_handle_eq.upgrade() {
                eq_params_eq.set(&eq_params_eq.low_gain, ui.get_eq_low_gain());
                eq_params_eq.set(&eq_params_eq.low_shelf, ui.get_eq_low_shelf());
                eq_params_eq.set(&eq_params_eq.low_resonance, ui.get_eq_low_resonance());
                eq_params_eq.set(&eq_params_eq.mid_gain, ui.get_eq_mid_gain());
                eq_params_eq.set(&eq_params_eq.mid_body, ui.get_eq_mid_body());
                eq_params_eq.set(&eq_params_eq.mid_presence, ui.get_eq_mid_presence());
                eq_params_eq.set(&eq_params_eq.high_gain, ui.get_eq_high_gain());
                eq_params_eq.set(&eq_params_eq.high_shelf, ui.get_eq_high_shelf());
                eq_params_eq.set(&eq_params_eq.high_air, ui.get_eq_high_air());
                eq_params_eq.set(&eq_params_eq.master_gain, ui.get_eq_master_gain());
            }
        });
    }

    {
        let ui_handle_lim = ui.as_weak();
        let output_lim = output_params.clone();
        let lim_params_clone = limiter_params.clone();
        ui.on_limiter_changed(move || {
            if let Some(ui) = ui_handle_lim.upgrade() {
                output_lim.limiter_enabled.store(ui.get_eq_limiter_enabled(), Ordering::Relaxed);
                lim_params_clone.ceiling.store(f32::to_bits(ui.get_eq_limiter_ceiling()), Ordering::Relaxed);
            }
        });
    }

    {
        let ui_handle_vol = ui.as_weak();
        let out_params = output_params.clone();
        ui.on_output_changed(move || {
            if let Some(ui) = ui_handle_vol.upgrade() {
                out_params.set_volume(ui.get_eq_volume());
                out_params.soft_clip_coeff
                    .store(f32::to_bits(ui.get_soft_clipping()), Ordering::Relaxed);
                out_params.soft_clip_enabled
                    .store(ui.get_soft_clip_enabled(), Ordering::Relaxed);
            }
        });
    }

    let ui_handle = ui.as_weak();
    let controls_ui = controls.clone();
    let eq_params_ui = eq_params.clone();
    let limiter_params_ui = limiter_params.clone();
    let output_params_ui = output_params.clone();

    // Callback del boton play/pause de la ui
    ui.on_play_pause(move || {
        let ui = ui_handle.unwrap();

        if ui.get_is_loading() {
            return;
        }

        if !controls_ui.song_loaded.load(Ordering::Relaxed) {
            ui.set_is_loading(true);
            // Establecer en false el flag de song_ended antes de iniciar la reproduccion para que la ui no piense que
            // la cancion ha terminado antes de tiempo y ademas permitir que la ui pueda reiniciar la cancion si el usuario desea.
            controls_ui.song_ended.store(false, Ordering::Relaxed);

            let ui_weak = ui_handle.clone();
            let controls_thread = controls_ui.clone();
            let eq_params_thread = eq_params_ui.clone();
            let limiter_params_thread = limiter_params_ui.clone();
            let output_params_thread = output_params_ui.clone();
            let path = Path::new("BAD OMENS  Like A Villain (Official Music Video).m4a");

            thread::spawn(move || {
                let mut engine = match AudioEngine::new() {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Error al crear el motor: {e}");
                        return;
                    }
                };
                println!("Dispositivo reconocido. Sample rate: {} Hz, {} canales", engine.sample_rate(), engine.channels());

                let eq = ThreeBandEQ::new(engine.sample_rate() as f32, 180.0, 3500.0);

                if let Err(e) = engine.play(
                    path, 
                    eq, 
                    controls_thread.clone(), 
                    eq_params_thread, 
                    limiter_params_thread,
                    output_params_thread
                ) {
                    eprintln!("Error al arrancar stream: {e}");
                    let _ = ui_weak.upgrade_in_event_loop(|ui| {
                        ui.set_is_loading(false);
                        ui.set_track_title("Error al cargar".into());
                    });
                    return;
                }

                let _ = ui_weak.upgrade_in_event_loop(|ui| {
                    ui.set_is_loading(false);
                    ui.set_is_playing(true);
                });

                // Se bloquea el hilo hasta que la cancion termine o el usuario pida salir
                while !controls_thread.song_ended.load(Ordering::Relaxed) && !controls_thread.quit.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(100));
                }

                let _ = ui_weak.upgrade_in_event_loop(|ui| {
                    ui.set_is_playing(false);
                });
                
                engine.stop();
            });
        } else {
            // Pausa / reanudar.
            let resume = !ui.get_is_playing();
            controls_ui.paused.store(!resume, Ordering::Relaxed);
            ui.set_is_playing(resume);
        }
    });
    
    println!("Ventana lista, ejecutando UI...");
    ui.run()?;
    Ok(())
}
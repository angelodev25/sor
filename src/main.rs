use std::error::Error as StdError;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod audio;
mod config;
mod playback;
mod ui;

use slint::{ComponentHandle, Model, ModelRc, VecModel};
use ui::AppWindow;

use crate::audio::{Controls, EqParams, LimiterParams, OutputParams};
use crate::config::LibraryConfig;
use crate::playback::{placeholder_cover, spawn_engine, start_track, folder_entries, FolderView};
use crate::audio::library::get_music_dir;

fn main() -> Result<(), Box<dyn StdError>> {
    // Backend de Slint: por defecto el renderer software (winit-software).
    // En una GPU vieja (i915/G33) el FemtoVG con OpenGL acaba ejecutando los
    // shaders GLSL en CPU y hunde el rendimiento: idle 18% -> 4%, RAM 20% -> 1%.
    // Se puede sobrescribir con SLINT_BACKEND=... en el entorno.
    if std::env::var_os("SLINT_BACKEND").is_none() {
        slint::BackendSelector::new()
            .backend_name("winit-software".into())
            .select()?;
    }

    let ui = AppWindow::new()?;
    ui.set_track_title("Titulo".into());
    // Hueco inicial del reproductor, redondeado (el gradiente de Slint no lo está).
    ui.set_cover(placeholder_cover(260, 20));

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

    // Motor en su hilo gestor + stream persistente. Falla rápido si no hay audio.
    let ctx = spawn_engine(controls, eq_params, limiter_params, output_params)?;

    // --- Inicialización de la Biblioteca con persistencia ---
    let mut lib_config = LibraryConfig::load();
    let start_dir = lib_config.get_last_folder().unwrap_or_else(get_music_dir);
    let initial_view = folder_entries(&start_dir);

    // Poblar UI inicial
    ui.set_folder_path(initial_view.current_path.to_string_lossy().into_owned().into());
    ui.set_folder_name(initial_view.current_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
        .into());
    ui.set_tracks(ModelRc::new(VecModel::from(initial_view.tracks)));
    ui.set_selected_track(-1);
    
    // Subdirectorios como SubdirEntry para la UI
    let subdirs_entries: Vec<crate::ui::SubdirEntry> = initial_view.subdirs
        .iter()
        .map(|p| crate::ui::SubdirEntry {
            name: p.file_name().unwrap_or_default().to_string_lossy().into_owned().into(),
            path: p.to_string_lossy().into_owned().into(),
        })
        .collect();
    ui.set_subdirs(ModelRc::new(VecModel::from(subdirs_entries)));
    ui.set_active_tab_index(0);
    ui.set_show_back_arrow(initial_view.parent_path.is_some());

    // Guardar carpeta inicial si no había config previa
    if lib_config.last_folder.is_empty() {
        lib_config.set_last_folder(&start_dir);
    }

    // Helper para actualizar la UI desde un FolderView
    let update_ui_from_view = |ui: &AppWindow, view: &FolderView, config: &mut LibraryConfig| {
        ui.set_folder_path(view.current_path.to_string_lossy().into_owned().into());
        ui.set_folder_name(view.current_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
            .into());
        ui.set_tracks(ModelRc::new(VecModel::from(view.tracks.clone())));
        ui.set_selected_track(-1);
        
        let subdirs_entries: Vec<crate::ui::SubdirEntry> = view.subdirs
            .iter()
            .map(|p| crate::ui::SubdirEntry {
                name: p.file_name().unwrap_or_default().to_string_lossy().into_owned().into(),
                path: p.to_string_lossy().into_owned().into(),
            })
            .collect();
        ui.set_subdirs(ModelRc::new(VecModel::from(subdirs_entries)));
        ui.set_active_tab_index(0);
        ui.set_show_back_arrow(view.parent_path.is_some());
        
        config.set_last_folder(&view.current_path);
    };

    // Callback: navegar a una ruta (desde tab click, back, etc.)
    {
        let ui_handle_nav = ui.as_weak();
        let mut config_nav = lib_config.clone();
        ui.on_navigate(move |path_str| {
            let Some(ui) = ui_handle_nav.upgrade() else { return };
            let path = PathBuf::from(path_str.as_str());
            let view = folder_entries(&path);
            update_ui_from_view(&ui, &view, &mut config_nav);
        });
    }

    // Callback: volver a carpeta padre
    {
        let ui_handle_back = ui.as_weak();
        let mut config_back = lib_config.clone();
        ui.on_go_back(move || {
            let Some(ui) = ui_handle_back.upgrade() else { return };
            let current = PathBuf::from(ui.get_folder_path().as_str());
            if let Some(parent) = current.parent() {
                let view = folder_entries(parent);
                update_ui_from_view(&ui, &view, &mut config_back);
            }
        });
    }

    // Callback: cambiar pestaña (0 = carpeta actual, 1+ = subdirs)
    {
        let ui_handle_tab = ui.as_weak();
        let mut config_tab = lib_config.clone();
        ui.on_tab_changed(move |tab_idx| {
            let Some(ui) = ui_handle_tab.upgrade() else { return };
            let current = PathBuf::from(ui.get_folder_path().as_str());
            let subdirs_model = ui.get_subdirs();
            
            let target_path = if tab_idx == 0 {
                // Carpeta actual
                current.clone()
            } else if let Some(subdir) = subdirs_model.row_data((tab_idx - 1) as usize) {
                PathBuf::from(subdir.path.as_str())
            } else {
                return;
            };
            
            let view = folder_entries(&target_path);
            update_ui_from_view(&ui, &view, &mut config_tab);
        });
    }

    // Reescanea la ruta escrita en la barra de la Biblioteca (legacy open-folder)
    {
        let ui_handle_scan = ui.as_weak();
        let mut config_scan = lib_config.clone();
        ui.on_open_folder(move || {
            let Some(ui) = ui_handle_scan.upgrade() else { return };
            let dir = PathBuf::from(ui.get_folder_path().as_str());
            let view = folder_entries(&dir);
            update_ui_from_view(&ui, &view, &mut config_scan);
        });
    }

    // Click en una fila: reproduce esa pista.
    {
        let ui_handle_track = ui.as_weak();
        let ctx_track = ctx.clone();
        ui.on_track_clicked(move |idx| {
            let Some(ui) = ui_handle_track.upgrade() else { return };
            if ui.get_is_loading() {
                return;
            }
            let Some(track) = ui.get_tracks().row_data(idx as usize) else { return };
            ui.set_selected_track(idx);
            start_track(PathBuf::from(track.path.as_str()), &ui, ui_handle_track.clone(), &ctx_track);
        });
    }

    // Callback del boton play/pause de la ui
    let ctx_play = ctx.clone();
    ui.on_play_pause(move || {
        let ui = ui_handle.unwrap();

        if ui.get_is_loading() {
            return;
        }

        if !ctx_play.controls.song_loaded.load(Ordering::Relaxed) {
            // Primera reproducción: seleccionada o primera de la lista.
            let tracks = ui.get_tracks();
            let idx = ui.get_selected_track();
            let row = (idx >= 0)
                .then(|| tracks.row_data(idx as usize))
                .flatten()
                .or_else(|| if tracks.row_count() > 0 { tracks.row_data(0) } else { None });
            let Some(track) = row else {
                ui.set_track_title("No hay audio en la carpeta".into());
                return;
            };
            if idx < 0 {
                ui.set_selected_track(0);
            }
            start_track(PathBuf::from(track.path.as_str()), &ui, ui_handle.clone(), &ctx_play);
        } else {
            // Pausa / reanudar.
            let resume = !ui.get_is_playing();
            ctx_play.controls.paused.store(!resume, Ordering::Relaxed);
            ui.set_is_playing(resume);
        }
    });
    
    println!("Ventana lista, ejecutando UI...");
    ui.run()?;
    Ok(())
}
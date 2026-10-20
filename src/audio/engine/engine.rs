use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::audio::eq::{ThreeBandEQ, SoftClipper};
use crate::audio::engine::Limiter;
use crate::audio::loader::{spawn_streamer, Streamer};
use crate::audio::controls::{Controls, EqParams, LimiterParams, OutputParams};
use crate::audio::source::AudioSource;


// Vive en un único hilo gestor (cpal::Stream no es Send): el resto del
// programa le manda órdenes por canal. El callback cpal lee del AudioSource,
// compartido con ese hilo mediante el Mutex interno.
pub struct AudioEngine {
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    streamer: Option<Streamer>,
    source: Arc<AudioSource>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, Box<dyn StdError>> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No se encontro dispositivo de audio")?;
        
        let default_cfg = device.default_output_config()?;

        // Validamos que el formato de sample sea f32, si no reportamos error
        if default_cfg.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "El dispositivo usa {:?}, se requiere f32",
                default_cfg.sample_format()
            ).into());
        }
        
        // Tamaño del buffer de cpal en funcion del sample rate, redondeando a potencia de dos para mejor compatibilidad con
        // backends de audio
        // En 44100 dara 1024 o 2048 frames (de 10-46ms).
        let target_ms = 25.0;
        let sample_rate = default_cfg.sample_rate().0;
        let frames = ((sample_rate as f32) * target_ms / 1000.0) as u32;
        let buffer_frames = frames.next_power_of_two().max(512);
        println!("Buffer frames: {} Frames requeridos: {}", buffer_frames, frames);
        let stream_config = cpal::StreamConfig {
            channels: default_cfg.channels(),
            sample_rate: default_cfg.sample_rate(),
            buffer_size: cpal::BufferSize::Fixed(buffer_frames),
        };

        Ok(Self {
            device,
            stream_config,
            stream: None,
            streamer: None,
            source: Arc::new(AudioSource::new()),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.stream_config.sample_rate.0
    }

    pub fn channels(&self) -> u16 {
        self.stream_config.channels
    }

    // Abre el stream una sola vez para toda la sesión. El callback lee del
    // AudioSource, que empieza vacío (silencio) hasta el primer play_track.
    pub fn start(
        &mut self,
        controls: Arc<Controls>,
        eq_params: Arc<EqParams>,
        limiter_params: Arc<LimiterParams>,
        output_params: Arc<OutputParams>,
    ) -> Result<(), Box<dyn StdError>> {
        let fs = self.sample_rate();
        let channels = self.stream_config.channels as usize;

        // por defecto exigimos solo estereo
        if channels != 2 {
            return Err(format!("Se esperaban dos canales de salida, se encontraron {}", channels).into());
        }

        // Crear elementos de procesamiento de audio para pasarlos al callback
        let mut eq = ThreeBandEQ::new(fs as f32, 180.0, 3500.0);
        let mut limiter = Limiter::new(
            fs as f32,
            7.0, // lookahead ms
            2.0, // attack ms
            80.0, // release ms
            1.5, // celing db
        );

        let cb_controls = controls.clone();
        let cb_source = self.source.clone();
        let cb_fs = fs;
        let cb_eq_params = eq_params.clone();
        let cb_lim_params = limiter_params.clone();
        let cb_output = output_params.clone();
        let initial_db = f32::from_bits(output_params.soft_clip_coeff.load(Ordering::Relaxed));
        let initial_threshold = 10f32.powf(initial_db / 20.0);
        let mut soft_clip = SoftClipper::new(initial_threshold);

        // Valores "anteriores" para saber si reconstruimos los biquads.
        // Como el callback corre en un solo hilo, no necesitan ser atómicos.
        let mut last_eq: [f32; 10] = [f32::NAN; 10];
        let mut last_lim_ceiling: f32 = f32::NAN;
        let mut last_lim = [0, 0, 0];
        let mut last_soft_clip_db: f32 = initial_db;

        // construir el stream de salida
        let stream = self.device.build_output_stream(
            &self.stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Cambio de pista: cadena DSP virgen. Los biquads y el
                // lookahead del limiter guardan cola del tema anterior y su
                // respuesta al escalón suena como golpe grave-medio.
                if cb_controls.dsp_reset.swap(false, Ordering::Relaxed) {
                    eq = ThreeBandEQ::new(cb_fs as f32, 180.0, 3500.0);
                    limiter = Limiter::new(cb_fs as f32, 7.0, 2.0, 80.0, 1.5);
                    last_eq = [f32::NAN; 10];
                    last_lim_ceiling = f32::NAN;
                    last_lim = [0, 0, 0];
                    // NAN fuerza al código de abajo a releer el threshold actual.
                    last_soft_clip_db = f32::NAN;
                }

                // Aplicar parámetros de eq
                let p = &cb_eq_params;
                let eq_values = [
                    p.get(&p.low_gain),
                    p.get(&p.low_resonance),
                    p.get(&p.low_shelf),
                    p.get(&p.mid_gain),
                    p.get(&p.mid_presence),
                    p.get(&p.mid_body),
                    p.get(&p.high_gain),
                    p.get(&p.high_shelf),
                    p.get(&p.high_air),
                    p.get(&p.master_gain),
                ];

                let p_lim = &cb_lim_params;
                let lim_values = [
                    p_lim.lookahead.load(Ordering::Relaxed),
                    p_lim.attack.load(Ordering::Relaxed),
                    p_lim.release.load(Ordering::Relaxed),
                ];
                let ceiling_value = f32::from_bits(p_lim.ceiling.load(Ordering::Relaxed));

                // Threshold del soft clipper: leemos en dB, convertimos a lineal.
                let soft_clip_db = f32::from_bits(cb_output.soft_clip_coeff.load(Ordering::Relaxed));
                if soft_clip_db != last_soft_clip_db {
                    let threshold_linear = 10f32.powf(soft_clip_db / 20.0);
                    soft_clip.set_threshold(threshold_linear);
                    last_soft_clip_db = soft_clip_db;
                }

                // Reconstruir solo los biquads cuyos valores cambiaron
                if eq_values[0] != last_eq[0] { eq.set_low_gain(eq_values[0]); last_eq[0] = eq_values[0]; }
                if eq_values[1] != last_eq[1] { eq.set_low_resonance(eq_values[1]); last_eq[1] = eq_values[1]; }
                if eq_values[2] != last_eq[2] { eq.set_low_shelf(eq_values[2]); last_eq[2] = eq_values[2]; }
                if eq_values[3] != last_eq[3] { eq.set_mid_gain(eq_values[3]); last_eq[3] = eq_values[3]; }
                if eq_values[4] != last_eq[4] { eq.set_mid_presence(eq_values[4]); last_eq[4] = eq_values[4]; }
                if eq_values[5] != last_eq[5] { eq.set_mid_body(eq_values[5]); last_eq[5] = eq_values[5]; }
                if eq_values[6] != last_eq[6] { eq.set_high_gain(eq_values[6]); last_eq[6] = eq_values[6]; }
                if eq_values[7] != last_eq[7] { eq.set_high_shelf(eq_values[7]); last_eq[7] = eq_values[7]; }
                if eq_values[8] != last_eq[8] { eq.set_high_air(eq_values[8]); last_eq[8] = eq_values[8]; }
                if eq_values[9] != last_eq[9] { eq.set_master_gain(eq_values[9]); last_eq[9] = eq_values[9]; }

                // Misma logica con el limitador
                if last_lim_ceiling != ceiling_value { limiter.set_ceiling_db(ceiling_value); last_lim_ceiling = ceiling_value; }
                if lim_values[0] != last_lim[0] { last_lim[0] = lim_values[0]; }
                if lim_values[1] != last_lim[1] { last_lim[1] = lim_values[1]; }
                if lim_values[2] != last_lim[2] { last_lim[2] = lim_values[2]; }

                let paused = cb_controls.paused.load(Ordering::Relaxed);
                let eq_on = cb_output.eq_enabled.load(Ordering::Relaxed);
                let limiter_on = cb_output.limiter_enabled.load(Ordering::Relaxed);
                let volume = cb_output.get_volume();
                let soft_clip_on = cb_output.soft_clip_enabled.load(Ordering::Relaxed);

                // data viene como [L, R, L, R, ...] para estereo
                let frames = data.len() / channels;
                let mut consumed: u64 = 0;

                // Fuente actual; None entre pistas: silencio sin cortar el stream.
                let mut src = cb_source.lock();
                let Some(consumer) = src.as_mut() else {
                    for s in data.iter_mut() {
                        *s = 0.0;
                    }
                    return;
                };

                for frame in 0..frames {
                    let base = frame * channels;

                    if paused {
                        for c in 0..channels {
                            data[base+c] = 0.0;
                        }
                        continue;
                    }

                    // Comprueba que haya un frame completo en el ring antes de consumir
                    if consumer.slots() < channels {
                        // Underrun. en la paractica solo pasa al principio de la reproduccion o al final de la cancion.
                        for c in 0..channels {
                            data[base+c] = 0.0;
                        }
                        continue;
                    }

                    let l = consumer.pop().unwrap();
                    let r = consumer.pop().unwrap();
                    consumed += 1;

                    // Cadena de DSP
                    
                    // EQ
                    let (l, r) = if eq_on { eq.process(l, r) } else { (l, r) };
                    // Soft Clipper
                    let (l, r) = if soft_clip_on { soft_clip.process(l, r) } else { (l, r) };
                    // Limitador
                    // Con los parametros acuales su efecto es muy suave, solo se nota en los graves fuertes (bombos pesados)  
                    // reduciendo la distorsion de los bajos.
                    let (l, r) = if limiter_on { limiter.process(l, r) } else { (l, r) };

                    // Volumen final.
                    data[base] = l * volume;
                    data[base + 1] = r * volume;
                }
                if consumed > 0 {
                    cb_controls.frames_played.fetch_add(consumed, Ordering::Relaxed);
                }
            },
            |err| eprintln!("Error en el stream de audio: {}", err),
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    // Cambia de pista sin tocar el stream: mata el ffmpeg anterior, enchufa
    // el consumer nuevo y marca la pista como vigente para el lector.
    // El swap y el aviso de reset DSP van bajo el mismo lock: el primer
    // callback con el consumer nuevo reconstruye la cadena virgen.
    // Corre en el hilo gestor, dueño único del motor.
    pub fn play_track(&mut self, path: &Path, track_id: u64, controls: &Arc<Controls>) -> Result<(), Box<dyn StdError>> {
        let fs = self.sample_rate();
        let channels = self.stream_config.channels as usize;
        controls.current_track.store(track_id, Ordering::Relaxed);
        let (consumer, streamer) = spawn_streamer(path, fs, channels as u16, controls.clone(), track_id)?;
        {
            let mut src = self.source.lock();
            *src = Some(consumer);
            controls.dsp_reset.store(true, Ordering::Relaxed);
        }
        // Soltar el viejo mata su ffmpeg y une su lector (ya silenciado por track_id).
        self.streamer = Some(streamer);
        controls.song_loaded.store(true, Ordering::Relaxed);
        println!("Cancion cargada.");
        Ok(())
    }
}
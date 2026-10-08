use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::error::Error as StdError;
use rtrb::Consumer;

use crate::audio::eq::ThreeBandEQ;
use crate::audio::engine::Limiter;
use crate::audio::loader::{spawn_streamer, Streamer};


pub struct AudioEngine {
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    streamer: Option<Streamer>,
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
        println!("Buffer frames: {} Frames: {}", buffer_frames, frames);
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
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.stream_config.sample_rate.0
    }

    pub fn channels(&self) -> u16 {
        self.stream_config.channels
    }

    pub fn play(&mut self, path: &Path, eq: ThreeBandEQ) -> Result<(), Box<dyn StdError>> {
        let fs = self.sample_rate();
        let channels = self.stream_config.channels as usize;

        // por defecto exigimos solo estereo
        if channels != 2 {
            return Err(format!("Se esperaban dos canales de salida, se encontraron {}", channels).into());
        }

        // Crear el streamer con ffmpeg
        let (consumer, streamer) = spawn_streamer(path, fs, channels as u16)?;
        self.streamer = Some(streamer);

        // Crear elementos de procesamiento de audio para pasarlos al callback
        let mut eq = eq;
        let mut limiter = Limiter::new(
            fs as f32,
            7.0, // lookahead ms
            2.0, // attack ms
            80.0, // release ms
            1.5, // celing db
        );
        let mut consumer = consumer;

        // construir el stream de salida
        let stream = self.device.build_output_stream(
            &self.stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                process_callback(data, channels, &mut consumer, &mut eq, &mut limiter);
            },
            |err| eprintln!("Error en el stream de audio: {}", err),
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
    }
}

pub fn process_callback(
    data: &mut [f32],
    channels: usize,
    consumer: &mut Consumer<f32>,
    eq: &mut ThreeBandEQ,
    limiter: &mut Limiter,
) {
    // data viene como [L, R, L, R, ...] para estereo
    let frames = data.len() / channels;

    for frame in 0..frames {
        let base = frame * channels;

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

        // Cadena de DSP
        let (l, r) = eq.process(l, r);
        // Con los parametros acuales su efecto es muy suave, solo se nota en los graves fuertes (bombos pesados)  
        // reduciendo la distorsion de los bajos.
        let (l, r) = limiter.process(l, r);

        data[base] = l;
        data[base + 1] = r;
    }
}
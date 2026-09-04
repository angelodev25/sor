use audioadapter_buffers::direct::SequentialSliceOfVecs;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{Async, Resampler, SincInterpolationParameters};

use crate::audio::types::AudioConfig;
use crate::audio::eq::ThreeBandEQ;
use crate::audio::engine::Limiter;

use std::cell::{Cell, RefCell};
use std::error::Error as StdError;

pub struct AudioEngine {
    config: AudioConfig,
    device: cpal::Device,
    stream: Option<cpal::Stream>,
}

impl AudioEngine {
    pub fn new(config: AudioConfig) -> Result<Self, Box<dyn StdError>> {
       
        let host = cpal::default_host();
        let device = host.default_output_device().expect("No se encontro dispositivo de audio");
        Ok(Self { config, device, stream: None })
    }

    fn get_device_config(&self, target_sample_rate: u32) -> Result<cpal::StreamConfig, Box<dyn StdError>> {
        let supported_configs = self.device.supported_output_configs()?;
        let config = supported_configs
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
            .find(|c| {
                c.min_sample_rate().0 <= 44100
                    && c.max_sample_rate().0 >= 44100
                    && c.channels() == self.config.channels
            })
            .ok_or(format!("No se encontró configuración compatible para {} Hz y estéreo", target_sample_rate))?
            .with_sample_rate(cpal::SampleRate(44100))
            .config();

        Ok(cpal::StreamConfig {
            channels: config.channels,
            sample_rate: config.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        })
    }

    pub fn play(
        &mut self,
        samples: Vec<f32>,
        sample_rate: f32,
        channels: u16,
        eq: ThreeBandEQ,
    ) -> Result<(), Box<dyn StdError>> {
        let config = self.get_device_config(sample_rate as u32)?;
        let device_sample_rate = config.sample_rate.0;
        let device_channels = config.channels as usize;

        let ratio = device_sample_rate as f64 / sample_rate as f64;
        println!("Relación de remuestreo: {:.3}", ratio);

        let file_channels = channels as usize;

        // Solo creamos el remuestreador si ratio != 1.0
        let mut resampler = if (ratio - 1.0).abs() > 0.001 {
            let params = SincInterpolationParameters {
                sinc_len: 1024,
                f_cutoff: Some(0.93),
                interpolation: rubato::SincInterpolationType::Cubic,
                oversampling_factor: 16,
                window: rubato::WindowFunction::BlackmanHarris2,
            };

            Some(Async::new_sinc(
                ratio,
                2.0,
                &params,
                1024,
                file_channels,
                rubato::FixedAsync::Output,
            )?)
        } else {
            None
        };

        let sample_index = Cell::new(0);
        let samples_len = samples.len();
        let ring_buffer = RefCell::new(Vec::with_capacity(8192));
        let input_vecs_buffer = RefCell::new(Vec::<Vec<f32>>::new());
        let eq_cell = RefCell::new(eq);
		let limiter = RefCell::new(Limiter::new(0.9, 0.99)); // ceiling = 0.95

        let stream = self.device.build_output_stream(
			&config,
			move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
				let mut idx = sample_index.get();
				let mut ring = ring_buffer.borrow_mut();

				// 1) Llenar el ring buffer desde la fuente (con o sin resampler)
				if let Some(resampler) = &mut resampler {
					let mut input_vecs = input_vecs_buffer.borrow_mut();
					let needed_output_samples = data.len();

					while ring.len() < needed_output_samples && idx < samples_len {
						let needed_input_frames = resampler.input_frames_next();
						if needed_input_frames == 0 { break; }

						if input_vecs.len() != file_channels
							|| input_vecs[0].len() != needed_input_frames {
							*input_vecs = vec![vec![0.0f32; needed_input_frames]; file_channels];
						}

						for frame in 0..needed_input_frames {
							for chan in 0..file_channels {
								if idx < samples_len {
									input_vecs[chan][frame] = samples[idx];
									idx += 1;
								} else {
									input_vecs[chan][frame] = 0.0;
								}
							}
						}

						let input_buffer = SequentialSliceOfVecs::new(
							&*input_vecs, file_channels, needed_input_frames
						).unwrap();

						if let Ok(output) = resampler.process(&input_buffer, None) {
							use audioadapter::Adapter;
							for frame in 0..output.frames() {
								for chan in 0..device_channels {
									let src = if chan < output.channels() { chan } else { 0 };
									ring.push(output.read_sample(src, frame).unwrap_or(0.0));
								}
							}
						} else { break; }
					}
				} else {
					// Sin resampler: copia directa al ring
					let needed = data.len();
					while ring.len() < needed && idx < samples_len {
						ring.push(samples[idx]);
						idx += 1;
					}
				}

				// 2) Pipeline único de procesamiento para ambos paths
				let samples_to_copy = std::cmp::min(ring.len(), data.len());
				for i in 0..samples_to_copy {
					let s = ring[i];
					let s = eq_cell.borrow_mut().process(s);
					let s = limiter.borrow_mut().process(s);   // ceiling = 0.95
					data[i] = s; // sin dithering
				}
				ring.drain(0..samples_to_copy);

				// Rellena con silencio si faltó fuente
				for i in samples_to_copy..data.len() {
					data[i] = 0.0;
				}

				sample_index.set(idx);
			},
			|err| eprintln!("Error en el stream: {}", err),
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
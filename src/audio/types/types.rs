pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

// Configuración del motor de audio

#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
        }
    }
}
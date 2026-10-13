use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// Estado compartido entre la UI y el callback de audio
pub struct Controls {
    pub paused: AtomicBool,
    pub song_ended: AtomicBool,
    pub song_loaded: AtomicBool,
    pub quit: AtomicBool,
    pub frames_played: AtomicU64,
    pub duration_secs: AtomicU32,
}

impl Controls {
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            song_ended: AtomicBool::new(false),
            song_loaded: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            frames_played: AtomicU64::new(0),
            duration_secs: AtomicU32::new(f32::to_bits(0.0)),
        }
    }

    pub fn reset_progress(&self, duration_secs: f32) {
        self.frames_played.store(0, Ordering::Relaxed);
        self.duration_secs.store(f32::to_bits(duration_secs), Ordering::Relaxed);
    }

    pub fn progress(&self, sample_rate: u32) -> (f32, f32) {
        let duration = f32::from_bits(self.duration_secs.load(Ordering::Relaxed));
        let elapsed = self.frames_played.load(Ordering::Relaxed) as f32 / sample_rate.max(1) as f32;
        let ratio = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 0.0 };
        (ratio, elapsed)
    }
}

/// Parámetros del EQ compartidos con la UI.
/// Cada campo es un AtomicU32 que guarda los bits de un f32.
pub struct EqParams {
    pub low_gain: AtomicU32,
    pub low_resonance: AtomicU32,
    pub low_shelf: AtomicU32,
    pub mid_gain: AtomicU32,
    pub mid_presence: AtomicU32,
    pub mid_body: AtomicU32,
    pub high_gain: AtomicU32,
    pub high_shelf: AtomicU32,
    pub high_air: AtomicU32,
    pub master_gain: AtomicU32,
}

impl EqParams {
    pub fn new() -> Self {
        Self {
            low_gain: AtomicU32::new(f32::to_bits(0.0)),
            low_resonance: AtomicU32::new(f32::to_bits(0.0)),
            low_shelf: AtomicU32::new(f32::to_bits(0.0)),
            mid_gain: AtomicU32::new(f32::to_bits(0.0)),
            mid_presence: AtomicU32::new(f32::to_bits(0.0)),
            mid_body: AtomicU32::new(f32::to_bits(0.0)),
            high_gain: AtomicU32::new(f32::to_bits(0.0)),
            high_shelf: AtomicU32::new(f32::to_bits(0.0)),
            high_air: AtomicU32::new(f32::to_bits(0.0)),
            master_gain: AtomicU32::new(f32::to_bits(0.8)),
        }
    }

    /// Lee un f32 del atómico.
    #[inline]
    pub fn get(&self, field: &AtomicU32) -> f32 {
        f32::from_bits(field.load(Ordering::Relaxed))
    }

    /// Escribe un f32 al atómico.
    #[inline]
    pub fn set(&self, field: &AtomicU32, v: f32) {
        field.store(f32::to_bits(v), Ordering::Relaxed);
    }
}

pub struct LimiterParams {
    pub ceiling: AtomicU32,
    pub lookahead: AtomicU32,
    pub attack: AtomicU32,
    pub release: AtomicU32,
}

impl LimiterParams {
    pub fn new() -> Self {
        Self {
            ceiling: AtomicU32::new(f32::to_bits(1.0)),
            lookahead: AtomicU32::new(8),
            attack: AtomicU32::new(2),
            release: AtomicU32::new(80),
        }
    }
}

pub struct OutputParams {
    pub volume: AtomicU32,
    pub eq_enabled: AtomicBool,
    pub limiter_enabled: AtomicBool,
    pub soft_clip_enabled: AtomicBool,
    pub soft_clip_coeff: AtomicU32,
}

impl OutputParams {
    pub fn new() -> Self {
        Self {
            volume: AtomicU32::new(f32::to_bits(1.0)),
            eq_enabled: AtomicBool::new(true),
            limiter_enabled: AtomicBool::new(true),
            soft_clip_enabled: AtomicBool::new(true),
            // -3 dBFS: threshold lineal ≈ 0.71.
            soft_clip_coeff: AtomicU32::new(f32::to_bits(-3.0)),
        }
    }

    #[inline]
    pub fn get_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn set_volume(&self, v: f32) {
        self.volume.store(f32::to_bits(v), Ordering::Relaxed);
    }
}
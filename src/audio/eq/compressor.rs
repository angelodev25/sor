pub struct Compressor {
    /// Umbral en dominio lineal (amplitud). Se pasa en dB al constructor.
    threshold: f32,
    /// Ratio de compresión (p. ej. 2.0 = 2:1).
    ratio: f32,
    /// Coeficiente de suavizado para subidas (attack).
    attack_coeff: f32,
    /// Coeficiente para bajadas (release).
    release_coeff: f32,
    /// Valor actual de la envolvente (dominio lineal).
    envelope: f32,
    /// Ganancia de compensación post-compresión. 1.0 = neutro.
    makeup: f32,
}

impl Compressor {
    pub fn new(
        fs: f32,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
    ) -> Self {
        Self {
            threshold: 10f32.powf(threshold_db / 20.0), // convertir a dB lineal,
            ratio,
            attack_coeff: (-1.0 / (attack_ms * 0.001 * fs)).exp(),
            release_coeff: (-1.0 / (release_ms * 0.001 * fs)).exp(),
            envelope: 0.0,
            makeup: 1.0,  
        }
    }
    
    #[inline]
    pub fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        // Calcular la envolvente como el máximo absoluto de ambos canales
        let detector = l.abs().max(r.abs());
        
        // Actualizar la envolvente con suavizado
        let coeff = if detector > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = detector + coeff * (self.envelope - detector);

        let gain = if self.envelope > self.threshold {
            (self.threshold + (self.envelope - self.threshold) / self.ratio) / self.envelope
        } else {
            1.0
        };

        let g = gain * self.makeup;
        (l * g, r * g)
    }

    pub fn set_makeup(&mut self, makeup: f32) {
        self.makeup = makeup;
    }
}
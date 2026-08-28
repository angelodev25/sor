pub struct Compressor {
    threshold: f32,   // típicamente -0.5 a -0.7 (lineal)
    ratio: f32,      // 2.0 — suave
    attack: f32,     // 0.99 — rápido (5ms)
    release: f32,    // 0.9995 — lento (50ms)
    envelope: f32,
    makeup_gain: f32,
}

impl Compressor {
    pub fn new_for_bass() -> Self {
        Self {
            threshold: 0.6,
            ratio: 2.0,
            attack: 0.997,
            release: 0.99,
            envelope: 0.0,
            makeup_gain: 1.4,  
        }
    }

    pub fn new_for_highs() -> Self {
        Self {
            threshold: 0.4,
            ratio: 1.8,
            attack: 0.85,
            release: 0.94,
            envelope: 0.0,
            makeup_gain: 1.8,  
        }
    }

    pub fn new_for_vocals() -> Self {
        Self {
            threshold: 0.3, 
            ratio: 1.8,       
            attack: 0.9,       
            release: 0.997,    
            envelope: 0.0,
            makeup_gain: 1.32, 
        }
    }
    
    pub fn process(&mut self, sample: f32) -> f32 {
        let abs_in = sample.abs();
        if abs_in > self.envelope {
            self.envelope = abs_in * (1.0 - self.attack) + self.envelope * self.attack;
        } else {
            self.envelope = abs_in * (1.0 - self.release) + self.envelope * self.release;
        }
        
        let gain_reduction = if self.envelope > self.threshold {
            let over = self.envelope - self.threshold;
            let reduced = over / self.ratio;
            (self.threshold + reduced) / self.envelope
        } else {
            1.0
        };
        
        sample * gain_reduction * self.makeup_gain
    }
}
pub struct Limiter {
    envelope: f32,
    ceiling: f32,
    release: f32,
}

impl Limiter {
    pub fn new(ceiling: f32, release: f32) -> Self {
        Self {
            envelope: 0.0,
            ceiling,
            release,
        }
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let abs_sample = sample.abs();
        if abs_sample > self.envelope {
            self.envelope = abs_sample;
        } else {
            self.envelope *= self.release;
        }

        if self.envelope > self.ceiling {
            sample * (self.ceiling / self.envelope)
        } else {
            sample
        }
    }
}
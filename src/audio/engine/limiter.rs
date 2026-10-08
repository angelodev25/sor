pub struct Limiter {
    /// Buffer de señal retrasadas por canal
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,

    write: usize,
    lookahead: usize,
    envelope: f32,
    attack: f32,
    release: f32,
    ceiling: f32,
    makeup: f32,
}

impl Limiter {
    pub fn new(fs: f32, lookahead_ms: f32, attack_ms: f32, release_ms: f32, ceiling: f32) -> Self {
        // Minimo una muestra para ecitar division por cero
        let n = ((lookahead_ms * 0.001 * fs).round() as usize).max(1);

        // Coeficientes de suavizado exponencial
        let attack = (-1.0 / (attack_ms * 0.001 * fs)).exp();
        let release = (-1.0 / (release_ms * 0.001 * fs)).exp();

        Self {
            delay_l: vec![0.0; n],
            delay_r: vec![0.0; n],
            write: 0,
            lookahead: n,
            attack,
            release,
            envelope: 0.0,
            ceiling: 10f32.powf(ceiling / 20.0),
            makeup: 1.0,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        // detector sobre la señal de entrada que esta lookahead muestra por delante de la salida
        let detector = l.abs().max(r.abs());

        // Envolvente asimetrica
        let coeff = if detector > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = detector + coeff * (self.envelope - detector);

        // Leer la muestra retrasada que sale AHORA y guardar la actual que salda en lookahead muestras
        let delayed_l = self.delay_l[self.write];
        let delayed_r = self.delay_r[self.write];
        self.delay_l[self.write] = l;
        self.delay_r[self.write] = r;
        self.write = (self.write + 1) % self.lookahead;

        // ganancia. si la envolvente supera el techo se reducen ambos canales por igual
        let gain = if self.envelope > self.ceiling {
            self.ceiling / self.envelope
        } else {
            1.0
        } * self.makeup;

        (delayed_l * gain, delayed_r * gain)
    }

    pub fn set_ceiling_db(&mut self, ceiling_db: f32) {
        self.ceiling = 10f32.powf(ceiling_db / 20.0);
    }
    pub fn set_makeup(&mut self, makeup: f32) {
        self.makeup = makeup;
    }
}
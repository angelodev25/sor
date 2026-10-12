//! Soft clipper con curva tanh.
//!
//! Caracteristicas:
//!   - Por debajo de `threshold`: la señal pasa intacta (sin cambio,
//!     sin distorsion, sin DC).
//!   - Por encima: comprime con una curva tangente hiperbolica que
//!     asintota a `ceiling` (por defecto 0.95). Deja headroom real
//!     para el DAC.
//!   - Curva C2 continua en el umbral: sin "grit" al entrar en la
//!     zona de compresion.
//!   - Solo armonicos impares (curva simetrica): saturacion calida,
//!     no "digital".

pub struct SoftClipper {
    threshold: f32,
    ceiling: f32,
    /// Cache: ceiling - threshold. Recalculado al cambiar threshold.
    span: f32,
}

impl SoftClipper {
    /// Constructor con ceiling por defecto
    pub fn new(threshold: f32) -> Self {
        Self::with_ceiling(threshold, 0.98)
    }

    pub fn with_ceiling(threshold: f32, ceiling: f32) -> Self {
        // El ceiling debe estar por debajo de 1.0 pero no demasiado
        // bajo, o el soft clipper se convierte en un limitador agresivo.
        let ceiling = ceiling.clamp(0.5, 1.0);
        // El threshold debe ser menor que el ceiling y positivo.
        let threshold = threshold.clamp(0.001, ceiling - 0.001);
        Self {
            threshold,
            ceiling,
            span: ceiling - threshold,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.clip(l), self.clip(r))
    }

    #[inline]
    fn clip(&self, x: f32) -> f32 {
        let a = x.abs();
        // Region lineal: por debajo del umbral, intacto.
        if a <= self.threshold {
            return x;
        }
        let over = a - self.threshold;
        // Curva tanh escalada al span. Verifica:
        //   f(0) = threshold                 (continuidad)
        //   f'(0) = 1                        (knee suave)
        //   f(∞) = threshold + span = ceiling (asintota)
        let compressed = self.threshold + self.span * (over / self.span).tanh();
        x.signum() * compressed
    }

    /// Cambia el umbral. `threshold` en amplitud lineal (0..1).
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.001, self.ceiling - 0.001);
        self.span = self.ceiling - self.threshold;
    }

    /// Cambia el ceiling. `ceiling` en amplitud lineal (0.5..1.0).
    pub fn set_ceiling(&mut self, ceiling: f32) {
        self.ceiling = ceiling.clamp(0.5, 1.0);
        self.threshold = self.threshold.clamp(0.001, self.ceiling - 0.001);
        self.span = self.ceiling - self.threshold;
    }
}
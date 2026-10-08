//! Ecualizador de 3 bandas estéreo, con crossover Linkwitz-Riley 4º orden,
//! filtros de carácter y compresores estéreo-link por banda.

use biquad::{Biquad as _, Coefficients, DirectForm2Transposed, Hertz, Type};
use crate::audio::eq::Compressor;

const Q1: f32 = 0.5412;   // Linkwitz-Riley Q1
const Q2: f32 = 1.3066;   // Linkwitz-Riley Q2

struct StereoBiquad {
    l: DirectForm2Transposed<f32>,
    r: DirectForm2Transposed<f32>,
}

impl StereoBiquad {
    fn new(coeffs: Coefficients<f32>) -> Self {
        Self {
            l: DirectForm2Transposed::new(coeffs),
            r: DirectForm2Transposed::new(coeffs),
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.l.run(l), self.r.run(r))
    }
}

// Estructura principal del motor
pub struct ThreeBandEQ {
     // --- HPF subsonic (LR4 a 20 Hz) ---
    hpf1: StereoBiquad,
    hpf2: StereoBiquad,

    // --- Crossover LR4 (reparte la señal en graves / medios / agudos) ---
    // Graves: LP4 @ low_freq
    lp_low1: StereoBiquad,
    lp_low2: StereoBiquad,
    // Medios: HP4 @ low_freq  +  LP4 @ high_freq  (pasabanda)
    hp_mid1: StereoBiquad,
    hp_mid2: StereoBiquad,
    lp_mid1: StereoBiquad,
    lp_mid2: StereoBiquad,
    // Agudos: HP4 @ high_freq
    hp_high1: StereoBiquad,
    hp_high2: StereoBiquad,

    // --- Procesamiento por banda ---
    /// Peaking @ 65 Hz para reforzar/cortar resonancia de graves.
    low_resonance: StereoBiquad,
    /// Shelf @ 180 Hz para reforzar/cortar cuerpo de graves.
    low_shelf: StereoBiquad,
    /// Peaking @ 300 Hz para "cuerpo" de medios.
    mid_body: StereoBiquad,
    /// Peaking @ 2200 Hz para "presencia".
    mid_presence: StereoBiquad,
    /// High shelf @ 2000 Hz.
    high_shelf: StereoBiquad,
    /// Peaking @ 9000 Hz para "aire".
    high_air: StereoBiquad,

    // --- Compresores por banda (estéreo-link) ---
    bass_compressor: Compressor,
    mid_compressor: Compressor,
    highs_compressor: Compressor,

    // --- Ganancias (lineales) ---
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
    master_gain: f32,

    sample_rate: f32,
}

fn make_lp(fs: Hertz<f32>, freq: f32, q: f32) -> Coefficients<f32> {
    Coefficients::<f32>::from_params(
        Type::LowPass, fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap()
}

fn make_hp(fs: Hertz<f32>, freq: f32, q: f32) -> Coefficients<f32> {
    Coefficients::<f32>::from_params(
        Type::HighPass, fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap()
}

fn make_peaking(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> Coefficients<f32> {
    Coefficients::<f32>::from_params(
        Type::PeakingEQ(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap()
}

fn make_low_shelf(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> Coefficients<f32> {
    Coefficients::<f32>::from_params(
        Type::LowShelf(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap()
}

fn make_high_shelf(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> Coefficients<f32> {
    Coefficients::<f32>::from_params(
        Type::HighShelf(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap()
}

impl ThreeBandEQ {
    pub fn new(sample_rate: f32, low_freq: f32, high_freq: f32) -> Self {
        let fs = Hertz::from_hz(sample_rate).unwrap();
        Self {
            // Subsonic HPF
            hpf1: StereoBiquad::new(make_hp(fs, 20.0, Q1)),
            hpf2: StereoBiquad::new(make_hp(fs, 20.0, Q2)),

            // Crossovers
            lp_low1: StereoBiquad::new(make_lp(fs, low_freq, Q1)),
            lp_low2: StereoBiquad::new(make_lp(fs, low_freq, Q2)),
            hp_mid1: StereoBiquad::new(make_hp(fs, low_freq, Q1)),
            hp_mid2: StereoBiquad::new(make_hp(fs, low_freq, Q2)),
            lp_mid1: StereoBiquad::new(make_lp(fs, high_freq, Q1)),
            lp_mid2: StereoBiquad::new(make_lp(fs, high_freq, Q2)),
            hp_high1: StereoBiquad::new(make_hp(fs, high_freq, Q1)),
            hp_high2: StereoBiquad::new(make_hp(fs, high_freq, Q2)),

            // Carácter
            low_resonance: StereoBiquad::new(make_peaking(fs, 65.0, 1.15, 0.0)),
            low_shelf: StereoBiquad::new(make_low_shelf(fs, 180.0, 0.6, 0.0)),
            mid_body: StereoBiquad::new(make_peaking(fs, 300.0, 1.2, 0.0)),
            mid_presence: StereoBiquad::new(make_peaking(fs, 2200.0, 0.8, 0.0)),
            high_shelf: StereoBiquad::new(make_high_shelf(fs, 4500.0, 1.0, 0.0)),
            high_air: StereoBiquad::new(make_peaking(fs, 9000.0, 1.0, 0.0)),

            // Compresores 
            bass_compressor:  Compressor::new(sample_rate, 0.0, 2.0, 10.0, 100.0),
            mid_compressor:   Compressor::new(sample_rate, -6.0, 2.0, 5.0, 80.0),
            highs_compressor: Compressor::new(sample_rate, 3.0, 2.0, 3.0, 60.0),

            low_gain: 1.0,
            mid_gain: 1.0,
            high_gain: 1.0,
            master_gain: 1.0,

            sample_rate,
        }
    }
    
    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // 1) HPF subsonic
        let (l, r) = self.hpf1.process(in_l, in_r);
        let (l, r) = self.hpf2.process(l, r);
        
        // 2) Split en 3 bandas con crossovers LR4
        // Low: LP4 a low_freq
        let (low_l, low_r) = self.lp_low1.process(l, r);
        let (low_l, low_r) = self.lp_low2.process(low_l, low_r);
        
        // Mid: HP4 a low_freq + LP4 a high_freq
        let (mid_l, mid_r) = self.hp_mid1.process(l, r);
        let (mid_l, mid_r) = self.hp_mid2.process(mid_l, mid_r);
        let (mid_l, mid_r) = self.lp_mid1.process(mid_l, mid_r);
        let (mid_l, mid_r) = self.lp_mid2.process(mid_l, mid_r);

        
        // High: HP4 a high_freq
        let (high_l, high_r) = self.hp_high1.process(l, r);
        let (high_l, high_r) = self.hp_high2.process(high_l, high_r);
        
        // Procesameinto por banda

        // Graves: compressor, resonance, shelf, gain
        self.bass_compressor.set_makeup(1.06); // leve refuerzo de graves
        let (low_l, low_r) = self.bass_compressor.process_stereo(low_l, low_r);
        let (low_l, low_r) = self.low_resonance.process(low_l, low_r);
        let (low_l, low_r) = self.low_shelf.process(low_l, low_r);
        let low_l = low_l * self.low_gain;
        let low_r = low_r * self.low_gain;

        // Medios: body → presence → compresor → gain
        let (mid_l, mid_r) = self.mid_body.process(mid_l, mid_r);
        let (mid_l, mid_r) = self.mid_presence.process(mid_l, mid_r);
        let (mid_l, mid_r) = self.mid_compressor.process_stereo(mid_l, mid_r);
        let mid_l = mid_l * self.mid_gain;
        let mid_r = mid_r * self.mid_gain;

        // Agudos: compresor → shelf → air → gain
        let (high_l, high_r) = self.highs_compressor.process_stereo(high_l, high_r);
        let (high_l, high_r) = self.high_shelf.process(high_l, high_r);
        let (high_l, high_r) = self.high_air.process(high_l, high_r);
        let high_l = high_l * self.high_gain;
        let high_r = high_r * self.high_gain;

        // Mezcla de las tres bandas
        let out_l = (low_l + mid_l + high_l) * self.master_gain;
        let out_r = (low_r + mid_r + high_r) * self.master_gain;
        (out_l, out_r)
    }
    
    // Setters por banda

    pub fn set_low_gain(&mut self, gain: f32) {
        self.low_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_low_resonance(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.low_resonance = StereoBiquad::new(make_peaking(fs, 65.0, 1.15, gain));
    }
    pub fn set_low_shelf(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.low_shelf = StereoBiquad::new(make_low_shelf(fs, 180.0, 0.6, gain));
    }

    pub fn set_mid_gain(&mut self, gain: f32) {
        self.mid_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_mid_presence(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.mid_presence = StereoBiquad::new(make_peaking(fs, 2200.0, 0.8, gain));
    }
    pub fn set_mid_body(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.mid_body = StereoBiquad::new(make_peaking(fs, 300.0, 1.2, gain));
    }

    pub fn set_high_gain(&mut self, gain: f32) {
        self.high_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_high_shelf(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.high_shelf = StereoBiquad::new(make_high_shelf(fs, 5000.0, 1.0, gain));
    }
    pub fn set_high_air(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.high_air = StereoBiquad::new(make_peaking(fs, 9000.0, 1.0, gain));
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = gain;
    }
}
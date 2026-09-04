use biquad::{Biquad as _, Coefficients, DirectForm1, Hertz, Type};
use crate::audio::eq::Compressor;

const Q1: f32 = 0.5412;   // Linkwitz-Riley Q1
const Q2: f32 = 1.3066;   // Linkwitz-Riley Q2

// Estructura principal del motor
pub struct ThreeBandEQ {
    // HPF subsonic
    hpf1: DirectForm1<f32>,
    hpf2: DirectForm1<f32>,
    // Crossovers LR4
    // Low band: LP4 a low_freq
    lp_low1: DirectForm1<f32>,
    lp_low2: DirectForm1<f32>,
    // Mid band: HP a low_freq + LP a high_freq
    hp_mid1: DirectForm1<f32>,
    hp_mid2: DirectForm1<f32>,
    lp_mid1: DirectForm1<f32>,
    lp_mid2: DirectForm1<f32>,
    mid_body: DirectForm1<f32>,
    // High band: HP4 a high_freq
    hp_high1: DirectForm1<f32>,
    hp_high2: DirectForm1<f32>,
    
    // Procesameinto por banda 
    low_resonance: DirectForm1<f32>,
    low_shelf: DirectForm1<f32>,
    high_shelf: DirectForm1<f32>,
    mid_presence: DirectForm1<f32>,
    high_air: DirectForm1<f32>,

    bass_compressor: Compressor,
    highs_compressor: Compressor,
    mid_compressor: Compressor,
    
    // Gains por banda (lineales, no dB)
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
    master_gain: f32,

    sample_rate: f32,
}

fn make_lp(fs: Hertz<f32>, freq: f32, q: f32) -> DirectForm1<f32> {
    let c = Coefficients::<f32>::from_params(
        Type::LowPass, fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap();
    DirectForm1::new(c)
}

fn make_hp(fs: Hertz<f32>, freq: f32, q: f32) -> DirectForm1<f32> {
    let c = Coefficients::<f32>::from_params(
        Type::HighPass, fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap();
    DirectForm1::new(c)
}

fn make_peaking(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> DirectForm1<f32> {
    let c = Coefficients::<f32>::from_params(
        Type::PeakingEQ(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap();
    DirectForm1::new(c)
}

fn make_low_shelf(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> DirectForm1<f32> {
    let c = Coefficients::<f32>::from_params(
        Type::LowShelf(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap();
    DirectForm1::new(c)
}

fn make_high_shelf(fs: Hertz<f32>, freq: f32, q: f32, gain: f32) -> DirectForm1<f32> {
    let c = Coefficients::<f32>::from_params(
        Type::HighShelf(gain), fs, Hertz::from_hz(freq).unwrap(), q,
    ).unwrap();
    DirectForm1::new(c)
}

impl ThreeBandEQ {
    pub fn new(sample_rate: f32, low_freq: f32, high_freq: f32) -> Self {
        let fs = Hertz::from_hz(sample_rate).unwrap();
        // HPF subsonic a 20 Hz, LR4
        let hpf1 = make_hp(fs, 20.0, Q1);
        let hpf2 = make_hp(fs, 20.0, Q2);

        
        Self {
            hpf1, hpf2,
            lp_low1: make_lp(fs, low_freq, Q1),
            lp_low2: make_lp(fs, low_freq, Q2),
            hp_mid1: make_hp(fs, low_freq, Q1),
            hp_mid2: make_hp(fs, low_freq, Q2),
            lp_mid1: make_lp(fs, high_freq, Q1),
            lp_mid2: make_lp(fs, high_freq, Q2),
            hp_high1: make_hp(fs, high_freq, Q1),
            hp_high2: make_hp(fs, high_freq, Q2),
            low_gain: 1.0,
            mid_gain: 1.0,
            high_gain: 1.0,
            master_gain: 0.85,
            sample_rate,
            low_resonance: make_peaking(fs, 65.0, 1.15, 0.0),
            low_shelf: make_low_shelf(fs, 180.0, 0.6, 0.0),
            mid_presence: make_peaking(fs, 2200.0, 0.8, 0.0),
            mid_body: make_peaking(fs, 300.0, 1.2, 0.0),
            high_air: make_peaking(fs, 8000.0, 1.0, 0.0),
            high_shelf: make_high_shelf(fs, 2000.0, 1.0, 0.0),
            bass_compressor: Compressor::new_for_bass(),
            mid_compressor: Compressor::new_for_vocals(),
            highs_compressor: Compressor::new_for_highs(),
        }
    }
    
    #[inline]
    pub fn process(&mut self, sample: f32) -> f32 {
        // 1) HPF subsonic
        let s = self.hpf1.run(sample);
        let s = self.hpf2.run(s);

        // Este filtro se llama dos veces para la misma muestra, la primera antes de cualquier crossover y la segunda despues del crossover de bajo.
        // Esto comenzo como un bug de edicion pero se notó que el filtro de resonancia de bajos tiene un efecto más pronunciado 
        // si se aplica antes del crossover, y el resultado es (mucho) más agradable al oído.
        let s = self.low_resonance.run(s);
        
        // 2) Split en 3 bandas con crossovers LR4
        let mut low = self.lp_low1.run(s);
        low = self.lp_low2.run(low);
        
        // Mid: HP4 a low_freq + LP4 a high_freq
        let mut mid = self.hp_mid1.run(s);
        mid = self.hp_mid2.run(mid);
        mid = self.lp_mid1.run(mid);
        mid = self.lp_mid2.run(mid);
        
        // High: HP4 a high_freq
        let mut high = self.hp_high1.run(s);
        high = self.hp_high2.run(high);
        
        // Procesameinto por banda

        // Bajos
        let low = self.bass_compressor.process(low);
        let low = self.low_resonance.run(low);
        let low = self.low_shelf.run(low);
        let low = low * self.low_gain;

        // Medios
        let mid = self.mid_body.run(mid);
        let mid = self.mid_presence.run(mid);
        let mid = self.mid_compressor.process(mid);
        let mid = mid * self.mid_gain;

        // Agudos
        let high = self.highs_compressor.process(high);
        let high = self.high_shelf.run(high);
        let high = self.high_air.run(high);
        let high = high * self.high_gain;

        // Mezcla de las tres bandas
        let mix = low + mid + high;
        mix * self.master_gain
    }
    
    // Setters por banda

    pub fn set_low_gain(&mut self, gain: f32) {
        self.low_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_low_resonance(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.low_resonance = make_peaking(fs, 65.0, 1.15, gain);
    }
    pub fn set_low_shelf(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.low_shelf = make_low_shelf(fs, 180.0, 0.6, gain);
    }

    pub fn set_mid_gain(&mut self, gain: f32) {
        self.mid_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_mid_presence(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.mid_presence = make_peaking(fs, 2200.0, 0.8, gain);
    }
    pub fn set_mid_body(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.mid_body = make_peaking(fs, 300.0, 0.7, gain);
    }

    pub fn set_high_gain(&mut self, gain: f32) {
        self.high_gain = 10.0_f32.powf(gain / 20.0);
    }
    pub fn set_high_shelf(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        // Nota: este shelf se establecio a 2000Hz mientras hacia pruebas empiricas para encontrar mejor calidad de sonido. Se descubrio que al establecerlo en 2000hz con 
        // este Q especifico ayuda a reforzar frecuencia arriba de 2000 (2500 - 4000) las cuales cortaba el hp_high con corte en 4000 el cual atenuaba esas frecuencias.
        // Importante dejarlo asi para sonido con mejor definicion
        self.high_shelf = make_high_shelf(fs, 2000.0, 1.0, gain);
    }
    pub fn set_high_air(&mut self, gain: f32) {
        let fs = Hertz::from_hz(self.sample_rate).unwrap();
        self.high_air = make_peaking(fs, 9000.0, 1.0, gain);
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = gain;
    }
}
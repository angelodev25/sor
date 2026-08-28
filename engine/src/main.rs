use std::path::Path;
use std::error::Error as StdError;

mod audio;
mod eq;
mod loader;
mod types;

use audio::AudioEngine;
use types::AudioConfig; 
use eq::ThreeBandEQ;
use loader::load_audio_file;

fn main() -> Result<(), Box<dyn StdError>> {
    // Mis accesos rapidos.
    //
    // BAD OMENS  Like A Villain (Official Music Video).m4a
    // Coldplay  Yellow (Official Video).m4a
    // Arctic Monkeys  Do I Wanna Know_.m4a
    // Martin Garrix & MOTI - Virus.mp3
    // Sander van Doorn, Martin Garrix, DVBBS - Gold Skies (ft. Aleesia) [Official Music Video] OUT NOW (128 kbps).mp3
    // BRING ME THE HORIZON  Drown (Album Version).m4a
    // Alok & Alan Walker - Headlights (feat. KIDDO) [Official Lyric Video] (192 kbps).mp3
    // Avicii - Levels (192 kbps).mp3
    // Avicii  Levels.m4a
    let path = Path::new("BAD OMENS  Like A Villain (Official Music Video).m4a");

    println!("Cargando archivo en memoria...");
    let audio_data = load_audio_file(&path)?;
    println!("Muestras originales: {}", audio_data.samples.len());

    let config = AudioConfig::default();
    let mut engine = AudioEngine::new(config.clone())?;

    let sample_rate_f32 = config.sample_rate as f32;
    let mut eq = ThreeBandEQ::new(sample_rate_f32, 125.0, 4000.0);
    // Esta es la configuracion que me dio el mejor sonido en mi equipo hasta ahora.
    // Nota, mi equipo de sonido es bastante modesto, para ti puede ser una configuracion muy diferente.
    eq.set_low_gain(4.0);     
    eq.set_low_resonance(6.0);
    eq.set_low_shelf(4.0);
    eq.set_mid_gain(1.0);   
    eq.set_mid_presence(3.0);
    eq.set_mid_body(2.5);
    eq.set_high_gain(2.7);  
    eq.set_high_shelf(4.5);
    eq.set_high_air(6.0); 
    eq.set_master_gain(0.7);
    
    println!("Iniciando reproduccion...");

    // Reproducir
    engine.play(audio_data.samples, audio_data.sample_rate as f32, audio_data.channels, eq)?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    engine.stop();

    Ok(())
}
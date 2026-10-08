use std::path::Path;
use std::error::Error as StdError;

mod audio;

use crate::audio::{
    engine::AudioEngine,
    eq::ThreeBandEQ,
};

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
    let path = Path::new("/home/angelo/Música/Rock/BAD OMENS  Like A Villain (Official Music Video).m4a");

    let mut engine = AudioEngine::new()?;
    println!(
        "Dispositivo: {} Hz, {} canales",
        engine.sample_rate(),
        engine.channels()
    );

    let mut eq = ThreeBandEQ::new(engine.sample_rate() as f32, 180.0, 3500.0);
    // Esta es la configuracion que me dio el mejor sonido en mi equipo hasta ahora.
    // Nota, mi equipo de sonido es bastante modesto, para ti puede ser una configuracion muy diferente.
    eq.set_low_gain(1.9);     
    eq.set_low_resonance(6.0);
    eq.set_low_shelf(4.0);
    eq.set_mid_gain(1.0);   
    eq.set_mid_presence(2.0);
    eq.set_mid_body(1.0);
    eq.set_high_gain(2.8);  
    eq.set_high_shelf(4.0);
    eq.set_high_air(6.0); 
    eq.set_master_gain(0.72);
    
    println!("Iniciando reproduccion...");

    // Reproducir
    engine.play(path, eq)?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    engine.stop();

    Ok(())
}
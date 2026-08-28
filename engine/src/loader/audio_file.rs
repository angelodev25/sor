use std::io::Read;
use std::process::{Command, Stdio};
use std::path::Path;
use std::error::Error as StdError;

use crate::types::AudioData;

pub fn load_audio_file(path: &Path) -> Result<AudioData, Box<dyn StdError>> {
    // Decodifica a PCM f32le raw, 44100 Hz, estéreo, vía stdout
    let mut child = Command::new("ffmpeg")
        .args([
            "-i", path.to_str().unwrap(),
            "-f", "f32le",
            "-acodec", "pcm_f32le",
            "-ar", "44100",
            "-ac", "2",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let mut bytes = Vec::with_capacity(8 * 1024 * 1024);
    child.stdout.take().unwrap().read_to_end(&mut bytes)?;
    let status = child.wait()?;
    if !status.success() {
        return Err("ffmpeg falló".into());
    }

    // 4 bytes por muestra f32
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    // techo de seguridad
    let max_abs = samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    let samples = if max_abs > 1.0 {
        let g = 1.0 / max_abs;
        samples.iter().map(|s| s * g).collect()
    } else { samples };

    Ok(AudioData {
        samples,
        sample_rate: 44100,
        channels: 2,
    })
}
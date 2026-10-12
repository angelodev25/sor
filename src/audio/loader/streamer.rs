use std::io::{BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::path::{Path, PathBuf};
use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rtrb::{Producer, Consumer, RingBuffer};
use crate::audio::controls::Controls;

const RING_FRAMES: usize = 44_100 * 3;
const READ_CHUNK_BYTES: usize = 9216;

pub struct Streamer {
    stop: Arc<AtomicBool>,
    child: Option<Child>,
    handle:  Option<JoinHandle<()>>,
}

pub fn spawn_streamer(
    path: &Path, 
    sample_rate: u32, 
    channels: u16,
    controls: Arc<Controls>,
) -> Result<(Consumer<f32>, Streamer), Box<dyn StdError>> {
    // convertir a PatBuff porque se movera al hilo
    let path: PathBuf = path.to_path_buf();

    let capacity = RING_FRAMES * channels as usize;
    let (producer, consumer) = RingBuffer::<f32>::new(capacity);

    // Lanzar ffmpeg
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel", "error",
            "-nostdin",
            "-i", path.to_str().ok_or("Invalid path")?,
            "-f", "f32le", // salida en PCM float 32-bit little-endian crudo
            "-acodec", "pcm_f32le",
            "-ar", &sample_rate.to_string(), // remuestrea a sample_rate (swresample, alta calidad)
            "-ac", &channels.to_string(), // rematrixa a `channels`
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Obtener el stdout del proceso ffmpeg
    // el child se queda en el Streamer para poder matarlo cuando se detenga la reproduccion
    let stdout = child.stdout.take().ok_or("ffmpeg stdout error")?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    // Hilo lector que lee de ffmpeg y empuja al buffer
    let handle = thread::spawn(move || {
        reader_loop(stdout, producer, stop_thread);
        controls.song_loaded.store(false, Ordering::Relaxed);
        controls.song_ended.store(true, Ordering::Relaxed);
    });

    Ok((consumer,
        Streamer{
            stop,
            child: Some(child),
            handle: Some(handle),
        },
    ))
}

pub fn reader_loop(stdout: ChildStdout, mut producer: Producer<f32>, stop: Arc<AtomicBool>) {
    let mut reader = BufReader::with_capacity(64*1024, stdout);
    let mut chunk = vec![0u8; READ_CHUNK_BYTES];

    'outer: loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // Leer un bloque
        let n = match reader.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break, // Error de lectura
        };

        let samples = n/4; // 4 bytes por muestra
        for i in 0..samples {
            if stop.load(Ordering::Relaxed) {
                break 'outer;
            }

            let bytes = &chunk[i * 4..i * 4 + 4];
            let sample = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            // empujar con reintento, si el buffer esta lleno, esperar y reintentar
            loop {
                match producer.push(sample) {
                    Ok(()) => break,
                    Err(_) => {
                        if stop.load(Ordering::Relaxed) {
                            break 'outer;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        // Señalar al hilo que se detenga
        self.stop.store(true, Ordering::Relaxed);

        // Matar el proceso ffmpeg si sigue corriendo
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait(); // recolecta el zombie
        }

        // Uni el hilo. se bloquera hasta que salga del bucle
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
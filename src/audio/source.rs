use std::sync::{Mutex, MutexGuard};
use rtrb::Consumer;

// Enchufe de audio del stream persistente: el callback cpal lee siempre de
// aquí, y cambiar de pista es intercambiar el consumer sin tocar el stream.
pub struct AudioSource {
    inner: Mutex<Option<Consumer<f32>>>,
}

impl AudioSource {
    pub fn new() -> Self {
        Self { inner: Mutex::new(None) }
    }

    pub fn lock(&self) -> MutexGuard<'_, Option<Consumer<f32>>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

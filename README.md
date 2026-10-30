# Sor — música con calidad

Reproductor de música en Rust. Nace de la calidad de salida de Mixxx con buena
ecualización: el objetivo es esa pegada y claridad, pero en una app sencilla
para escuchar música, sin la complejidad de una cabina DJ.

## Estado actual (v0.2.0: biblioteca navegador + persistencia)

- Motor DSP propio: HPF + crossover LR4 de 3 bandas + EQ + compresores por
  banda + soft clipper tanh + limitador lookahead + volumen. Todo en el
  callback de `cpal`, sin alocar, con parámetros atómicos desde la UI.
- Stream `cpal` persistente: se abre una vez al arrancar; cambiar de pista
  solo intercambia la fuente de datos, sin reabrir el dispositivo.
- **Biblioteca estilo navegador** (vista nueva): se abre por defecto la carpeta
  Música/Música del sistema (o `~/Music`); la parte superior muestra las
  subcarpetas como pestañas con scroll horizontal; al hacer click en una pestaña
  se carga automáticamente el contenido de esa carpeta; una flecha izquierda
  permite volver a la carpeta padre. La carpeta visitada se persiste en
  `~/.config/sor/library.json` y se restaura al arrancar.
- Lista de pistas con carátula, título, artista y nombre de archivo; reproducir
  al pulsar una fila, con cambio de pista limpio (sin doble click ni estados
  cruzados).
- Metadatos en el Player vía `lofty`: título, artista, duración, progreso
  (barra + tiempos) y carátula (o icono por defecto).
- Decodificación universal vía `ffmpeg` en subprocess (MP3, AAC/M4A incluyendo
  HE-AACv2, FLAC, Opus, WAV, ALAC…).
- UI en Slint: reproductor + panel de EQ con 9 perillas, drive, volumen,
  clip y techo del limitador, con toggles + vista de biblioteca navegador.

## Arquitectura

```text
ffmpeg subprocess --f32le--> ringbuffer (rtrb, 3 s) --> callback cpal
  --> [EQ 3 bandas] --> [soft clipper] --> [limiter] --> [volumen] --> DAC

UI Slint --Atomics--> mismo callback (sin locks)
```

El `AudioEngine` vive en un hilo gestor propio (`cpal::Stream` no es `Send`)
y recibe órdenes por canal (`PlayTrack { path, track_id }`). El callback lee
siempre del `AudioSource` (`Mutex<Option<Consumer>>`); entre pistas hay
silencio sin cortar el stream. Un contador de generación invalida hilos de
progreso y lectores de pistas sustituidas; al cambiar de pista la cadena DSP
se reconstruye virgen (la cola del tema anterior sonaba como golpe grave-medio).

### Biblioteca (navegador de carpetas)

| Flujo | Qué pasa |
|-------|----------|
| Arranque | `get_music_dir()` devuelve `~/Music` (o `~/Música` en Linux) o fallback `$HOME`. Si no existe, devuelve `~/Music`. Se carga `last_folder` desde `~/.config/sor/library.json` si hay una guardada y válida. |
| Barra superior | Icono + "Biblioteca" (fijo, a la izquierda) | flecha `←` (solo si hay padre) | pestaña de carpeta actual (no clickeable) | pestañas de subcarpetas con scroll horizontal (wheel + drag). |
| Click en pestaña | `tab-changed(i)` → `folder_entries(path)` → UI actualiza carpeta, subcarpetas y pistas. Se guarda en `library.json`. |
| Flecha atrás | `go-back()` → sube al directorio padre y vuelve a cargar. |

| Archivo | Rol |
|---|---|
| `src/main.rs` | Arranque + estado compartido + presets iniciales + cableado de callbacks Slint (`navigate`, `go-back`, `tab-changed`, `open-folder`, `track-clicked`, `play-pause`) |
| `src/config.rs` | Persistencia de `LibraryConfig` (última carpeta visitada) en `~/.config/sor/library.json` |
| `src/playback.rs` | Puente UI↔motor: `spawn_engine`, `start_track`, hilo de progreso (250 ms), `folder_entries()` (carpeta + padre + subcarpetas + pistas), filas y metadatos |
| `src/audio/engine/engine.rs` | `AudioEngine`: abre dispositivo `cpal` (exige `f32` y estéreo), buffer ~25 ms redondeado a potencia de dos (mín. 512), stream persistente y cadena DSP |
| `src/audio/source.rs` | `AudioSource`: enchufe `Mutex<Option<Consumer>>` que el callback lee siempre |
| `src/audio/loader/streamer.rs` | `spawn_streamer`: lanza `ffmpeg -f f32le -acodec pcm_f32le -ar <fs> -ac <ch> pipe:1`, hilo lector a ringbuffer `rtrb`. Al EOF espera a drenar la cola antes de señalizar el fin. `Drop` mata el proceso y une el hilo |
| `src/audio/library.rs` | `get_music_dir()`, `list_subdirs()`, `scan_folder`: escaneo no recursivo, 11 extensiones de audio, ordenado por nombre de archivo |
| `src/audio/metadata.rs` | `read_metadata` con `lofty` (título/artista/duración/caratula) + `fmt_mmss` |
| `src/audio/eq/three_band.rs` | `ThreeBandEQ`: HPF + crossover LR4 + EQ + compresores por banda |
| `src/audio/eq/compressor.rs` | `Compressor` estéreo-link (detector = máx. \|L\|,\|R\|), envolvente attack/release exponencial |
| `src/audio/eq/soft_clip.rs` | `SoftClipper` tanh, lineal bajo el umbral, asíntota a `ceiling = 0.98` |
| `src/audio/engine/limiter.rs` | `Limiter` brickwall con lookahead (línea de retardo + envolvente asimétrica, ganancia común a L/R) |
| `src/audio/controls.rs` | `Controls` (paused/song_loaded/song_ended/quit + `current_track`/`dsp_reset`/progreso) + `EqParams` + `LimiterParams` + `OutputParams`, todo atómico |
| `src/ui/app_window.slint` | `AppWindow`: layout `Navbar + (Player | EqPanel | Library)`, propiedades y callbacks |
| `src/ui/player.slint`, `eq_panel.slint`, `knob.slint`, `navbar.slint` | Player (título/artista/progreso/caratula) con botones Anterior/Siguiente (`previous-track`, `next-track`) además de `play-pause`; panel de EQ, knob, nav. |
| `src/ui/library.slint`, `library_row.slint` | Biblioteca navegador (barra + pestañas + lista) y fila (carátula/título/artista/archivo). Props: `scroll-y`, `scroll-target-y` (bind desde AppWindow para preservar posición al cambiar vistas). Callbacks: `navigate`, `go-back`, `tab-changed`. |
| `build.rs` | Compila `app_window.slint` con `slint-build` + librería de iconos `lucide` |

## Cadena DSP (valores reales del código)

Orden por muestra estéreo, en `engine.rs` → `three_band.rs`:

1. **HPF subsónico LR4 a 20 Hz** (`three_band.rs`). Limpia DC y rumble.
2. **Crossover LR4 en 180 Hz y 3500 Hz** (`engine.rs`, `three_band.rs`).
   LOW = LP4, MID = HP4+LP4, HIGH = HP4. 24 dB/oct, fase coherente.
3. **Procesado por banda** (`three_band.rs`):
   - LOW: compresor (umbral 0 dB, 2:1, 10/100 ms, makeup 1.06) → peaking
     resonancia 65 Hz Q 1.15 → low-shelf Baxandall 180 Hz Q 0.6 → ganancia.
   - MID: peaking cuerpo 300 Hz Q 1.2 → peaking presencia 2200 Hz Q 0.8 →
     compresor (umbral −6 dB, 2:1, 5/80 ms) → ganancia.
   - HIGH: compresor (umbral +3 dB, 2:1, 3/60 ms) → high-shelf 5 kHz Q 1.0 →
     peaking aire 9000 Hz Q 1.0 → ganancia.
4. **Mezcla + `master_gain`** (lineal, perilla "Drive" 0–1.5).
5. **Soft clipper tanh** (`soft_clip.rs`): bajo el umbral pasa intacto; por
   encima comprime hacia `ceiling = 0.98`. Umbral configurable en dB desde la
   UI (perilla "Clip", por defecto −1.2 dB en `main.rs`).
6. **Limitador lookahead** (`limiter.rs`, `engine.rs`): 7 ms lookahead,
   2 ms attack, 80 ms release. Techo configurable en dB desde la UI
   (perilla "Techo", por defecto 1.5 dB en `main.rs`).
7. **Volumen final** (lineal 0–1, por defecto 0.75).

Los biquads solo se reconstruyen cuando su valor cambia (comparación contra
caché en el callback, `engine.rs`). Los toggles de EQ / soft-clip /
limiter hacen bypass de cada etapa.

## UI (Slint)

- `Player`: título, artista, carátula (o icono `HeadphonesIcon` por defecto),
  play/pause, barra de progreso + tiempos, estados `is-loading` / `is-playing`.
  Nota: `MusicIcon` de lucide no se rasteriza con el backend FemtoVG+MESA
  (solo se ve en la preview de Slint); por eso el placeholder usa
  `HeadphonesIcon`.
- `EqPanel`: 9 knobs (graves: ganancia/shelf/resonancia; medios:
  ganancia/cuerpo/presencia; agudos: ganancia/shelf/aire) + `Drive`
  (master_gain) + slider de volumen + perillas `Clip` (−15…+2 dB) y `Techo`
  (−6…+3 dB) con sus toggles ON/OFF.
- `Library`: barra superior con icono + "Biblioteca", flecha `←` (volver al
  padre), pestaña de carpeta actual (no clickeable) y pestañas de subcarpetas
  con scroll horizontal. Lista de pistas (carátula, título, artista, archivo);
  click en fila reproduce al instante. La carpeta se persiste y restaura al
  arrancar. **La posición de scroll se preserva al cambiar entre vistas**
  Player <-> Biblioteca, y se resetea al cambiar de carpeta o pestaña.
- `Player`: además de play/pause, tiene botones **Anterior** (`SkipBackIcon`)
  y **Siguiente** (`SkipForwardIcon`) con bucle circular (último → primero y
  primero → último).
- Callbacks hacia Rust: `play-pause`, `eq-changed`, `limiter-changed`,
  `output-changed`, `open-folder`, `navigate`, `go-back`, `tab-changed`,
  `track-clicked`, `previous-track`, `next-track` (`app_window.slint`,
  `main.rs`).

## Preset inicial

En `main.rs`. Es el ajuste que mejor suena en el equipo del autor
(modesto — en otro equipo puede pedir valores distintos):

```rust
low_gain 1.9, low_resonance 6.0, low_shelf 4.0,
mid_gain 1.0, mid_presence 2.0, mid_body 1.0,
high_gain 2.8, high_shelf 4.0, high_air 6.0,
master_gain 0.72, volumen 0.75, clip -1.2 dB, techo +1.5 dB
```

## Requisitos

- Toolchain Rust estable.
- `ffmpeg` en el `PATH`.
- Audio compatible con `cpal` (ALSA en Linux), salida estéreo en `f32`.
- Dependencias Rust (`Cargo.toml`): `cpal`, `biquad`, `rtrb`, `slint`,
  `image` (png/jpeg), `id3`, `lofty`, `lucide-slint`, `dirs`, `serde`,
  `serde_json`.

### Debian/Ubuntu

```sh
sudo apt install ffmpeg libasound2-dev pkg-config make
```

### Arch

```sh
sudo pacman -S ffmpeg alsa-lib pkgconf base-devel
```

## Uso

```sh
git clone https://github.com/angelodev25/Sor.git
cd Sor
cargo run
```

Al arrancar se carga la última carpeta visitada (o `~/Music` si no hay). La
barra de la Biblioteca muestra las subcarpetas como pestañas; hacer click en
una carga sus pistas automáticamente. Click en una fila para reproducir; el
botón play reanuda o arranca la pista seleccionada (o la primera de la lista).

## Roadmap realista (corto plazo)

1. ~~Selector de archivos en la UI~~ (hecho: navegador de carpetas con pestañas + play por fila).
2. ~~Mostrar título / artista / carátula / progreso~~ (hecho vía `lofty` + contador de frames).
3. ~~Persistencia de última carpeta~~ (hecho: `~/.config/sor/library.json`).
4. Presets de EQ guardables (el actual + flat + alguno más).
5. Seek en la barra de progreso (hoy solo muestra; el hilo espera a
   `song_ended` o `quit`).
6. Pulir `Navbar` (hoy sin efecto) o quitarla si no aporta.
7. Escaneo recursivo (hoy solo una carpeta por nivel, no recursivo).

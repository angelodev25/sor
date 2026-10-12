# Sor — música con calidad

Reproductor de música en Rust. Nace de la calidad de salida de Mixxx con buena
ecualización: el objetivo es esa pegada y claridad, pero en una app sencilla
para escuchar música, sin la complejidad de una cabina DJ.

## Estado actual (v0.2: motor + UI funcional)

- Motor DSP propio: HPF + crossover LR4 de 3 bandas + EQ + compresores por
  banda + soft clipper tanh + limitador lookahead + volumen. Todo en el
  callback de `cpal`, sin alocar, con parámetros atómicos desde la UI.
- Decodificación universal vía `ffmpeg` en subprocess (MP3, AAC/M4A incluyendo
  HE-AACv2, FLAC, Opus, WAV, ALAC…).
- UI en Slint: reproductor + panel de EQ con 9 perillas, drive, volumen,
  clip y techo del limitador, con toggles.
- Limitación actual: la pista sigue hardcodeada en `src/main.rs:138`.
  El botón play/pause de la UI funciona, pero aún no hay selector de
  archivos. Es la próxima feature.

## Arquitectura

```text
ffmpeg subprocess --f32le--> ringbuffer (rtrb, 3 s) --> callback cpal
  --> [EQ 3 bandas] --> [soft clipper] --> [limiter] --> [volumen] --> DAC

UI Slint --Atomics--> mismo callback (sin locks)
```

### Archivos

| Archivo | Rol |
|---|---|
| `src/main.rs` | Crea UI + estado compartido + presets iniciales, callback `play-pause` |
| `src/audio/engine/engine.rs` | `AudioEngine`: abre dispositivo `cpal` (exige `f32` y estéreo), buffer ~25 ms redondeado a potencia de dos (mín. 512), construye el stream y la cadena DSP |
| `src/audio/loader/streamer.rs` | `spawn_streamer`: lanza `ffmpeg -f f32le -acodec pcm_f32le -ar <fs> -ac <ch> pipe:1`, hilo lector a ringbuffer `rtrb`. `Drop` mata el proceso y une el hilo |
| `src/audio/eq/three_band.rs` | `ThreeBandEQ`: HPF + crossover LR4 + EQ + compresores por banda |
| `src/audio/eq/compressor.rs` | `Compressor` estéreo-link (detector = máx. \|L\|,\|R\|), envolvente attack/release exponencial |
| `src/audio/eq/soft_clip.rs` | `SoftClipper` tanh, lineal bajo el umbral, asíntota a `ceiling = 0.98` |
| `src/audio/engine/limiter.rs` | `Limiter` brickwall con lookahead (línea de retardo + envolvente asimétrica, ganancia común a L/R) |
| `src/audio/controls.rs` | `Controls` (paused/song_loaded/song_ended/quit) + `EqParams` + `LimiterParams` + `OutputParams`, todo atómico |
| `src/ui/app_window.slint` | `AppWindow`: layout `Navbar + (Player | EqPanel)`, propiedades y callbacks |
| `src/ui/player.slint`, `eq_panel.slint`, `knob.slint`, `navbar.slint` | Player, panel de EQ (knobs + sliders + toggles), knob reutilizable, nav |
| `build.rs` | Compila `app_window.slint` con `slint-build` + librería de iconos `lucide` |

## Cadena DSP (valores reales del código)

Orden por muestra estéreo, en `engine.rs` → `three_band.rs`:

1. **HPF subsónico LR4 a 20 Hz** (`three_band.rs:111-112`). Limpia DC y rumble.
2. **Crossover LR4 en 180 Hz y 3500 Hz** (`main.rs:150`, `three_band.rs:115-122`).
   LOW = LP4, MID = HP4+LP4, HIGH = HP4. 24 dB/oct, fase coherente.
3. **Procesado por banda** (`three_band.rs:170-194`):
   - LOW: compresor (umbral 0 dB, 2:1, 10/100 ms, makeup 1.06) → peaking
     resonancia 65 Hz Q 1.15 → low-shelf Baxandall 180 Hz Q 0.6 → ganancia.
   - MID: peaking cuerpo 300 Hz Q 1.2 → peaking presencia 2200 Hz Q 0.8 →
     compresor (umbral −6 dB, 2:1, 5/80 ms) → ganancia.
   - HIGH: compresor (umbral +3 dB, 2:1, 3/60 ms) → high-shelf 5 kHz Q 1.0 →
     peaking aire 9000 Hz Q 1.0 → ganancia.
4. **Mezcla + `master_gain`** (lineal, perilla "Drive" 0–1.5).
5. **Soft clipper tanh** (`soft_clip.rs`): bajo el umbral pasa intacto; por
   encima comprime hacia `ceiling = 0.98`. Umbral configurable en dB desde la
   UI (perilla "Clip", por defecto −1.2 dB en `main.rs:33`).
6. **Limitador lookahead** (`limiter.rs`, `engine.rs:90-96`): 7 ms lookahead,
   2 ms attack, 80 ms release. Techo configurable en dB desde la UI
   (perilla "Techo", por defecto 1.5 dB en `main.rs:64`).
7. **Volumen final** (lineal 0–1, por defecto 0.75).

Los biquads solo se reconstruyen cuando su valor cambia (comparación contra
caché en el callback, `engine.rs:149-165`). Los toggles de EQ / soft-clip /
limiter hacen bypass de cada etapa.

## UI (Slint)

- `Player`: título, play/pause, estado `is-loading` / `is-playing`.
- `EqPanel`: 9 knobs (graves: ganancia/shelf/resonancia; medios:
  ganancia/cuerpo/presencia; agudos: ganancia/shelf/aire) + `Drive`
  (master_gain) + slider de volumen + perillas `Clip` (−15…+2 dB) y `Techo`
  (−6…+3 dB) con sus toggles ON/OFF.
- Callbacks hacia Rust: `play-pause`, `eq-changed`, `limiter-changed`,
  `output-changed` (`app_window.slint:41-44`, `main.rs:71-111`).

## Preset inicial

En `main.rs:39-48`. Es el ajuste que mejor suena en el equipo del autor
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
- Audio compatible con `cpal` (ALSA en Linux).
- Dependencias Rust (`Cargo.toml`): `cpal`, `biquad`, `rtrb`, `slint`,
  `image` (png/jpeg), `id3`, `lofty`, `lucide-slint`.
  `id3` / `lofty` / `image` ya están declaradas para metadatos y carátulas,
  aún sin cablear en la UI.

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

> Nota: el `Makefile` (`make run` con `SLINT_BACKEND=winit-software`) está
> roto — ese backend da panic. Usa `cargo run` directamente.

Limitación actual: la pista está hardcodeada (`src/main.rs:138`):

```rust
let path = Path::new("BAD OMENS  Like A Villain (Official Music Video).m4a");
```

Para probar otra pista, edita esa línea con la ruta a tu archivo.

## Roadmap realista (corto plazo)

1. Selector de archivos en la UI (eliminar el path hardcodeado).
2. Mostrar título / artista / carátula desde tags (`id3`/`lofty` + `image`
   ya en `Cargo.toml`).
3. Presets de EQ guardables (el actual + flat + alguno más).
4. Barra de progreso + seek (hoy solo play/pausa; el hilo espera a
   `song_ended` o `quit`).
5. Pulir `Navbar` (hoy sin efecto) o quitarla si no aporta.

## Objetivos del proyecto (visión)

1. Motor de audio con sonido decente. [listo]
2. Interfaz sencilla para cargar canciones, reproducir y pausar. [a medias:
   UI lista, falta selector de archivos]
3. Hacer la interfaz bonita cuando sea funcional. [en curso]
4. Mejoras iterativas del motor (calidad de salida con el tiempo).
5. Features de reproductor moderno (las del roadmap corto primero).

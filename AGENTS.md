# AGENTS.md — Sor

Rust music player: custom DSP engine (`cpal` callback) + `ffmpeg` subprocess decoding + Slint UI. See `README.md` for DSP chain values and roadmap.

## Commands

- Run: `cargo run` — ONLY way. `make run` panics, do not use.
- Backend: `winit-software` (renderer software) is set **in code** in `main.rs` via `slint::BackendSelector`. Override with `SLINT_BACKEND=...` in the environment. Do not use FemtoVG/OpenGL on old GPUs: i915 (G33) runs the GLSL shaders on the CPU.
- Check: `cargo check` / `cargo clippy`; no tests exist, no CI. Verify with `cargo check` + `cargo run`.
- Deps (Debian/Ubuntu): `sudo apt install ffmpeg libasound2-dev pkg-config make`. Runtime requires `ffmpeg` in `PATH` and a stereo `f32` ALSA device (engine errors out otherwise).
- Linker is `mold` via `.cargo/config.toml`; if link fails, `sudo apt install mold` or override `RUSTFLAGS`.

## Architecture

- `src/main.rs` — entrypoint only: shared state, initial EQ preset synced to UI, Slint callbacks (`eq-changed`, `limiter-changed`, `output-changed`, `open-folder`, `navigate`, `go-back`, `tab-changed`, `track-clicked`, `play-pause`). Initializes library with last persisted folder or `~/Music` fallback.
- `src/config.rs` — `LibraryConfig`: persists `last_folder` to `~/.config/sor/library.json` via `serde_json`. `load()`, `save()`, `get_last_folder()`, `set_last_folder()`.
- `src/playback.rs` — UI↔engine bridge. `spawn_engine` creates the manager thread + persistent stream; `start_track` sends `PlayTrack{path,track_id}` over a channel and spawns the 250 ms progress thread; `track_entries`/`show_track_info` map disk → Slint rows and metadata → Player. **New:** `FolderView` struct + `folder_entries(dir)` returns current path, parent path, subdirs, and tracks for a directory.
- `src/audio/library.rs` — `get_music_dir()` returns `~/Music`/`~/Música` (Linux) with fallback; `list_subdirs(dir)` returns immediate subdirectories sorted by name; `scan_folder`: NON-recursive scan, 11 extensions (mp3 m4a flac ogg oga opus wav aac wma aiff aif), sorted by `file_name`.
- `src/audio/engine/engine.rs` — `AudioEngine` lives on its own manager thread (`cpal::Stream` is not `Send`). `start()` opens the stream ONCE per session; `play_track()` only swaps the data source. Requires stereo (`channels == 2` errors otherwise), ~25 ms buffer rounded to pow2 (min 512).
- `src/audio/source.rs` — `AudioSource`: `Mutex<Option<Consumer<f32>>>` socket the callback always reads; `None` between tracks = silence, stream never restarts.
- `src/audio/loader/streamer.rs` — `spawn_streamer`: `ffmpeg -f f32le pcm_f32le -ar <fs> -ac <ch> pipe:1` → reader thread → `rtrb` ringbuffer (3 s). At EOF the reader waits for the callback to drain the ring before signalling end. `Drop` kills ffmpeg + joins the thread.
- `src/audio/metadata.rs` — `read_metadata` via `lofty` (title/artist/duration/first picture); fallbacks "Pista desconocida"/"Artista desconocido". Progress = `frames_played` counted in the callback vs `duration_secs`.
- `src/audio/controls.rs` — all UI→audio state is atomics (`Controls`, `EqParams`, `LimiterParams`, `OutputParams`). Never add locks in the `cpal` callback; never allocate there. `current_track: AtomicU64` + `generation` counter invalidate stale readers/progress threads; `dsp_reset` rebuilds the DSP chain virgin on track change (old reverb tail sounded like a low-mid thump otherwise).
- `src/audio/eq/three_band.rs`, `compressor.rs`, `soft_clip.rs`, `engine/limiter.rs` — DSP: HPF 20 Hz → LR4 crossover (180/3500 Hz) → per-band comp+EQ → master gain → tanh soft-clip (ceiling 0.98) → lookahead limiter → volume. Biquads rebuild only on value change (cache compare in `engine.rs`) — keep that pattern.
- `src/ui/*.slint` — `app_window.slint` is root (`Navbar + (Player | EqPanel | Library)`), compiled by `build.rs` with `slint-build` + `lucide` icon library.
  - `library.slint` — Biblioteca navegador: barra superior con icono + "Biblioteca", flecha `←` (volver al padre), pestaña de carpeta actual (no clickeable), pestañas de subcarpetas con scroll horizontal (wheel + drag), lista de pistas con scroll custom (Timer limitado a `scroll-fps`). Props: `scroll-y`, `scroll-target-y` (bind bidireccional desde AppWindow para preservar posición al cambiar vistas Player <-> Biblioteca), `library-reset-scroll` (activa el reset de scroll solo al cambiar de carpeta/pestaña, no al cambiar de vista). Callbacks: `navigate(string)`, `go-back()`, `tab-changed(int)`.
  - `player.slint` — Player con botones `previous-track()` y `next-track()` además de `play-pause()`. Bucle circular en los bordes de la lista.
  - `models.slint` — `TrackEntry` + `SubdirEntry { name, path }`.
  - `app_window.slint` — binds new library/player props/callbacks to `LibraryView` and `Player`. Props: `library-scroll-y`, `library-scroll-target-y`. Callbacks: `previous-track`, `next-track`.

## Gotchas

- Track switching is fully serialized in the manager thread — no mutex needed; don't reintroduce one (commit `98c4f1c`'s mutex is gone since `b17d3a4`).
- `is_loading` / `song_loaded` / `song_ended` guards in `main.rs`/`playback.rs` prevent double-start and stale-end signalling; natural end rewinds to 00:00, pauses, and marks unloaded so next play restarts from zero.
- Slint/Lucide rendering: `MusicIcon` never rasterizes under FemtoVG+MESA (only in Slint preview) — placeholder uses `HeadphonesIcon` (`5344ec0`). Lucide icons ignore `vertical-alignment`; centering uses the `VerticalLayout` wrapper trick (see `library.slint` tabs — icons are wrapped in `VerticalLayout` to center them since `vertical-alignment` does nothing on Lucide icons under the software renderer).
- `cover_to_image` downscales covers >512 px (`playback.rs`, marked `shortcut:` — add disk cache if this gets slow).
- `profile.dev` sets `opt-level = 1` (top) / `3` (deps) — builds are slow; prefer `cargo check` for iteration.
- No test harness, no lint config, no CI. Do not add one unasked.
- Library config persists to `~/.config/sor/library.json` — if a stale/invalid path is stored, `get_last_folder()` returns `None` and falls back to `~/Music`.

### Renderer software (Slint 1.17)

- No soporta `border-radius` + `clip: true` (se ve cuadrado) y los `@linear-gradient` **ignoran** `border-radius`. Por eso las carátulas traen el redondeo horneado en el alpha (`round_corners` en `playback.rs`) y el hueco sin portada se pinta en Rust (`placeholder_cover`).
- `cache-rendering-hint` no existe en el renderer software: inerte, no lo pongas.
- Los elementos **repetidos** (`for`) no reciben geometría por defecto: hay que darles `width`/`height`/`y` explícitos si su padre no es un layout. Por eso las filas de la biblioteca los posiciona a mano (`y: idx * 68px`).
- `id` no existe en los builtins (Rectangle, Flickable, Timer), solo en componentes propios.
- Un `TouchArea` **siempre** se queda con el ratón al pulsar (`GrabMouse`), tenga o no callback `pointer-event`: no sirve de overlay transparente. Y su `pointer-event` recibe `move` también al pasar el ratón por encima: para arrastrar hay que comprobar `self.pressed`.
- El `Flickable`/`ListView` de std-widgets anima la rueda 180 ms a 60 Hz (≈11 frames por muesca) y repinta el viewport entero: en un core lento satura la CPU. La biblioteca usa su propio motor (`scroll-fps` / `scroll-step-px` en `library.slint`).

### Rendimiento

- `perf` con eventos hardware no funciona en el equipo del usuario; usar `perf record -e cpu-clock` (evento software) y `perf report --sort=dso`. Recetas en `debug-profiling.txt` (build con símbolos: `--config 'profile.release.debug=1' --config 'profile.release.strip="none"'`).

## Skills (`.agents/skills/*/SKILL.md`, locked in `skills-lock.json`)

Load via the `skill` tool when the task matches; don't guess workflows.

- Routine default: `using-agent-skills` (pick the right skill), `test-driven-development` (any logic/bugfix), `debugging-and-error-recovery` (failures), `code-review-and-quality` (before merge).
- Plan/shape work: `spec-driven-development` (new feature, e.g. seek/presets), `planning-and-task-breakdown`, `incremental-implementation`, `idea-refine` / `interview-me` (vague asks).
- Code quality: `code-simplification` (refactor), `performance-optimization` (DSP/callback hot path — profile, keep no-alloc), `api-and-interface-design` (module boundaries, e.g. library/metadata APIs).
- UI: `frontend-ui-engineering` (any `src/ui/*.slint` work), `browser-testing-with-devtools` does NOT apply (no browser here).
- Ops/care: `security-and-hardening` (file paths, subprocess args, untrusted tags), `observability-and-instrumentation`, `shipping-and-launch`, `ci-cd-and-automation`, `git-workflow-and-versioning`, `deprecation-and-migration`, `documentation-and-adrs`, `source-driven-development` (verify Slint/cpal API in docs before coding), `constraint-driven-development`, `context-engineering`, `doubt-driven-development`.

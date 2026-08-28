# Sor - música con calidad.

Reproductor de música con motor de sonido construido en Rust buscando alcanzar (y si es posible superar) la calidad de sonido de Mixxx.

Este reproductor nace de experimentar la calidad de audio de salida que puede ofrecer Mixxx con una buena ecualización. El objetivo es construir una app
para reproducir tu música cómodamente, en una interfaz sencilla sin las tantas cosas que ofrece Mixxx (que es para DJs), pero que mantenga su calidad de audio.

## Objetivos del proyecto

1. Crear la primera version del motor de audio con un sonido decente. [listo]
2. Crear una interfaz sencilla para cargar las canciones, reproducir y pausar.
3. Hacer la interfaz bonita luego de que sea funcional (obviamente).
4. Desarrollar mejoras en el motor de sonido para mejorar la calidad de salida a medida pasa el tiempo.
5. Implementar features de un reproductor de verdad, moderno y de calidad.
---

## Estado actual

## v0.1 del motor de audio

#### Decodificación

- Carga y decodificación de cualquier formato soportado por FFmpeg (MP3, AAC/M4A, FLAC, Opus, WAV, ALAC, …) vía subprocess a PCM f32 little-endian a 44100 Hz estéreo.
- Requiere ffmpeg instalado en el sistema (sudo apt install ffmpeg).
- Esta capa resolvió problemas críticos de decodificación HE-AAC/HE-AACv2 que librerías puras de Rust no cubren (SBR/PS no implementado correctamente en Symphonia).

#### Procesamiento de muestras

El pipeline de audio por canal se aplica en serie para cada muestra:

1. HPF subsonic LR4 a 30 Hz — elimina DC offset y rumble sub-audible que ensucia los graves.
2. Crossover LR4 de 3 bandas (cortes a 125 Hz y 4000 Hz) — divide el espectro en LOW / MID / HIGH con aislamiento de 24 dB/octava. Fase coherente. Suma plana cuando todas las perillas están a 0 dB.

3. Procesamiento por banda aislada:

- LOW: PeakEQ "resonancia" a 65 Hz + LowShelf Baxandall a 180 Hz + compresor de graves dedicado.
- MID: PeakEQ "body" a 300 Hz + PeakEQ "presence" a 2200 Hz + compresor suave de voces.
- HIGH: HighShelf amplio a 2000 Hz (Q=1.0) + compresor de transientes de agudos.
4. Mezcla final con master_gain configurable.
5. Limitador brickwall de seguridad al final del pipeline.

### Componentes principales

Componente | Rol
------------|-----
audio_file.rs | Decodificación universal vía FFmpeg subprocess
engine.rs | Configuración de cpal + stream de salida
three_band.rs | EQ DJ con crossover LR4 + procesamiento por banda
compressor.rs | Compresor dinámico configurable por banda
limiter.rs | Limitador brickwall de seguridad

## Requisitos del sistema

- Rust toolchain (stable).
- ffmpeg instalado y accesible en PATH.
- Sistema de audio compatible con cpal (ALSA en Linux).

### Debian/Ubuntu
```
sudo apt install ffmpeg libasound2-dev pkg-config make
```

### Arch
```
pacman -S ffmpeg alsa-lib pkgconf base-devel
```
*'base devel' es para instalar la herramienta que permite ejecutar Makefile*

## Uso

```
git clone https://github.com/angelodev25/Sor.git
cd Sor
make run
```
(Solo ejecuta el motor de audio).

Por defecto reproduce un archivo hardcoded en main.rs. Para cambiar la pista, edita main.rs:
```
let path = Path::new("tu_archivo.m4a");
```

## Configuracion del EQ

Todas las perillas se ajustan en main.rs antes de llamar a engine.play(...):

```
let mut eq = ThreeBandEQ::new(sample_rate_f32, 125.0, 4000.0);

// Graves
eq.set_low_gain(2.5);       // Perilla DJ: +2.5 dB
eq.set_low_resonance(5.0);  // +5 dB a 65 Hz
eq.set_low_shelf(3.8);      // +3.8 dB Baxandall a 180 Hz

// Medios
eq.set_mid_gain(0.0);
eq.set_mid_presence(3.0);   // Presencia vocal
eq.set_mid_body(2.5);       // Cuerpo vocal a 300 Hz

// Agudos
eq.set_high_gain(2.5);
eq.set_high_shelf(5.0);     // Shelf amplio a 2-6 kHz
eq.set_high_air(6.0); // refuerza agudos +9000Hz

eq.set_master_gain(0.75);
```

## Tecnologías usadas

### Motor de audio (Rust):

```
[dependencies]
cpal = "0.15"
rubato = "0.15"
biquad = "0.6"
audioadapter = "0.5"
audioadapter-buffers = "0.5"
```

### Próximos pasos:

- Interfaz gráfica para cargar archivos
- Control de volumen maestro en tiempo real
- Perillas del EQ expuestas en la UI (low/mid/high)
- Soporte para playlists
- Soporte para metadatos (tags ID3, AAC)
- Normalización de loudness (LUFS) para que todas las pistas suenen al mismo volumen
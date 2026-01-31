# Capture

## Overview

Screenshots, video recording, and audio capture for content creation and documentation.

## Screenshots

### CLI

```bash
emu198x-cli -s c64 screenshot output.png
emu198x-cli -s c64 screenshot --format bmp output.bmp
emu198x-cli -s c64 screenshot --scale 2 output.png
```

### Formats

| Format | Extension | Notes |
|--------|-----------|-------|
| PNG | .png | Recommended, lossless |
| BMP | .bmp | Uncompressed |
| JPEG | .jpg | Lossy, not recommended |
| WebP | .webp | Good for web |

### Options

| Option | Description |
|--------|-------------|
| `--format` | Output format |
| `--scale` | Integer scale factor |
| `--aspect` | Apply correct aspect ratio |
| `--border` | Include border area |
| `--palette` | Use specific palette |

### Programmatic

```rust
let screenshot = emulator.screenshot();
// Returns ImageData { width, height, pixels: Vec<u8> (RGBA) }

let png_bytes = emulator.screenshot_png()?;
// Returns encoded PNG

let png_bytes = emulator.screenshot_png_scaled(2)?;
// Scaled 2×
```

## Video Recording

### CLI

```bash
# Start recording
emu198x-cli -s c64 record start output.mp4

# Stop recording
emu198x-cli -s c64 record stop

# Record specific duration
emu198x-cli -s c64 \
  record start output.mp4 \
  run --frames 600 \
  record stop
```

### Formats

| Format | Codec | Notes |
|--------|-------|-------|
| MP4 | H.264 | Wide compatibility |
| WebM | VP9 | Web-optimised |
| GIF | GIF | Simple animations |
| AVI | Uncompressed | Editing |

### Options

| Option | Description |
|--------|-------------|
| `--format` | Container format |
| `--codec` | Video codec |
| `--fps` | Frame rate (default: native) |
| `--scale` | Integer scale |
| `--audio` | Include audio |
| `--bitrate` | Video bitrate |

### GIF Creation

For short loops:

```bash
emu198x-cli -s c64 -H \
  load demo.prg \
  run --frames 60 \
  record start loop.gif --fps 10 \
  run --frames 100 \
  record stop
```

### Programmatic

```rust
emulator.start_recording(RecordingOptions {
    path: "output.mp4".into(),
    video: true,
    audio: true,
    fps: None,  // Use native frame rate
    scale: 2,
})?;

// Run emulator...

emulator.stop_recording()?;
```

## Audio Capture

### CLI

```bash
# Standalone audio
emu198x-cli -s c64 audio-capture output.wav

# With video
emu198x-cli -s c64 record start output.mp4 --audio
```

### Formats

| Format | Notes |
|--------|-------|
| WAV | Uncompressed PCM |
| FLAC | Lossless compressed |
| OGG | Lossy, smaller |
| MP3 | Lossy, compatible |

### Options

| Option | Description |
|--------|-------------|
| `--format` | Audio format |
| `--sample-rate` | Sample rate (default: 48000) |
| `--channels` | Mono/stereo |

### Programmatic

```rust
emulator.start_audio_capture("output.wav")?;
// ...
emulator.stop_audio_capture()?;

// Or get buffer directly
let samples = emulator.audio_buffer();
// Returns &[f32] - interleaved stereo samples
```

## Frame Grabbing

For custom processing:

```rust
// Register callback for each frame
emulator.on_frame(|frame| {
    // frame: &FrameData { pixels, width, height }
    process_frame(frame);
});

// Or grab single frame
let frame = emulator.grab_frame();
```

## Aspect Ratio Correction

Native resolutions don't match display aspect ratios:

| System | Native | Display | Correction |
|--------|--------|---------|------------|
| Spectrum | 256×192 | 4:3 | 1.25× width |
| C64 | 320×200 | 4:3 | 1.2× height |
| NES | 256×240 | 4:3 | 1.14× width |
| Amiga | 320×256 | 4:3 | 1.04× width |

### With Aspect Correction

```bash
emu198x-cli -s c64 screenshot --aspect output.png
# Outputs 384×288 (320×1.2 height, maintains aspect)
```

### Without (Raw Pixels)

```bash
emu198x-cli -s c64 screenshot output.png
# Outputs 320×200 (native resolution)
```

## Palettes

Different systems and regions have different colour palettes.

### Built-in Palettes

```bash
# C64
emu198x-cli -s c64 --palette vice screenshot out.png
emu198x-cli -s c64 --palette pepto screenshot out.png
emu198x-cli -s c64 --palette colodore screenshot out.png

# Spectrum
emu198x-cli -s spectrum --palette bright screenshot out.png
emu198x-cli -s spectrum --palette muted screenshot out.png

# NES
emu198x-cli -s nes --palette fceux screenshot out.png
emu198x-cli -s nes --palette nesticle screenshot out.png
```

### Custom Palette

```bash
emu198x-cli -s c64 --palette-file my-palette.pal screenshot out.png
```

Palette file format (one RGB triplet per line):

```
0 0 0
255 255 255
104 55 43
...
```

## Border Capture

Some systems have significant border areas used by software.

### Include Border

```bash
# Full overscan
emu198x-cli -s c64 screenshot --border full output.png

# Visible border only
emu198x-cli -s c64 screenshot --border visible output.png

# No border (screen area only)
emu198x-cli -s c64 screenshot --border none output.png
```

### Sizes with Border

| System | No Border | Visible Border | Full Overscan |
|--------|-----------|----------------|---------------|
| Spectrum | 256×192 | 320×240 | 352×288 |
| C64 | 320×200 | 384×272 | 400×284 |
| NES | 256×224 | 256×240 | 280×262 |

## Batch Capture

### Screenshot Series

```bash
emu198x-cli -s c64 -H \
  load game.d64 \
  type "LOAD \"*\",8,1\n" \
  wait 300 \
  type "RUN\n" \
  screenshot frame-001.png \
  wait 60 \
  screenshot frame-002.png \
  wait 60 \
  screenshot frame-003.png
```

### Automated Frame Dumps

```bash
# Dump every frame
emu198x-cli -s c64 -H \
  --frame-dump frames/ \
  load demo.prg \
  run --frames 600
# Creates frames/000001.png, frames/000002.png, ...
```

Then create video with ffmpeg:

```bash
ffmpeg -framerate 50 -i frames/%06d.png -c:v libx264 -pix_fmt yuv420p output.mp4
```

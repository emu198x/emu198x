# Capturing golden frames from FS-UAE

Pixel-exact reference images for cross-validating our emulator. Goldens
live in `crates/<machine>/tests/golden/` and are loaded by the
`*_golden_frames.rs` integration tests.

## Required output format

**768 × 576 PNG, ARGB or RGBA, no scaling, no scanlines, no filters.**

Matches our runtime's screenshot path: PAL `Standard` viewport, halved
superhires width × doubled deinterlaced height. Square-pixel 4:3.

If FS-UAE's output is any other size, the comparison helper hard-fails
with the dimension mismatch — there is no implicit scaling on either
side.

## FS-UAE configuration

Set the following in `~/Documents/FS-UAE/Configurations/<config>.fs-uae`
(or via the launcher GUI):

```
video_resolution = 1x
video_format = rgba
video_aspect_ratio = auto
zoom = full
filter = nearest
scanlines = 0
shader = none
```

`zoom = full` shows the same 192 CCK × 288 line PAL Standard viewport
we render. Disable the launcher's "border" trim if it's enabled.

For the screenshot itself: F12 → Save screenshot, or set
`screenshots_output_format = png` and use the in-emulator screenshot
key. FS-UAE writes to `~/Documents/FS-UAE/Screenshots/`.

## Per-machine configurations

| Golden file | Machine | Kickstart | RAM | Chipset |
|---|---|---|---|---|
| `a1000-ks12-512k-chip-frame250.png` | `model = A1000` | `kickstart_file = kick12.rom` | `chip_memory = 512` | OCS |
| `a500-ks13-512k-chip-frame250.png` | `model = A500` | `kickstart_file = kick13.rom` | `chip_memory = 512`, `slow_memory = 0` | OCS |
| `a500-ks13-512k-chip-512k-slow-frame250.png` | `model = A500` | `kickstart_file = kick13.rom` | `chip_memory = 512`, `slow_memory = 512` | OCS |
| `a500+-ks204-1m-chip-frame250.png` | `model = A500+` | `kickstart_file = kick204.rom` | `chip_memory = 1024` | ECS |
| `a600-ks205-1m-chip-frame250.png` | `model = A600` | `kickstart_file = kick205.rom` | `chip_memory = 1024` | ECS |
| `a1200-ks30-2m-chip-frame250.png` | `model = A1200` | `kickstart_file = kick30.rom` | `chip_memory = 2048` | AGA |
| `a1200-ks31-2m-chip-frame250.png` | `model = A1200` | `kickstart_file = kick31.rom` | `chip_memory = 2048` | AGA |

## Capture timing

Our tests run `BOOT_FRAMES = 250` PAL frames before sampling. At 50Hz
that's exactly 5 seconds of emulated time. Capture the FS-UAE screen
as close to the same point as possible.

For Kickstart 1.x insert-disk screens the display reaches its final
composed state by frame ~210 and is stable thereafter, so any capture
between frame 230 and end-of-animation matches.

## Cropping after capture

If FS-UAE saved at a different size (e.g. 752×572 from default
overscan settings), prefer reconfiguring FS-UAE rather than cropping —
cropping loses subpixel alignment and bakes filter assumptions into
the golden.

If you must crop, use a tool that preserves the source palette
(ImageMagick `convert -crop 768x576+X+Y +repage`) and verify the result
visually before committing.

## Validating a new golden

```sh
cargo test -p machine-commodore-amiga \
    --test kickstart_golden_frames -- --ignored --nocapture
```

On a mismatch, the helper writes `<stem>.actual.png` (our render) and
`<stem>.diff.png` (magenta = mismatched pixels) next to the golden.
Both are git-ignored — inspect, then either fix the emulator or, if
the emulator is correct and the golden was wrong, replace the golden.

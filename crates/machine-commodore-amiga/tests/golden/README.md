# Amiga golden frames

Pixel-exact reference images captured from FS-UAE for cross-validating our
Amiga emulator's display output. Any deviation is treated as a regression.

## Naming convention

```
<machine>-<kickstart>-<ram>-frame<N>.png
```

Captured to date:

| File | Chipset | Status |
|---|---|---|
| `a1000-ks12-512k-chip-frame250.png` | OCS | active |
| `a500-ks13-512k-chip-frame250.png` | OCS | active |
| `a500-ks13-512k-chip-512k-slow-frame250.png` | OCS | active — passes pixel-exact |
| `a500+-ks204-1m-chip-frame250.png` | ECS | skipped (no ECS yet) |
| `a600-ks205-1m-chip-frame250.png` | ECS | skipped (no ECS yet) |
| `a1200-ks30-2m-chip-frame250.png` | AGA | skipped (no AGA yet) |
| `a1200-ks31-2m-chip-frame250.png` | AGA | skipped (no AGA yet) |

`<N>` is the frame count after reset at which the screenshot was captured
(the test boots that many frames before sampling).

## Required dimensions

`768 × 576` ARGB8888 (or RGBA8) PNG.

This matches our runtime's screenshot path: PAL `Standard` viewport
(192 CCK × 288 lines), deinterlaced, then `to_display()` halves the
superhires width and doubles the deinterlaced height → 768×576 at
square-pixel 4:3.

FS-UAE can be configured to output exactly this size; do it that way
rather than cropping a different size. **Anything other than 768×576 is
a hard fail in the comparison helper** — pixel-exact comparison only
works when both images describe the same raster region at the same
resolution.

## Capture process

See `wiki/processes/golden-image-capture.md`.

## On failure

When a golden test fails, the comparison helper writes
`<name>.actual.png` and `<name>.diff.png` next to the golden so you can
diff visually. Magenta pixels in the diff are mismatches; greyscale pixels
match. Both `actual.png` and `diff.png` are git-ignored.

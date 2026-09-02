# @emu198x/zx-spectrum

A cycle-accurate ZX Spectrum 48K for the browser. The same emulator core as the
native Emu198x application, compiled to WebAssembly — its output is checked
pixel-for-pixel against the native build on every change.

The 48K ROM travels inside the package, so a page needs no firmware of its own.

## Install

```sh
npm install @emu198x/zx-spectrum
```

## Use

```js
import init, { Spectrum } from '@emu198x/zx-spectrum';

await init();
const spectrum = await Spectrum.createBundled(document.querySelector('canvas'));

let last = performance.now();
function frame(now) {
  spectrum.tick(now - last);
  last = now;
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
```

Pass elapsed real time to `tick`, not one frame per callback. The Spectrum runs
at 50.08 Hz and a display usually refreshes at 60 Hz or more; `tick` converts
elapsed time into whole machine frames, so the machine runs at its own speed
rather than the monitor's.

The canvas drawing buffer is resized to the machine's picture (352×296) and the
page controls the displayed size with CSS. Add `image-rendering: pixelated` or
the browser will blur the pixels.

## API

| Method | Purpose |
|---|---|
| `Spectrum.createBundled(canvas)` | Build a 48K on the ROM in this package. |
| `Spectrum.create(canvas, rom)` | Build a 48K on a ROM you supply. |
| `tick(elapsedMs)` | Run elapsed real time and draw. Returns frames run. |
| `loadSnapshot(bytes, format)` | Load a `.sna` or `.z80` snapshot. |
| `load(slot, kind, bytes)` | Load media — for example a tape into `tape-1`. |
| `keyDown(code)` / `keyUp(code)` | Feed a DOM `KeyboardEvent.code`. |
| `setAudioEnabled(on)` | Start or stop audio. |
| `configureAudio(rate, channels, capacity)` | Match the page's `AudioContext`. |
| `audioDrain()` | Take buffered samples to feed a worklet. |
| `frameRgba()` / `frameSize()` | The picture, for presenting it yourself. |
| `mediaSlots()` | Slot names this machine accepts. |
| `resize(width, height)` | Tell it the canvas changed size. |

`createBundled` and `create` are async because the API keeps room for a GPU
renderer, which needs an adapter.

Keys are mapped from `KeyboardEvent.code`, the physical key, so a learner on an
AZERTY or Dvorak layout presses the key that sits where the Spectrum's does.
Shift reaches `CapsShift`; Control and Alt reach `SymbolShift`. Cursor keys
expand to the chord the hardware actually uses — the Spectrum has no cursor
keys, and `Up` is `CapsShift`+`7`.

## ROM copyright

Amstrad have kindly given their permission for the redistribution of their
copyrighted material but retain that copyright.

The permission is Cliff Lawson's, for Amstrad plc, on comp.sys.sinclair,
31 August 1999:
<https://web.archive.org/web/20180828125931/http://www.worldofspectrum.org/permits/amstrad-roms.txt>

The ROM image is included unmodified, and no charge is made for it. The
permission covers the Sinclair 48K and 128K ROMs and Amstrad's +2/+2A/+3
machines. It does not extend to the ZX80, ZX81, Interface 1 or 2, Timex
machines, or Spectrum clones.

## Licence

The emulator is licensed under the terms in the Emu198x repository. The ROM is
copyright Amstrad plc and is redistributed under the permission above; it is
not covered by that licence.

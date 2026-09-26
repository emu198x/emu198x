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

The ROM image is included unmodified, and no charge is made for it. Versions
0.1.0 to 0.4.0 carried a 48K image with 272 bytes changed in the ROM's unused
area, from address `0x386E`; use 0.4.1 or later, which carries the unmodified
image (SHA1 `5ea7c2b824672e914525d1d5c419d71b84a426a2`). The
permission covers the Sinclair 48K and 128K ROMs and Amstrad's +2/+2A/+3
machines. It does not extend to the ZX80, ZX81, Interface 1 or 2, Timex
machines, or Spectrum clones.

## Licence

The emulator is licensed under the terms in the Emu198x repository. The ROM is
copyright Amstrad plc and is redistributed under the permission above; it is
not covered by that licence.

### Running inside a Web Worker

`Spectrum.createHeadless(rom)` and, in a firmware-bundled build,
`Spectrum.createHeadlessBundled()` create the same 48K machine without a DOM
canvas. Run `autoload()` and `tick(elapsedMs)` in the worker, then transfer the
`frameRgba()` buffer and `frameSize()` dimensions to the page for presentation.
The existing canvas-based constructors remain available.

Boot and turbo tape loading are synchronous operations. Keeping them in a worker
prevents those operations from blocking editing and other page interactions; it
does not remove their emulation cost. Keep at most one tick request in flight,
terminate the previous worker when starting a new run, and pause requests when
the page is hidden. The caller owns worker lifecycle, frame transfer and audio
presentation. No snapshot injection or tape-loading shortcut is implied.

## Numbered BASIC listings

The additive `basicTape(source, name)` export converts a numbered ASCII listing
into a self-starting BASIC TAP. It uses the format crate's `tokenise_listing`
path and the shared TAP writer, preserving expression text rather than passing
it through the analysis AST. Line numbers are sorted; duplicate line numbers,
empty programs, unsupported characters, unterminated strings and out-of-range
numbers return errors. This is tokenisation, not a BASIC grammar validator:
the ROM still reports execution and syntax errors when the tape runs.

This route is currently a bounded Code198x browser trial (Meet BASIC greeting
and Sonar). DEF FN parameter markers, embedded graphics/control codes and broader
source-notation compatibility remain outside it. The listing and headless worker APIs are available from npm version 0.4.0.

### Direct program execution

On a fresh 48K instance, `runBasic(source)` installs the tokenised listing in
RAM through the shared native BASIC loader, updates its system variables, and
types RUN through the ROM. It mounts no tape. `basicTape(source, name)` remains
the independent download/export path.

`runCode(bytes, origin, entry)` boots the ROM, installs the machine code and a
small BASIC launcher, then executes CLEAR followed by RANDOMIZE USR. This
preserves ROM services and a return stack. Code must start at or above 24576,
fit below 65536, and contain its entry point. Use a fresh instance for each run.
Both APIs are synchronous: worker hosts keep boot work off the page thread.
These APIs are available from npm version 0.4.0.

`readMemory(address, length)` reads the current visible address space for lesson
inspectors. It is read-only and rejects ranges outside the 64 KiB address space.
This API is available from npm version 0.4.0.

For lesson replay, call `enableScreenWriteTrace(address, length)` on a fresh machine before
`runCode()`, then read `screenWriteTrace()`. Choose a narrow bitmap range to avoid filling the capture with ROM clearing.
It returns captured writes within that range
(including ROM writes) and a `full` flag when the shared 8192-record cap is
reached. The caller can filter by program PC range. A replay is a recording,
not live stepping; a full capture must not be presented as complete. These
APIs are available from npm version 0.4.0.

For the bounded routine lesson, call `enableRoutineTrace(stop)` before
`runCode()`, then `routineTrace()`. The JSON recording contains actual unconditional
CALL/RET transitions and bitmap writes in $4000–$47FF, with before/after register
snapshots. Existing debugger stepping stops at the supplied PC, on leaving the
program, or after 4096 steps. Check `complete`: only reaching the supplied stop
address makes it true. The intended stop is the lesson's `hold` label. This is
bounded teaching instrumentation, not a general instruction trace. It is available
from npm version 0.4.0.

### Debugger controls

`debugState()` serialises shared debugger CPU state and disassembly/bytes at PC.
`debugStep()` invokes the existing bounded native step; `debugRunTo(address)`
runs to an instruction boundary with a fixed 14-million-half-cycle budget and
returns whether it reached the address. Invalid addresses are rejected. Both
execution methods deliver queued input first. Suspend normal frame ticks before
using them; no implicit rendering or real-time playback runs while paused.
A failed run-to is not a breakpoint hit, and a step at HALT can remain waiting.
These exports are available from npm version 0.4.0.

# Spectrum +3 disk loading is wired but doesn't actually load

**Status:** Known limitation. Date: 2026-05-08, partial diagnosis 2026-05-10.

## Observation

On the +3 with a `.dsk` (or `.edsk`) file inserted via File > Open
Disk..., selecting **Loader** from the boot menu sits on the Loader
screen indefinitely — no error, no advance, no "loading" indicator,
no boot. Reset and re-try produces the same hang.

## What we know works

- The native UI's Open Disk... dialog picks `.dsk` / `.edsk` / `.zip`,
  routes through the asset loader (zip-aware), and reaches
  `runtime.load_media(MediaSet { slot: "disk-a", … })`.
- `runtime-sinclair-zx-spectrum::variants::SpectrumAmstradClassCore<V>::load_disk_image`
  parses the bytes via `format-amstrad-dsk::parse` and calls
  `self.fdc.insert_disk(0, image)`. The +3's `Plus3Marker` enables
  the µPD765A FDC.
- `format-amstrad-dsk` itself parses both legacy `.dsk` and EDSK
  formats; its tests are green.

## What's not wired

The path from the +3 BIOS's Loader option through the µPD765A's
read-sector commands and back to the BIOS's track / sector
expectations isn't completing. Possibilities (none investigated yet):

- The FDC's command set is partially implemented — `insert_disk`
  populates the chip's drive state, but the BIOS's specific READ /
  SEEK / SENSE_INTERRUPT command sequences may not be answered the
  way the +3 BIOS expects.
- The FDC's interrupt or DRQ signalling isn't routed back to the Z80
  in a way the BIOS reads correctly.
- The disk image's track layout is fine for parsing but doesn't
  match what the +3 BIOS reads at boot (e.g., a CPC-format `.dsk`
  rather than a +3-format one — though the Loader hang suggests the
  BIOS gets nowhere, not "wrong data").

## Why we're not fixing it now

- Spectrum SOLID criterion 7 (Native UI) acceptance bar is
  "runtime file picker, snapshot save/load, runtime window-scale
  selector". The menu *wiring* for Open Disk works correctly: the
  item enables on +3, accepts `.dsk` / `.edsk` / `.zip`, parses, and
  reaches the FDC. The hang is a deeper issue at the FDC ↔ BIOS
  boundary, not a menu bug.
- Code198x's curriculum uses tape-based loading (`.tap`, `.tzx`)
  exclusively — no curriculum unit needs the +3 disk path for the
  October launch.
- The catalogue (criterion 1) currently has one +3 entry (Chase
  H.Q. +3) authored as a future-state placeholder; the harness
  doesn't run +3 disk-load assertions today.
- The FDC fix is genuinely cross-stack — needs tracing the BIOS's
  command sequence against the chip's responses, possibly with a
  reference emulator (Fuse, ZEsarUX) for comparison. Real engineering
  with its own focused commit when it lands.

## What would unblock the fix

1. Pick a known-good +3 disk image (a TOSEC "+3" tagged `.dsk` such
   as a Chase H.Q. +3 dump).
2. Add an FDC command-trace facility — log every µPD765A command the
   Z80 issues plus the chip's response. Compare against a reference
   emulator's trace of the same disk.
3. Identify where the trace diverges. Likely candidates:
   - A command opcode the chip stub doesn't handle.
   - Status register bits the BIOS polls that never flip.
   - The boot disk's first track layout vs what the BIOS expects.
4. Fix the divergence; re-run the trace; the boot menu's Loader
   should now reach the program.

This is criterion 3 (Formats) territory — DSK parsing is present
but DSK *loading* on +3 isn't end-to-end. Tracking it under that
criterion's existing PARTIAL state.

## 2026-05-10 — partial diagnosis

A temporary FDC command/result eprintln trace, driven by a one-off
test that booted Chase H.Q. (+3) and pressed ENTER on Loader for
3000 frames, captured the BIOS's actual sequence:

```
Specify(0x03 0x03 0x03)         ; pre-menu init
SeekTrack(0xaf 0xaf 0xaf)       ; pre-menu init (note: bogus
                                ;   parameters — junk on the bus
                                ;   from earlier code, but the
                                ;   FDC accepts and replies)
Specify(0x03 0x03 0x03)
SenseDriveStatus(drive=0)        → ST3=0x30 (T0|RY)
=== ENTER pressed ===
SenseDriveStatus(drive=0)        → ST3=0x30
Recalibrate(drive=3)            ; probing all drives
Recalibrate(drive=0)
SenseInterruptStatus            → ST0=0x20, PCN=0  (drive 0 SE)
SenseInterruptStatus            → ST0=0x23, PCN=0  (drive 3 SE)
SenseInterruptStatus            → ST0=0x80, PCN=0  (no more pending)
SenseInterruptStatus            → ST0=0x80, PCN=0
SenseInterruptStatus            → ST0=0x80, PCN=0
…(infinite loop)
```

The BIOS keeps polling SenseInterruptStatus indefinitely **even
after the FDC reports ST0=0x80 (Invalid Command, "no pending
interrupt")**. Per the µPD765A datasheet, ST0=0x80 is the
documented "queue drained" signal. So the BIOS is using a
different exit condition we don't model — most likely either a
status-register bit transition (e.g. drive-busy bits D0B–D3B in
MSR pulsing during seek) or the FDC's INT line going low. Our
FDC's `interrupt` field exists but isn't wired to anything; our
MSR doesn't model the per-drive busy bits.

What landed in `nec-upd765a` from this session — strictly
datasheet-correctness improvements, none enough to unblock the
hang on its own:

- **Per-drive `seek_pending: [Option<u8>; 4]`.** Multi-drive
  recalibrate/seek now queues per-drive interrupts; SenseInt
  walks the drives in order, returning each pending result then
  ST0=0x80 once drained. Replaces the previous behaviour that
  always returned the same cached ST0.
- **Recalibrate without disk fails as Abnormal | Not Ready | EC
  (ST0 = 0xD0 | drive).** Real µPD765A fails recalibrate on a
  non-existent drive (TR0 signal never asserts, step counter
  exhausts). Previously we returned Normal Termination for every
  drive 0-3, so the BIOS thought all four drives existed.
- **SenseDriveStatus ST3 includes HD + US bits from the command
  byte and TS (two-sided) when a disk is present.** Previously
  st3 was missing the head bit and never set TS, which would
  make the BIOS treat every disk as single-sided.

The infinite SenseInt loop persists with these fixes. The next
investigative step is a side-by-side trace against FUSE running
the same DSK — likely needed to identify which status bit
transition the BIOS is waiting for.

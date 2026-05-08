# Spectrum +3 disk loading is wired but doesn't actually load

**Status:** Known limitation. Date: 2026-05-08.

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

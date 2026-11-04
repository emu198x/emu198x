# Apple II

Original NTSC revision-0 Apple II with 48 KB RAM. The native window, JSON
scripts and live stdio MCP use the same published family runtime. The window
uses the existing shared UI for video, audio, input and menus.
No firmware or disks ship with the binary.

Supply an original E000 Monitor/Integer BASIC ROM (8192 bytes) and normalized
64-glyph bitmap (512 bytes):

```sh
cargo run -p emu198x-apple-ii -- \
  --rom /path/original-e000.rom --characters /path/characters-64.bin
```

This opens the native colour window. Type using the host keyboard layout;
letters become uppercase and shifted punctuation keeps its host meaning.
Left Alt/Option holds Apple Ctrl: Left Alt+B, then Return enters Integer BASIC.
Alternatively, type `E000G` and Return at the Monitor prompt. Try `PRINT 6*7`.
Use the shared Machine menu for Pause/Resume and Reset, and View for scaling
and filters. Audio uses the host output device; `--no-audio` runs silently.
Display geometry and frame pacing come from the NTSC raster and runtime clock.
The initial inverse characters reflect uncleared RAM; the original board also
retains colour burst in text, producing the modelled colour fringes.

For automation:

```sh
cargo run -p emu198x-apple-ii -- \
  --rom /path/original-e000.rom --characters /path/characters-64.bin \
  --script crates/emu198x-apple-ii/examples/basic.json --frames 0 \
  --screenshot /path/basic.png
```

The example waits for Monitor, enters Integer BASIC, runs a program and waits
for `42`. `--frames 0` leaves execution to the script. For live control, use the
same firmware flags with `--mcp`; tools include typing, queries, exact ticks,
frames, bounded waits, RAM/CPU inspection, stepping, captures and media export.
`--no-default-features` builds the headless binary without the UI dependency.

Disk II needs both `--p5` and `--p6` (256 bytes each). `--disk` / `--disk-2`
mount read-only WOZ disks in `floppy-0` / `floppy-1`. From Monitor, type `C600G`
then Return to boot slot 6. Use `load_media` with `writable: true` for a work
copy, wait for `disk.motor_on = false`, then `export_media` with format `woz`
and a new output path. Export refuses existing destinations. Source disks are
never overwritten. A running motor blocks replacement, ejection and export.

`boot.detected` recognizes a visible Monitor or BASIC prompt; wait for specific
`screen.text` for DOS or programs. Lowercase becomes uppercase. RESET preserves
RAM and disks; both reset kinds press hardware RESET. Save states, register
editing, memory/IO watches, cassette and later Apple II boards remain future
work. The family repository's [verification record](https://github.com/emu198x/apple-ii/blob/main/docs/verification.md)
contains fixture identities and explicit external-ROM tests.

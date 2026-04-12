# Roadmap

Active work for the four primary systems: Spectrum, C64, NES, and Amiga.
Priorities here are ordered by leverage, not by date.

For current support status, see [status.md](status.md). For architecture notes
and crate inventory, see [inventory.md](inventory.md). For systems beyond the
core four, see [future-systems.md](future-systems.md).

---

## Current Priorities

| Item                                 | Why now                                                            | Done when                                                                                                           | Dependencies                                                            |
| ------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| WHDLoad infrastructure               | Biggest post-floppy compatibility unlock for Amiga                 | Supported hard-drive installs boot through IDE, filesystem, and autoconfig path                                     | IDE device, filesystem layer, autoconfig                                |
| Save states                          | Required for lesson checkpoints and repeatable debugging workflows | Snapshot and restore works across all four primary systems                                                          | Stable machine snapshot format                                          |
| Observable state and trace recording | Needed for debugger UX and agent workflows                         | Chips and machines expose structured snapshot/query APIs and trace capture                                          | Shared observability API                                                |
| Visual debugger                      | Central education feature                                          | Registers, disassembly, memory, and video/audio state can be inspected live per system                              | Observable state, trace data                                            |
| WASM per-system builds               | Needed for browser-hosted lessons                                  | Each core system ships as a separate JS/WASM package with deterministic asset loading                               | Stable frontend API                                                     |

## Per-System Next Steps

### Spectrum

Emulator work is effectively complete. Remaining work is content capture and
lesson material rather than core emulation.

### Commodore 64

PAL and NTSC emulation are in good shape. The main accuracy task left is better
SID revision calibration from measured hardware data, plus content capture and
lesson material.

### NES

Emulator work is effectively complete. NTSC and PAL cartridge support with
14 mappers, 32/35 Blargg sub-tests passing (four full suites clean). The main
remaining gap is Famicom Disk System support (low priority). Focus shifts to
content capture and lesson material.

### Amiga

All primary and extended models boot to insert-disk or beyond. A500 runs
Workbench 1.3 desktop. OCS (A500/A1000/A2000), ECS (A500+/A600/A3000), and
AGA (A1200/A4000) chipsets are working. WHDLoad remains the largest
compatibility step for running game software from hard-drive images.

Known cosmetic gaps: A4000/A3000 insert-disk floppy icon body missing (sprite
rendering), A4000 KS 3.0 blank (different AGA init path), A3000 KS 2.02 white
(ROM-level scsi.device task-switch abandons InitCode loop).

## Education And Tooling Backlog

Priority tooling work is listed above. Baseline scripting, capture, and MCP
request/response control already work across all four systems. The backlog
below covers the remaining gaps, richer UI and debugging work, and per-system
content packs. Detailed design specs live in [features/](features/).

| Item                    | Status      | Notes                                                                            |
| ----------------------- | ----------- | -------------------------------------------------------------------------------- |
| Spectrum capture pack   | Not started | Timing-sensitive visual demo, hero screenshot, audio capture, and lesson draft   |
| C64 capture pack        | Not started | Badline visual demo, SID audio example, hero visual, and lesson draft            |
| NES capture pack        | Not started | Pipeline-focused visual demo, sprite or timing capture, and lesson draft         |
| Amiga capture pack      | Not started | Copper or Blitter visual demo, audio DMA example, hero capture, and lesson draft |
| Breakpoint conditions   | Not started | Expression-based breakpoints beyond address-only                                 |
| Launcher UI             | Not started | Per-system variant and option selection before boot                              |
| MCP event notifications | Not started | `breakpoint_hit`, `frame_complete`, and related push events                      |
| Input configuration UI  | Not started | Keyboard, joystick, gamepad, and mouse mapping                                   |
| Media panel widgets     | Not started | Tape, disk, and cartridge controls with drag-and-drop                            |

## References

- [status.md](status.md)
- [inventory.md](inventory.md)
- [future-systems.md](future-systems.md)
- [systems/amiga.md](systems/amiga.md)
- [systems/c64.md](systems/c64.md)
- [systems/nes.md](systems/nes.md)
- [systems/spectrum.md](systems/spectrum.md)

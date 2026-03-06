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
| Amiga support-chip bring-up          | Needed to unblock broader Amiga compatibility and model coverage   | Gary, Buster-family, DMAC, and remaining bus glue have standalone implementations or clearly bounded stubs in place | Amiga machine-core integration (`machine-amiga` today), pin-level tests |
| A1200/A4000 boot path                | Main remaining AGA milestone                                       | Kickstart 3.x reaches insert-disk on A1200 and A4000 with AGA path enabled                                          | AGA integration, support-chip behavior                                  |
| A3000 device init                    | Needed for the A3000 line to progress beyond STRAP                 | Kickstart 2.02 and 3.1 reach insert-disk on A3000                                                                   | DMAC/SCSI and bus-controller stubs                                      |
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

Cartridge-based NTSC and PAL support is in good shape. The main compatibility
gap is Famicom Disk System support, plus content capture and lesson material.

### Amiga

Current engineering focus is support-chip bring-up and AGA model boot. A3000
device init is the next major unblocker after AGA boot, and WHDLoad remains the
largest compatibility step once the boot path is stable.

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

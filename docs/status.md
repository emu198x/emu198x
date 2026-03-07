# Status

Current support snapshot for the four primary systems and the project-wide
tooling surface. This is a dashboard, not a roadmap.

For active work, see [roadmap.md](roadmap.md). For architecture notes and crate
inventory, see [inventory.md](inventory.md).

---

## Status Legend

| Label                  | Meaning                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------- |
| Production-ready       | Core workflows are working and no known blocking emulator gaps remain for the current scope |
| Usable with known gaps | Main workflows work, but notable compatibility or accuracy work remains                     |
| Booting                | ROM or OS boots or reaches a stable screen, but broader software support is not yet ready   |
| In progress            | Active implementation exists, but the system is not yet reliably booting or usable          |
| Not started            | No meaningful implementation yet                                                            |

## Core Systems

| System   | Status                 | Summary                                                                                                                        | Details                                    |
| -------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ |
| Spectrum | Production-ready       | 48K, 128K, +2, +2A, and +3 PAL; TAP, TZX, SNA, Z80, and DSK/EDSK; real-time EAR simulation                                     | [systems/spectrum.md](systems/spectrum.md) |
| C64      | Production-ready       | PAL and NTSC, all VIC-II display modes, 1541 read/write, REU, and PRG/D64/TAP/CRT support                                      | [systems/c64.md](systems/c64.md)           |
| NES      | Usable with known gaps | NTSC and PAL cartridge support, 14 mappers, battery-backed PRG RAM; FDS not implemented                                        | [systems/nes.md](systems/nes.md)           |
| Amiga    | Usable with known gaps | OCS and ECS Kickstart boots to insert-disk, Workbench 1.3 desktop on A500, AGA display path present, ADF and IPF media support | [systems/amiga.md](systems/amiga.md)       |

## Amiga Model Detail

The canonical Amiga model matrix lives in
[systems/amiga.md](systems/amiga.md#model-snapshot), with Kickstart-specific
boot detail in [systems/amiga.md](systems/amiga.md#kickstart-boot-status).
At dashboard level: A500 is usable beyond insert-disk, A2000/A500+/A600 are
booting, A1000/A1200/A3000/A4000 are under bring-up, and CDTV/CD32 are not
started.

## Tooling Snapshot

| Area                         | Status                 | Notes                                                                                                   |
| ---------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------- |
| Scripting and batch control  | Usable with known gaps | All four runners support `--script`; richer breakpoint and trace workflows remain open                  |
| Capture and export           | Usable with known gaps | PNG screenshots, WAV capture, and recording work via script or MCP; unified CLI remains open            |
| MCP request/response control | Usable with known gaps | Cross-system query/control surface exists; push events, save states, and conditions remain open         |
| Frontend UX                  | Not started            | Native runners exist, but launcher screens, media panels, input UI, and debugger layouts are not built  |
| Save states                  | Not started            | Planned for lesson checkpoints and deterministic replay                                                 |
| Observability and trace      | In progress            | Path-based query and discovery exist; snapshots, trace capture, and richer debugger state remain open   |
| Visual debugger              | Not started            | Depends on observability and trace                                                                      |
| WASM builds                  | Not started            | Needed for browser-hosted lessons                                                                       |

## References

- [roadmap.md](roadmap.md)
- [inventory.md](inventory.md)
- [systems/amiga.md](systems/amiga.md)
- [systems/c64.md](systems/c64.md)
- [systems/nes.md](systems/nes.md)
- [systems/spectrum.md](systems/spectrum.md)

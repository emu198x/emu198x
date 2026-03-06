# Documentation Map

Use this file as the starting point for navigating `docs/`.

## Primary Entry Points

- [roadmap.md](roadmap.md) - active work and current priorities
- [status.md](status.md) - high-level support snapshot across systems and tooling
- [inventory.md](inventory.md) - architecture notes, crate inventory, and testing strategy
- [future-systems.md](future-systems.md) - systems explicitly out of scope until the core four are complete

## System Documentation

- [systems/amiga.md](systems/amiga.md) - Amiga technical notes and detailed Emu198x status
- [systems/c64.md](systems/c64.md) - C64 technical notes and detailed Emu198x status
- [systems/nes.md](systems/nes.md) - NES technical notes and detailed Emu198x status
- [systems/spectrum.md](systems/spectrum.md) - Spectrum technical notes and detailed Emu198x status

## Feature Specifications

`docs/features/` holds feature docs and design specs for tooling and frontend
work such as observability, MCP integration, capture, scripting, and launcher
behavior. Some files describe implemented or partially implemented behavior;
each file should state its current status near the top.

## Reference Material

- `docs/platforms/` contains extracted platform reference material, mostly taken from books and manuals
- `docs/Reference/` contains imported or archival reference material already present in the repository; treat it as background material, not current project docs

## Engineering Notes

`docs/solutions/` contains implementation, testing, and debugging notes from
problems encountered during emulator development. See
[solutions/README.md](solutions/README.md) for curation rules and templates.

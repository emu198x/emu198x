# Dragon 32

**Status:** sixth supported family, Codex-owned active development. Not in October launch scope.

## Current state

The Dragon 32 reaches BASIC over a real Microsoft / Dragon Data BASIC ROM, accepts CAS tape media (BASIC + machine-code via `CLOAD` / `CLOADM` / `EXEC`), mounts ROM and DGN cartridges, restores PC-Dragon PAK snapshots as machine state, plays PIA-driven audio, and accepts joystick input. A 12-title application smoke matrix compares headless screenshots against a patched XRoar reference at 11/12 exact matches.

This page is a placeholder. Codex owns the detailed Dragon line of work; when it stabilises, this page should be replaced with a proper overview matching `wiki/systems/commodore-c64.md` or `wiki/systems/nintendo-game-boy/overview.md`.

## Crates

- `motorola-6809` — the Dragon CPU
- `motorola-pia-6821` — the two PIAs (`U2` keyboard / cassette / VDG mode, `U4` audio / joystick / cartridge)
- `motorola-sam-6883` — synchronous address multiplexer; owns the video display register and memory map
- `motorola-vdg-6847` — video display generator
- `format-dragon-cas` — CAS cassette parser
- `format-dragon-pak` — PC-Dragon PAK snapshot reader
- `machine-dragon-32` — the system wiring
- `runtime-dragon` — `MachineCore` runtime over the machine
- `emu198x-dragon` — native verifier shell
- `emu198x-script-dragon` — headless smoke runner with optional XRoar reference

## Notably not done yet

- Dragon 64 mode
- Cartridge audio
- Disk support
- Beam-accurate VDG sub-line composition (current renderer is line-accurate per scanline)

## References

- README — `## Running` / `### Dragon 32 native verifier shell` for live commands
- `wiki/log.md` — the 2026-04-26 to 2026-04-28 entry summarises the family stand-up

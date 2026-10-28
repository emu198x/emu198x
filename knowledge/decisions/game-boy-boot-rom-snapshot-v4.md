# Optional monochrome boot firmware and snapshot version 4

Accepted 2026-10-28 for #323. The user approved optional 256-byte DMG0/DMG/MGB
firmware and version-4 snapshots with an explicit version-3 compatibility reader.

Firmware is optional. Absent firmware uses the existing skipped-boot profile;
selected firmware starts CPU/peripherals at reset and executes the ROM through
the machine clock. The monochrome overlay covers $0000–$00FF. BOOT bit 0 unmaps
it irreversibly until reset. ROM execution, rather than a post-boot reseed,
produces the handoff state. SGB execution remains #326.

The runtime advertises per-model optional firmware sources and validates sizes
before constructing a machine. New fallible machine constructors cover both
monochrome and colour firmware; existing infallible CGB-only constructors keep
their API and compatibility policy. No dependency or CPU/PPU timing rule changes.

Version 4 persists the monochrome image/latch and selected firmware for reset,
including a firmware-equipped runtime with no cartridge. Restore checks model,
firmware length, hardware and duplicate image consistency before replacing live
state. Version 3 uses its exact old machine layout via `GameBoyV3`, not defaults
on appended postcard fields. Independent baseline fixtures for all seven profiles
and advancing DMG/CGB continuations enforce compatibility across future changes.

Firmware bytes stay in the local ignored asset library and user ROM directory.
Committed snapshots contain an original synthetic cartridge and no boot ROM.
Mapping evidence and dump identities are in the umbrella reference library at
`reference/by-system/nintendo-gameboy/boot-rom-evidence.md`.

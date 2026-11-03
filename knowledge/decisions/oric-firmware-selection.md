# Oric ROM profiles have distinct firmware identities

Adopted 2026-11-03 for #345, following the approved October correction plan.
Automatic Oric-1 selection must not borrow Atmos BASIC 1.1.

- Oric-1 requires `oric1-rom`, reads `EMU198X_ORIC1_ROM`, and searches
  `oric-1.rom`, `oric1.rom`, then `basic10.rom`.
- Atmos retains `oric-rom`, `BIOS_FIRMWARE_ID`, `EMU198X_ORIC_ROM`, and its
  `atmos.rom` / `oric.rom` convention, with `basic11b.rom` also accepted.
- Both use the shared firmware directory convention. A directory containing
  only `oric.rom` cannot satisfy the Oric-1 requirement. MCP may still start
  blank under its existing contract; it must not load the other model's ROM.
- A bare `--rom PATH` binds after the final model is known, in either argument
  order. An explicit `ID=PATH` keeps its named ID and rejects for the wrong
  model. UI, headless and MCP startup use the same resolved overrides.
- Explicit custom and localised firmware remains valid. Production uses no
  known-hash allowlist. SHA-256 gates belong to the opt-in reference tests,
  which distinguish the original BASIC banners and preserve the STR$ bug.

The change affects firmware selection and errors, not CPU/ULA behaviour or
snapshot layout/version. The live snapshot still contains its ROM bytes and
model. These ROM profiles do not establish physical PCB issue, change-note
fitting or amplifier identity; board qualification remains under #2144.

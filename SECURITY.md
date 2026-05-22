# Security Policy

## Supported Versions

Emu198x is pre-1.0. Only `main` receives security fixes at this stage.

## Reporting a Vulnerability

Email **steve@stevehill.xyz** privately. Do not file a public issue.

If you prefer, open a **GitHub Security Advisory** on the repository — that's
also private until disclosed.

Expect an acknowledgement within 7 days. Fix timelines vary by severity.

## Scope

Emulators parse untrusted format input from arbitrary sources. The following
input surfaces are in scope:

- Format parsers: TAP, TZX, SNA, Z80, ADF, D64, T64, PRG, BAS, iNES (NES
  cartridges), Game Boy cartridges, CAS, VDK, DGN, PAK, DSK
- Snapshot deserialisation: postcard and rmp-serde envelopes
- ZIP archive extraction (used by several format loaders)
- Path handling in snapshot save/load and ROM/disk loader paths

Issues we'd want to know about:

- Memory corruption (panics in safe code count too, given
  `unsafe_code = "forbid"` workspace-wide)
- Integer overflow or underflow in parsers that affects subsequent memory
  access
- Path traversal in any code that touches the filesystem
- Denial of service via crafted format input (infinite loops, unbounded
  allocations)
- Anything else that lets a crafted ROM, tape, or disk reach unintended
  behaviour beyond the emulated machine boundary

## Out of Scope

- ROM and disk image legality and copyright — see the [Getting
  ROMs](README.md#getting-roms) section. Take legal questions through normal
  channels.
- Issues in the host operating system, GPU driver, audio driver, or other
  third-party dependencies — report those upstream.
- "This game doesn't work" — that's a normal bug, file an issue.

## Disclosure

After a fix lands, the advisory is published. Credit is given to reporters by
default unless they prefer anonymity.

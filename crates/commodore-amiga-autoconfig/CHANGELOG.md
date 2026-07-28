# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- [breaking] Rename the public partial configuration state to
  `WaitingHighBase { lo }` so its name and field describe the corrected
  low-then-high base-address handshake

### Added

- Allow Zorro-II fast-RAM functions to provide explicit manufacturer,
  product and serial identity
- Expose board-local RAM storage, mapped-address membership and reset helpers
  for expansion-board integrations
- Expose persisted identity and backing-size coherence checks for snapshot
  validation

### Fixed

- Keep the complete `ER_TYPE` byte uninverted so the host sees the advertised
  memory flag and size code instead of an I/O-shaped phantom function
- Advertise Fast RAM in the dedicated eight-megabyte Zorro-II memory space
  through `ER_FLAGS.MEMSPACE` instead of setting the unrelated no-shut-up bit
- Latch A19-A16 at `$4A` before the final A23-A20 write at `$48`, which
  configures the board and releases the next Autoconfig function
- Reject serialized partial base nibbles and configured bases that are
  unaligned or outside the 24-bit Zorro-II address space

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-amiga-autoconfig-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Add Amiga postcard snapshots across the chip stack
- land wb13 boot investigation and fixes
- Zorro-II autoconfig fast RAM (step 2 of 3)

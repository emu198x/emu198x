# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- Implements the `Ula` trait from `emu198x-bus-sinclair-zx-spectrum` over the
  engine in `emu198x-sinclair-zx-spectrum-ula`, where both moved from the
  family's glue crate; the crate no longer depends on `common-sinclair-zx-spectrum`.
- First release under the `emu198x-amstrad-ula-40077` name. Until now the crate was
  `amstrad-ula-40077`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/amstrad-ula-40077-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Tree housekeeping: project relocation paths + Cargo.lock
- Reattach Spectrum ULA timing config on snapshot restore
- Add ZX Spectrum +2A/+2B/+3 + Amstrad 40077 + NEC µPD765A + DSK

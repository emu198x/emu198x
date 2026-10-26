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
- First release under the `emu198x-sinclair-ula-7k010e` name. Until now the crate was
  `sinclair-ula-7k010e`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

### Fixed

- align the active-border contention table with HALT2INT128's complete early-128K profile

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/sinclair-ula-7k010e-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Tree housekeeping: project relocation paths + Cargo.lock
- Reattach Spectrum ULA timing config on snapshot restore
- Add ZX Spectrum 128K + Sinclair 7K010E ULA + AY-3-8912

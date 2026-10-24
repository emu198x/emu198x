# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- First release under the `emu198x-gi-ay-3-8912` name. Until now the crate was
  `gi-ay-3-8912`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/gi-ay-3-8912-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Tree housekeeping: project relocation paths + Cargo.lock
- AY R14 / R15: model the Sinclair 128K port-A pull (0xBF)
- Expose AY-3-8912 register file as runtime queries
- Apply mechanical Rust formatting cleanup
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Add ZX Spectrum 128K + Sinclair 7K010E ULA + AY-3-8912

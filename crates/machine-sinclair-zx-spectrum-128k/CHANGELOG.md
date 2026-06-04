# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-sinclair-zx-spectrum-128k-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(spectrum)* lock golden screenshots for 6 ULA / contention TAPs
- *(spectrum)* wire the remaining 5 ULA / contention TAPs as smokes
- Open Emu198x for public release
- Float128K harness landed and strict-asserted (Seam 5)
- Fix new clippy lints introduced by Rust 1.95.0
- Gate Spectrum-side line coverage at 90% in CI
- Complete Spectrum SOLID variant coverage via class layer crates
- Lock Spectrum SOLID criteria; extract SNA and snapshot crates
- Paging-aware glyph reader: 4 more Spectrum banners confirmed
- Run rustfmt across the workspace
- Consolidate Spectrum per-machine boilerplate
- Clean up Spectrum family architecture
- Wrap every Spectrum variant in a generic MachineCore runtime
- Factor SpectrumDriver trait + .z80 snapshot helpers across 7 machines
- Add ZX Spectrum 128K + Sinclair 7K010E ULA + AY-3-8912

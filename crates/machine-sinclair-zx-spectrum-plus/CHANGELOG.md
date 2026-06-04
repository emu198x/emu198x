# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-sinclair-zx-spectrum-plus-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Rename BoardIssue → UlaRevision with explicit revision variants
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Add boot tests for the 7 remaining in-scope Spectrum variants
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Complete Spectrum SOLID variant coverage via class layer crates
- Lock Spectrum SOLID criteria; extract SNA and snapshot crates
- Paging-aware glyph reader: 4 more Spectrum banners confirmed
- Run rustfmt across the workspace
- Consolidate Spectrum per-machine boilerplate
- Clean up Spectrum family architecture
- Wrap every Spectrum variant in a generic MachineCore runtime
- Factor SpectrumDriver trait + .z80 snapshot helpers across 7 machines
- Add ZX Spectrum +2A/+2B/+3 + Amstrad 40077 + NEC µPD765A + DSK

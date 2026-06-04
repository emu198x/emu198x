# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-sinclair-zx-spectrum-48k-v0.2.0) - 2026-06-04

### Fixed

- *(z80)* preserve WZ across INIR/INDR/OTIR/OTDR repeat path

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(spectrum)* lock golden screenshots for 6 ULA / contention TAPs
- *(spectrum)* wire the remaining 5 ULA / contention TAPs as smokes
- cargo fmt --all across the workspace
- Open Emu198x for public release
- 48K machine: 3 new edge-case tests for apply_input_event branches
- Float48K strict assertion un-gated (architecture review Seam 5 #2)
- Rename BoardIssue → UlaRevision with explicit revision variants
- Kempston joystick input routing (Seam 2)
- Float48K + architecture review: reflect Seam 1 status
- switchable RST 16 / PR-ALL capture, env-gated scroll suppression
- Add Spectrum-validated CPU and ULA test harnesses
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Add boot tests for the 7 remaining in-scope Spectrum variants
- Complete Spectrum SOLID variant coverage via class layer crates
- add native channel controls
- Run rustfmt across the workspace
- Tidy Spectrum runtime layering and file layout
- Consolidate Spectrum per-machine boilerplate
- Clean up Spectrum family architecture
- Factor SpectrumDriver trait + .z80 snapshot helpers across 7 machines
- Fix Spectrum tape loading and add Manic Miner regression
- Fix Spectrum Symbol Shift host mapping
- Add Spectrum machine timing integration checks
- Add Spectrum snapshots and headless runner
- Add Spectrum media runtime and beeper audio
- Add Spectrum tape progression and boot smoke test
- Add Ferranti ULA and 48K frame loop
- Add Spectrum 48K machine input and FE port state

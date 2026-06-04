# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/common-sinclair-zx-spectrum-v0.2.0) - 2026-06-04

### Fixed

- clear clippy warnings hidden behind the muda compile failure

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- watch_ay_* — AY register-write tracer
- full-trace memory_read / poke / watch_memory tools
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Snapshot test: account for AY R14/R15 input-mode pin pull
- Tree housekeeping: project relocation paths + Cargo.lock
- CRT filter: Smith Ch 16 luminance + chroma bleed + Q3 saturation
- FDC disk survives snapshot restore + serde_skip audit (Seam 3)
- AOLatch border granularity (Smith Ch 14)
- two-stage shifter pipeline (Seam 1 of architecture review)
- add pending-latch fields for two-stage shifter
- derive from Smith Ch 16 Table 16-1 per-primary currents
- routing-version gating for hash re-capture discipline
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- pinpoint wipe trigger at $fd6c (L=$28, want $3A)
- RAM-dump harness + tape-edge timing ruled out
- pause=0 in data blocks means "no pause", not "stop"
- Wire portable .sna / .z80 snapshot import; rename State menu honestly
- Lift Kempston joystick to a Peripheral, migrate all hosting machines
- Add Spectrum16kMemory and lock D8 (extract SpectrumMachineCore)
- Lock Spectrum SOLID criteria; extract SNA and snapshot crates
- directed-test passes on actionable workspace gaps
- add native channel controls
- Run rustfmt across the workspace
- Tidy Spectrum runtime layering and file layout
- Consolidate Spectrum per-machine boilerplate
- Clean up Spectrum family architecture
- Factor SpectrumDriver trait + .z80 snapshot helpers across 7 machines
- Add Pentagon, Scorpion ZS-256, and Timex SCLD variant ULAs
- Add ZX Spectrum +2A/+2B/+3 + Amstrad 40077 + NEC µPD765A + DSK
- Add ZX Spectrum 128K + Sinclair 7K010E ULA + AY-3-8912
- Fix Spectrum tape loading and add Manic Miner regression
- Add Spectrum snapshots and headless runner
- Add Spectrum media runtime and beeper audio
- Add Spectrum tape progression and boot smoke test
- Add Ferranti ULA and 48K frame loop
- Add Spectrum 48K common memory and timing

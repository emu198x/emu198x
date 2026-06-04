# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/nec-upd765a-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- µPD765A FDC: 11 new tests covering simpler-command paths
- FDC disk survives snapshot restore + serde_skip audit (Seam 3)
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- model exec-phase read timeout + rotational ReadID
- model marginal-encoding physics on CRC-erred sectors
- result-phase R reflects abort sector vs EOT+1 correctly
- Carry per-sector ST1/ST2 + DDAM through the EDSK pipeline
- Chase H.Q. (+3) title screen now loads end-to-end
- +3 disk Loader now runs end-to-end (architecturally)
- model MSR drive-busy bits + seek-completion timing
- drain seek interrupts per-drive, fail empty-drive Recalibrate
- Fix more clippy lints from Rust 1.95.0
- Run rustfmt across the workspace
- Add ZX Spectrum +2A/+2B/+3 + Amstrad 40077 + NEC µPD765A + DSK

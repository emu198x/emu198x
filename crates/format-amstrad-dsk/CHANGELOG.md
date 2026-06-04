# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/format-amstrad-dsk-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- Carry per-sector ST1/ST2 + DDAM through the EDSK pipeline
- Chase H.Q. (+3) title screen now loads end-to-end
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Add ZX Spectrum +2A/+2B/+3 + Amstrad 40077 + NEC µPD765A + DSK

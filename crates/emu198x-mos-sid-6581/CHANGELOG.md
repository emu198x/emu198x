# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- allocation-free reusable mixed and per-voice audio drains for real-time consumers

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/mos-sid-6581-v0.2.0) - 2026-06-04

### Fixed

- stop SID envelopes from silencing notes gated after warm-up

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- C64 + NES Seam 4: catalogue oracle integrity
- add native channel controls
- Run rustfmt across the workspace
- SID 6581 4096-entry combined waveform ROM tables from reSID
- VIA 6522 ORA-alt + IER bit 7; SID envelope gate-bug
- SID noise taps + ADSR rates + TEST; CIA 6526 alarm; 68000 cycle fixes
- Add C64 native verifier shell
- Wire live SID into C64 runtime

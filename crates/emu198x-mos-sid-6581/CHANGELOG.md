# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking:** `Voice::clock_accumulator` takes the `SidModel`, and `Voice` carries private waveform-generator state, so it can no longer be built with a struct literal; use `Voice::new`. Control-register writes go through the new `Voice::write_control` to apply the TEST edges (#777)

### Added

- allocation-free reusable mixed and per-voice audio drains for real-time consumers

### Fixed

- drive the pulse waveform high while the accumulator is at or above the pulse width, not below it (#777)
- ring modulation substitutes MSB EOR NOT source MSB, and sawtooth blocks it (#777)
- ground the triangle waveform's DAC bit 0 (#777)
- holding TEST drifts the noise register to all ones over reSID's per-model delay instead of reseeding it every cycle, and releasing TEST shifts it once (#777)

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

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- power up with the accumulator at `0x555555` and the noise register at `0x7FFFFE`, as real chips read from OSC3 straight after power-up (#1606)
- sample the noise waveform from shift-register bits 20, 18, 14, 11, 9, 5, 2 and 0, the die-photo positions reSID 1.0 and reSIDfp use, instead of reSID 0.16's 22, 20, 16, 13, 11, 7, 4 and 2 (#1606)
- 8580 combined waveforms read reSID's OSC3 samples from a real 8580 instead of ANDing the selected waveforms (#769)
- noise combined with pulse pulls further bits down, per model, as reSID's `noise_pulse6581`/`noise_pulse8580` do (#769)
- noise combined with another waveform writes the selector's zeros back into the noise shift register, so it locks at zero until TEST refills it; releasing TEST from a combination writes back first on the transitions real chips show (#769)

## [0.8.0] - 2026-10-06

### Changed

- **Breaking:** `Voice::clock_accumulator` takes the `SidModel`, and `Voice` carries private waveform-generator state, so it can no longer be built with a struct literal; use `Voice::new`. Control-register writes go through the new `Voice::write_control` to apply the TEST edges (#777)

### Added

- `Voice::output`, `Voice::clock_output` and `Voice::latch_output`, the latched waveform DAC input (#777)
- `Sid6581::cpu_read`, the CPU read path that drives the SID data bus; `Sid6581::read` stays side-effect-free (#777)

### Fixed

- drive the pulse waveform high while the accumulator is at or above the pulse width, not below it (#777)
- ring modulation substitutes MSB EOR NOT source MSB, and sawtooth blocks it (#777)
- ground the triangle waveform's DAC bit 0 (#777)
- holding TEST drifts the noise register to all ones over reSID's per-model delay instead of reseeding it every cycle, and releasing TEST shifts it once (#777)
- reads of write-only and undecoded registers return the last byte on the SID data bus, which discharges to zero after $1D00 (6581) or $A2000 (8580) cycles (#777)
- deselecting every waveform leaves the waveform DAC input floating: it holds its last value, which OSC3 reads, and fades bit by bit after about 200 ms (6581) or 5 s (8580) instead of dropping to zero (#777)

## [0.7.0] - 2026-08-31

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

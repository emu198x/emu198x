# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.6.1

### Added

- A YM2149 variant (`Variant`, `Ay3_8910::variant` and `set_variant`). The
  YM2149 is register-compatible but steps its envelope through 32 levels at
  twice the rate, so an envelope's period is unchanged, and its DAC has 32
  levels nominally 1.5 dB apart. The variant is configuration, not state: it
  is not serialised, so existing snapshots keep their layout and a host sets
  it again after restoring one.
- `Ay3_8910::output`, the three channels' mixed output at this instant, for
  hosts that mix the chip at their own clock instead of taking the resampled
  frame buffer, and `envelope_level`. The chip is now `Clone`.

### Fixed

- Reading port B in output mode combines the output latch with the external
  pin levels, as port A already did, so grounded controller and keyboard
  pins stay visible.

## 0.6.0

- First release under the `emu198x-gi-ay-3-8910` name.

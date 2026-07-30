# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Preserve the raw byte awaiting a host handshake in serialized keyboard state.

### Added

- Add a side-effect-free diagnostic snapshot covering the complete implemented
  keyboard protocol state, timers, in-flight and queued bytes, byte-level serial
  progress, reset sequence, timeout behaviour, and counters.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/peripheral-commodore-amiga-keyboard-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Add Amiga postcard snapshots across the chip stack
- land wb13 boot investigation and fixes
- Retire peripheral-commodore-amiga-keyboard-archive: archive is now live
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Add fresh Amiga headless baseline

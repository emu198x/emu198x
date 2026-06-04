# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-atari-5200-v0.2.0) - 2026-06-04

### Added

- 6502 group on the shared debug tools (8 machines + Atari 800XL)
- *(borders)* atari-gtia — canonical TV-visible 384x288 frame
- extract Atari 5200 SuperSystem from donor codebase

### Fixed

- Atari 5200 boots Pac-Man end-to-end to its menu screen
- *(6502)* move items before the test module (clippy items_after_test_module)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- workspace clippy autofix sweep

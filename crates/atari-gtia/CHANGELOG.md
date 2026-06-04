# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/atari-gtia-v0.2.0) - 2026-06-04

### Added

- Atari 800XL MCP inspection tools (memory + chip state)
- *(borders)* atari-gtia — canonical TV-visible 384x288 frame
- *(chips)* port atari-antic + atari-gtia + atari-pokey for Atari 8-bit family

### Fixed

- Atari GTIA CONSOL read returns switches, not the speaker latch
- Atari 800XL renders the GR.0 screen correctly

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(atari)* chip-state accessors + 800XL boot probe

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/atari-antic-v0.2.0) - 2026-06-04

### Added

- Atari 800XL MCP inspection tools (memory + chip state)
- *(chips)* port atari-antic + atari-gtia + atari-pokey for Atari 8-bit family

### Fixed

- ANTIC text modes 4-7 decode colour and glyphs correctly
- Atari 800XL renders the GR.0 screen correctly
- Atari 800XL cold-boots into a programmed display

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(atari-antic)* use slice contains() in a test (clippy::manual_contains)
- *(atari)* chip-state accessors + 800XL boot probe

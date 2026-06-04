# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/atari-pokey-v0.2.0) - 2026-06-04

### Added

- Atari 800XL MCP inspection tools (memory + chip state)
- Atari 800XL keyboard input
- Atari 800XL boots to BASIC READY (POKEY serial transmit)
- *(chips)* port atari-antic + atari-gtia + atari-pokey for Atari 8-bit family

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- clear pre-existing clippy lints in atari-pokey and atari-maria

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-atari-800xl-v0.2.0) - 2026-06-04

### Added

- 6502 group on the shared debug tools (8 machines + Atari 800XL)
- Atari 800XL MCP run_until_pc + keyboard tools
- Atari 800XL MCP inspection tools (memory + chip state)
- Atari 800XL keyboard input
- Atari 800XL boots to BASIC READY (POKEY serial transmit)
- *(borders)* atari-gtia — canonical TV-visible 384x288 frame
- extract Atari 800XL from donor codebase

### Fixed

- *(6502)* move items before the test module (clippy items_after_test_module)
- Atari GTIA CONSOL read returns switches, not the speaker latch
- Atari 800XL renders the GR.0 screen correctly
- Atari 800XL cold-boots into a programmed display

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(800xl)* pin down the 800XL halt root cause
- *(800xl)* boot probe — PC histogram for the OS-XL stall
- *(atari)* chip-state accessors + 800XL boot probe
- workspace clippy autofix sweep

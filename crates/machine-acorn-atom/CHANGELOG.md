# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-acorn-atom-v0.2.0) - 2026-06-04

### Added

- 6502 group on the shared debug tools (8 machines + Atari 800XL)
- *(borders)* acorn atom MC6847 inline — green-phosphor border
- extract Acorn Atom from donor codebase

### Fixed

- *(6502)* move items before the test module (clippy items_after_test_module)

### Other

- Merge pull request #8 from emu198x/release/independent-per-machine
- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- workspace clippy autofix sweep
- *(atom)* migrate inline MC6847 to shared motorola-vdg-6847 crate

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/format-sinclair-zx-spectrum-bas-v0.2.0) - 2026-06-04

### Added

- AST-based parser for Spectrum BASIC tokenisation

### Fixed

- parse two-argument BASIC functions (ATTR, POINT, SCREEN$)
- remove <> <= >= from KEYWORDS table to fix operator parsing

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- Open Emu198x for public release
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- Port the ZX Spectrum BASIC tokeniser into a fresh crate

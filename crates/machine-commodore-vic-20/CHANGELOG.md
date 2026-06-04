# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-vic-20-v0.2.0) - 2026-06-04

### Added

- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(borders)* VIC-20 inline VIC chip — TV-visible frame with border
- extract Commodore VIC-20 from donor codebase

### Fixed

- VIC-20 boots to BASIC READY — reset the CPU + correct ROM map

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* make the debug-target macros storage-agnostic
- rustfmt the workspace clean
- clear remaining workspace clippy issues
- workspace clippy autofix sweep
- *(vic-20)* extract VIC 6560/6561 into mos-vic-i chip crate

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Use the exact 64,584-T-state frame budget in MCP mode so one requested frame
  cannot execute two machine frames.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-jupiter-ace-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(parity)* Jupiter Ace / Acorn Atom / Oric Atmos / SVI-328 operational parity
- extract Jupiter Ace from donor codebase

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean

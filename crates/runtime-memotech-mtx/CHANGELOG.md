# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Advance live-machine snapshots to version 3 and rehydrate the Z80 walker sequence before resumed execution; version 2 cannot preserve accepted interrupt response identity

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-memotech-mtx-v0.2.0) - 2026-06-04

### Added

- *(mtx)* boot to BASIC Ready — support paged ROMs (OS + BASIC + ASSEM)
- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(mtx)* operational parity — runtime crate, MCP server, shell-backed script

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- workspace clippy autofix sweep

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-msx-v0.2.0) - 2026-06-04

### Added

- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(msx)* operational parity — runtime crate, MCP server, shell-backed script
- *(borders)* TMS9918 — canonical TV-visible frame with border
- *(emu198x-msx)* MSX1 headless runner + gated BIOS-boot smoke

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- clear remaining workspace clippy issues
- workspace clippy autofix sweep

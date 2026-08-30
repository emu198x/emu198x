# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Load and flush Master System battery-backed cartridge SRAM through `.sav` sidecars in UI and headless modes

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-sega-master-system-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(sms)* operational parity — runtime crate, MCP server, shell-backed script
- *(borders)* sega-vdp — canonical TV-visible frame with border
- *(emu198x-sega-master-system)* headless runner + Alex Kidd live boot

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- clear remaining workspace clippy issues

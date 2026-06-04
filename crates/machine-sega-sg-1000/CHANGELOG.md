# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-sega-sg-1000-v0.2.0) - 2026-06-04

### Added

- debug-target macros + SG-1000 on the shared debug tools
- *(sg-1000)* operational parity — runtime crate, MCP server, shell-backed script
- *(borders)* TMS9918 — canonical TV-visible frame with border
- *(emu198x-sega-sg-1000)* headless runner + cart boot smoke
- *(machine-sega-sg-1000)* fresh-write SG-1000 / SC-3000 machine wiring

### Fixed

- *(z80)* reliable single-instruction stepping via a retirement counter

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait

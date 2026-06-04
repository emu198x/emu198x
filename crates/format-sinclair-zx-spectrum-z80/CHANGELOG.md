# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/format-sinclair-zx-spectrum-z80-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Lock Spectrum SOLID criteria; extract SNA and snapshot crates
- Fix two parse_z80 over-reads (lib.rs:149, lib.rs:163)
- Remove accidental Spectrum snapshot changes
- Expand 6809 timing validation
- directed-test passes on actionable workspace gaps
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Add format-sinclair-zx-spectrum-z80 (.z80 + .sna snapshots)

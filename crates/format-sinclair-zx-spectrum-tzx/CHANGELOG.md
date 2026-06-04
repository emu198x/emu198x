# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/format-sinclair-zx-spectrum-tzx-v0.2.0) - 2026-06-04

### Fixed

- clear clippy warnings hidden behind the muda compile failure

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- 18 new tests covering all parser block IDs
- Tree housekeeping: project relocation paths + Cargo.lock
- Fix TZX partial-last-byte parsing — unblocks Speedlock-7 tape loading
- pause=0 in data blocks means "no pause", not "stop"
- Fix Spectrum tape loading and add Manic Miner regression
- Add Spectrum media runtime and beeper audio

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- Implements the `Peripheral` trait from `emu198x-bus-sinclair-zx-spectrum`,
  where it moved from the family's glue crate; the crate no longer depends on
  `common-sinclair-zx-spectrum`.
- First release under the `emu198x-peripheral-sinclair-zx-spectrum-beta-disk` name. Until now the crate was
  `beta-disk-interface`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/beta-disk-interface-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Add Pentagon 128 and Scorpion ZS-256 machines + beta-disk-interface

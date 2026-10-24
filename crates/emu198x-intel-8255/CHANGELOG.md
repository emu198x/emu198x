# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- First release under the `emu198x-intel-8255` name. Until now the crate was
  `intel-8255`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

### Changed

- The chip models its port directions instead of the MSX's wiring. It resets
  with every port an input (`$9B`), reads input bits from `input_a`,
  `input_b` and `input_c` live, reads output bits from their latches, accepts
  writes to port B, and no longer returns the mode word from the control
  register. `port_a()`, `port_b()` and `port_c()` give the lines; the public
  latch fields and the MSX-specific `keyboard_row()` are gone.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/intel-8255-v0.2.0) - 2026-06-04

### Added

- *(intel-8255)* port Intel 8255 PPI chip crate as MSX foundation

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0

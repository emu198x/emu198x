# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The MC6850 ACIA: control and status registers, the clock divider and word
  formats, transmit and receive framing timed from the host's clock input,
  overrun, and the interrupt request.
- First release, at 0.1.0, under the `emu198x-motorola-acia-6850` name. Until now the crate was
  `motorola-acia-6850`, versioned with the Emu198x suite and unpublished.

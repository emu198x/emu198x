# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- The `Ula`, `MemoryBus`, `Peripheral` and `MemoryIntercept` traits, and the
  `FrameTiming` descriptor with each Spectrum-family video configuration's
  frame data. They moved here unchanged from `common-sinclair-zx-spectrum`,
  the family's glue, which re-exports them, so that the ULAs and peripherals
  that implement them can be published without the glue.

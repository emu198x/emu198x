# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- The shared Spectrum-family ULA engine, `UlaEngine`, with its per-variant
  `UlaConfig`s, contention and delay tables, `snow_address`,
  `floating_bus_byte`, `FRAME_ROUTING_VERSION`, and the `attr_to_indices` and
  `attr_flash` attribute decoders. They moved here unchanged from the
  `ula_engine` and `palette` modules of `common-sinclair-zx-spectrum`, the
  family's glue, which re-exports them under those paths.

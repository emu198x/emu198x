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
- First release under the `emu198x-peripheral-kempston-sinclair-zx-spectrum-mouse` name. Until now the crate was
  `peripheral-kempston-mouse`, versioned with the Emu198x suite and unpublished.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The ST Shifter: the 16-entry palette, the resolution register, and the
  bitplane shift registers turning the MMU's video words into 320×200×16,
  640×200×4 and 640×400×2 pixels, one CPU cycle at a time.
- First release, at 0.1.0, under the `emu198x-atari-st-shifter` name. Until now the crate was
  `atari-st-shifter`, versioned with the Emu198x suite and unpublished.

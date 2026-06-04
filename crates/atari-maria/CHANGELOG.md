# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/atari-maria-v0.2.0) - 2026-06-04

### Added

- *(borders)* atari-maria — canonical TV-visible 384x288 frame
- extract Atari 7800 ProSystem from donor codebase

### Fixed

- *(7800)* rewrite the MARIA display-list parser to the hardware format
- *(7800)* correct MARIA CTRL bit map — games now boot past the black screen

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- clear pre-existing clippy lints in atari-pokey and atari-maria

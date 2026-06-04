# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/atari-tia-v0.2.0) - 2026-06-04

### Added

- *(borders)* atari-tia — render HBLANK as canonical TV border
- *(chips)* port mos-riot-6532 + atari-tia for Atari 2600 foundation

### Fixed

- Atari 2600 renders HBLANK black, not COLUBK

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean

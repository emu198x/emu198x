# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- The Atari 8-bit SIO bus at byte granularity: command frames, the
  acknowledge, complete and error replies, checksums, and disk drives that
  answer from numbered sectors with a write-protect tab.
- First release under the `emu198x-atari-sio` name. Until now the crate was
  `atari-sio`, versioned with the Emu198x suite and unpublished.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate now holds only the iNES / NES 2.0 file layout. The mapper trait
  and boards it used to carry moved to `nintendo-nes-cartridge`, which loads
  a board from what this crate parses.

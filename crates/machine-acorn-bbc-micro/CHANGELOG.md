# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-acorn-bbc-micro-v0.2.0) - 2026-06-04

### Added

- BBC Micro renders MODE 7 — model the SAA5050 teletext generator
- 6502 group on the shared debug tools (8 machines + Atari 800XL)
- *(bbc)* operational parity — runtime crate, MCP server, shell-backed script
- *(acorn-bbc-micro)* port machine + binary + Acorn OS v1.2 bank-scan live

### Fixed

- BBC Micro boots to BASIC — wire the keyboard to System VIA PA7
- *(6502)* move items before the test module (clippy items_after_test_module)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- clear remaining workspace clippy issues

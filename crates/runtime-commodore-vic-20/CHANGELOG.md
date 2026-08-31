# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- ESP-AT TCP links now emit `CLOSED` on peer loss and repeated 2400-baud
  configuration remains idempotent, matching reconnect behaviour on hardware

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-commodore-vic-20-v0.2.0) - 2026-06-04

### Added

- *(debug)* disassemble the 6502 family via the Asm198x isa_disasm spec crate
- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(vic-20)* operational parity — runtime crate, MCP server, shell-backed script

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* make the debug-target macros storage-agnostic
- rustfmt the workspace clean
- workspace clippy autofix sweep

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add a side-effect-free diagnostic snapshot covering every implemented CIA register, latch, timer, port, TOD, serial, control, and interrupt field.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/mos-cia-8520-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Add Amiga postcard snapshots across the chip stack
- fix workspace clippy and test hygiene
- land wb13 boot investigation and fixes
- Tighten CIA-8520 API: hide fields, fold duplication, name the bits
- Retire mos-cia-8520-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- CIA 6526 SP rate + 50/60Hz selector; 6502 BCD Oxyron flag semantics
- CIA 8520 8520-specific TOD halt + floppy /DSKRDY ID stream
- Correct Amiga CIA TOD alarm semantics and floppy status reporting
- Tighten Amiga floppy status and index handling
- Tighten Amiga floppy ready and CIA TOD behavior
- Add Amiga boot diagnostics and CIA TOD fix
- Tighten Amiga CIA and floppy boot path
- Add fresh Amiga headless baseline

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/peripheral-commodore-amiga-floppy-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Fix floppy spin-up regression that broke KS 2.04 disk loading
- Add Amiga postcard snapshots across the chip stack
- commit mechanical cleanup across diagnostics
- land wb13 boot investigation and fixes
- MFM encoder: rectify boundary clock bits + gap-fill post-track DMA
- Retire peripheral-commodore-amiga-floppy-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Rewrite Amiga MFM encoder as a direct port of vAmiga's algorithm
- Thread prev-bit state through the Amiga MFM encoder
- CIA 8520 8520-specific TOD halt + floppy /DSKRDY ID stream
- Correct Amiga CIA TOD alarm semantics and floppy status reporting
- Tighten Amiga floppy status and index handling
- Tighten Amiga floppy ready and CIA TOD behavior
- Add Amiga boot diagnostics and CIA TOD fix
- Tighten Amiga CIA and floppy boot path
- Add fresh Amiga headless baseline

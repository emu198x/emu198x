# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68040-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- Musashi corpora scaled 10x → 1000x; three real bugs caught
- Cpu68030 + Cpu68040 wrappers — variant pattern across the family
- 68030 + 68040 Phase-0 baselines
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000

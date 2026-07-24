# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Drive sprite blanking and control refetch from the edge-driven
  `VBSTRT`/`VBSTOP` state selected by `BEAMCON0.VARVBEN`
- Keep untouched programmable blank comparators unarmed when other
  vertical-timing registers are written

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-agnus-ecs-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-k: ECS blitter extension registers — WB content draws on ECS
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- Open Emu198x for public release
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
- Lift commodore-agnus-ecs and commodore-denise-ecs from archive

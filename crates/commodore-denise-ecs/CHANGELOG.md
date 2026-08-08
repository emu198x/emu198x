# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose the live `BPLCON3.EXTBLKEN` selector through a read-only accessor
- Expose serialized raw, in-flight and display-visible ECSENA/EXTBLKEN
  selector state

### Fixed

- Propagate BPLCON0.ECSENA and BPLCON3.EXTBLKEN through the normal
  three-half-CCK display path

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-denise-ecs-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- Open Emu198x for public release
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
- Lift commodore-agnus-ecs and commodore-denise-ecs from archive

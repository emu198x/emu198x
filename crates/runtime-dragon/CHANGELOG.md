# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-dragon-v0.2.0) - 2026-06-04

### Added

- *(dragon)* wire the 6809 debug target — first 6809 isa-disasm consumer

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* make the debug-target macros storage-agnostic
- Open Emu198x for public release
- Fix MC6809 stack instruction timing
- Add Dragon runtime snapshots
- Share DragonDOS directory entry lookup
- Cover Dragon runtime VDK directory export
- Add Dragon VDK sidecar export
- Fix DragonDOS DIR disk flow
- Add initial DragonDOS VDK disk support
- Smoke Dragon 64 BASIC after mode switch
- Add Dragon 64 firmware diagnostics
- Implement Dragon 64 ROM mode switching
- Add Dragon 64 cold-boot profile
- Boot Dragon BASIC before BIN autorun
- Add DragonDOS BIN program loading
- Add continuous Dragon gamepad axes
- Delay Dragon VDG display base updates
- Add source-backed 6809 timing checks
- Fix Dragon raster timing diagnostics
- Expose Dragon PAL overscan framebuffer
- Align Dragon VDG beam phase
- Fix Dragon SYNC wake handling
- Add Dragon cartridge and PAK snapshot media
- Add Dragon joystick hardware input
- Add Dragon PIA audio output
- Add Dragon VDG beam buffer
- Align Dragon VDG text rendering with XRoar
- Render Dragon VDG graphics modes
- Add Dragon video smoke instrumentation
- Add Dragon cassette compatibility matrix
- Add Dragon machine-code cassette smoke
- Implement Dragon cassette playback
- Fix Dragon keyboard matrix and host mapping
- Add Dragon runtime boot golden
- Add Dragon machine runtime and native shell

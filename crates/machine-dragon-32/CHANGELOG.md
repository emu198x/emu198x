# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-dragon-32-v0.2.0) - 2026-06-04

### Added

- *(dragon)* wire the 6809 debug target — first 6809 isa-disasm consumer

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- retire emu198x-script-* references after binary consolidation
- Open Emu198x for public release
- Model MC6809 external bus ownership pins
- Pin Dragon SAM and cassette timing guardrails
- Add Dragon runtime snapshots
- Add Dragon VDK sidecar export
- Model DragonDOS index and write-track status
- Support DragonDOS sector writes
- Fix DragonDOS DIR disk flow
- Add initial DragonDOS VDK disk support
- Implement Dragon 64 ROM mode switching
- Add Dragon 64 cold-boot profile
- Model Dragon SAM RAM paging
- Boot Dragon BASIC before BIN autorun
- Add DragonDOS BIN program loading
- Wire Dragon cartridge SND audio input
- Pin Dragon analogue mux behaviour
- Use source-backed Dragon VDG fetch timing
- Delay Dragon VDG display base updates
- Split Dragon stepping into CPU phase windows
- Drive Dragon CPU through 6809 phases
- Add repeatable Dragon trace watches
- Instrument Dragon master tick references
- Tighten Dragon VDG reference timing
- Improve Dragon raster CSS timing
- Improve Dragon timing diagnostics
- Fix Dragon frame sync and CSS latch timing
- Fix Dragon raster timing diagnostics
- Sync Dragon snapshot XRoar reference timing
- Add Dragon completed-frame screenshot capture
- Render Dragon VDG bytes incrementally
- Align Dragon VDG CSS pipeline
- Expose Dragon PAL overscan framebuffer
- Align Dragon VDG beam phase
- Model Dragon VDG CSS pipeline
- Add Dragon VDG timing diagnostics
- Improve Dragon cycle timing
- Fix Dragon empty cartridge slot mapping
- Fix Dragon XRoar snapshot references
- Model Dragon PIA signal edges
- Wire Dragon frame sync into PIA
- Add Dragon cartridge and PAK snapshot media
- Add Dragon joystick hardware input
- Validate Dragon audio levels
- Add Dragon PIA audio output
- Add Dragon VDG beam buffer
- Render Dragon VDG graphics modes
- Add Dragon video smoke instrumentation
- Implement Dragon cassette playback
- Fix Dragon keyboard matrix and host mapping
- Add Dragon machine runtime and native shell

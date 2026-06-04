# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-c64-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- consolidate C64 into one emu198x-c64 binary (UI/script/MCP)
- Open Emu198x for public release
- Seam 2 (NES + C64): host input → controller / joystick routing
- add native presentation filters
- migrate native windows to wgpu presenter
- add amiga joystick controls
- share native gamepad button mapping
- add native channel controls
- share native audio output
- preserve stereo in native audio conversion
- Add first C64 BASIC disk autoload path
- Mount D64 media into live 1541 path
- Attach live 1541 runtime to C64
- Standardize native shell tape hotkeys
- Add C64 native shell tape controls
- Step native shells in sub-frame slices
- Reduce native shell input latency
- Add C64 native verifier shell

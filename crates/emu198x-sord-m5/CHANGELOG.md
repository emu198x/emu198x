# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-sord-m5-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- Sord M5 boots through the Z80 CTC + MCP debug surface
- *(sord-m5)* operational parity — runtime crate, MCP server, shell-backed script
- *(emu198x-sord-m5)* headless runner + honestly-broken boot smoke

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- workspace clippy autofix sweep

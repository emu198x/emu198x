# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The picture: planes A and B and the window, fetched in their own slots, with every scroll mode, priority, the column-0 mask, shadow and highlight, interlace mode 2, and output to an ARGB framebuffer with borders, mid-line CRAM changes and CRAM dots.
- The VDP's bus side, stepped slot by slot: memories, command latch, registers, write FIFO, read prefetch, VRAM fill, VRAM copy and 68000 DMA, the H and V counters, status, and interrupts. Rendering is not yet modelled.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The ST GLUE and MMU: 50, 60 and 71 Hz line and frame timing, display
  enable, the HBL and VBL interrupts, the timer B input, the video base and
  counter with one fetch every four cycles, the memory configuration register
  with STF DRAM row/column mapping, and the DMA address counter.
- First release, at 0.1.0, under the `emu198x-atari-st-glue-mmu` name. Until now the crate was
  `atari-st-glue-mmu`, versioned with the Emu198x suite and unpublished.

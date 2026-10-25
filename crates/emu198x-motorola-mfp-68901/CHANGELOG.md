# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The MC68901 MFP: eight GPIO lines with edge interrupts, timers A–D in
  delay, event-count and pulse-width modes, the 16-channel vectored interrupt
  controller with software end-of-interrupt, and the USART at register level
  clocked from timer D's output.
- First release, at 0.1.0, under the `emu198x-motorola-mfp-68901` name. Until now the crate was
  `motorola-mfp-68901`, versioned with the Emu198x suite and unpublished.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Wd1770::set_byte_cycles` paces the data phase. Taking a byte drops `DRQ`
  until the next one arrives, and `INTRQ` follows the two CRC bytes, so a host
  on an edge-triggered interrupt such as the BBC B+'s NMI sees every edge.
  Zero, the default, keeps the old behaviour.
- `Disk::sides` reports how many sides a disk has.
- `Wd1770::reset` pulses the master-reset pin: the controller clears and runs
  a Restore, keeping its disks and drive selection.

### Changed

- The serialised state gains the byte time and the countdown to the next
  byte, so snapshots that hold a `Wd1770` change shape.

## 0.1.0

### Added

- First release under the `emu198x-western-digital-wd1770` name. Until now the crate was
  `western-digital-wd1770`, versioned with the Emu198x suite and unpublished.

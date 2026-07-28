# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add serde-persistable GVP A530 configuration and board-local state
- Add 1, 2, 4 and 8 MiB Zorro-II-discovered local-RAM functions
- Add byte, storage and MC68030-sized access through the 32-bit local port
- Record cache-enable and autoboot jumper settings without implementing
  cache, CPU or SCSI/controller behaviour
- Validate that persisted local-RAM backing and Autoconfig identity still
  match the immutable A530 configuration

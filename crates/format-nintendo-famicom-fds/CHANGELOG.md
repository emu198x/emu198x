# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The `.fds` layout: the optional 16-byte fwNES header, 65,500-byte disk
  sides, and the four block types on a side (disk info, file amount, file
  header and file data), read and written back.

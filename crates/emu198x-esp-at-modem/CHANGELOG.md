# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Extracted from `runtime-commodore-vic-20` so any machine with a bit-banged
  serial line can drive an ESP-AT modem, rather than the capability living
  inside one system's runtime (RULES.md #30). No behaviour change: the VIC-20
  completes the same end-to-end game against the same server, move for move.
- Published to crates.io. The crate has no dependencies and no machine-specific
  knowledge, so it stands alone.
- `is_connected` reports whether the emulated modem holds an open TCP
  connection, which a host needs to tell a stalled client from a dropped link.
- An optional `query` feature exposes `QUERY_LEAVES` and `query_leaf`, the
  diagnostic names this peripheral answers. A host runtime mounts them wherever
  it likes and advertises them only while the modem is plugged in, so the leaf
  names stay here rather than being restated in each machine. The feature is
  off by default; without it the crate still has no dependencies.

- Hayes dialing (`ATD`/`ATDT`/`ATDI`/`ATDP`, `ATZ`, `ATH`), as spoken by a
  Zimodem-based user-port WiFi modem, alongside the existing ESP-AT command
  set. The modem latches whichever dialect the computer uses first, so a client
  needs no configuration. After `CONNECT` a Hayes link goes transparent in both
  directions: no `AT+CIPSEND` framing on the way out, no `+IPD,` envelope on
  the way back, and `CONNECT` stands alone on its line because a Hayes client
  drains one response line and reads the next byte as data.
- `ModemDialect` and `dialect()` expose which dialect was latched.

### Changed

- `tick` takes `transmit` rather than `cb2`. The old name was a 6522 VIA pin,
  which leaked the first machine into an interface that never depended on it.

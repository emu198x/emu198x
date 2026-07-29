# Amiga Test Data

This directory contains executable and captured test data used to validate
Commodore Amiga machine implementations.

Its scope is machine-level media and evidence corpora. Chip implementation
tests remain beside their crates, while primary manuals and third-party
emulator sources remain in the umbrella reference layers.

Subdirectories should contain one focused corpus with its own purpose,
licensing, provenance, build instructions, and validation contract. Generated
or restricted artifacts must state whether they are tracked.

The first corpus is the emulator-neutral
[`programmable-hblank/`](programmable-hblank/) probe suite.

## Related directories

- [`../c64/`](../c64/) contains Commodore 64 test data.
- [`../../../crates/`](../../../crates/) contains the implementations that
  consume machine-level test data.

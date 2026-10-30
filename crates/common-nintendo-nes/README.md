# Shared NES wiring

The NTSC and PAL machine owners bind this engine to fixed boards. The raw
`Nes` type supports shared regression tests and emulator composition; it is
not itself a physical board evidence owner. Corrected v3 runtime snapshots
retain CPU open bus, PPU decay timestamps and output codes.

CPU, PPU, APU, DMA, cartridge mapping and input algorithms live here. The
physical owners supply construction and checked restore. Original core tests
exercise common behaviour; each owner has separate direct guest tests.

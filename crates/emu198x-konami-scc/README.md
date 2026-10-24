# emu198x-konami-scc

Konami's SCC wavetable sound chip, from the MSX MegaROM cartridges of
Nemesis 2, Salamander and Space Manbow, and its successor the SCC-I (sold
as the SCC+ in the Snatcher and SD Snatcher Sound Cartridges): five
channels, each playing a 32-byte signed waveform at a 12-bit frequency and a
4-bit volume, plus the deformation register that changes how the
frequencies are read and lets software rotate the waveforms.

The chip owns no bus and makes no callbacks: the cartridge mapper forwards
the reads and writes of its register window, and the machine steps it once
per input clock (3.58 MHz on the MSX) and reads its output.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. The register map, the output arithmetic,
the power-on state and the deformation register's behaviour are facts read
from openMSX (GPL-2.0), which records the measurements of Jon De Schrijder,
Manuel Pazos, NYYRIKKI and others, not code taken from it.

Licensed GPL-2.0-or-later.

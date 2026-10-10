# Synthetic VIA timer start probe

`via-timer-start.rom` is a generated 8 KiB test KERNAL, not Commodore firmware.
`via-timer-start.bin` contains its 68 RAM result bytes from native VICE xvic
3.10; PAL and NTSC agree byte for byte.

The first four bytes are T1/T2 low/high counter observations, with the low
read 105 cycles after the high-byte write. Then come 32 T1 and 32 T2 IFR
observations: initial count 0..7, then read delays W+4, W+6, W+8, W+10.
IFR values are masked to the relevant timer bit. The program terminates at
$E521 and writes results to $0200..$0243.

Generate and capture with the Emu198x docs repository's
`plans/2026-10-10-via-timer2-start/probe.py --output <new-directory>`.
That plan preserves the generator, reference hashes and native monitor logs.

# Synthetic VIA timer probes

## Timer start

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

## Timer 2 after timeout

`via-timer2-underflow.rom` is another generated 8 KiB test KERNAL.
`via-timer2-underflow.bin` contains its 96 RAM result bytes from native VICE
xvic 3.10; PAL and NTSC agree byte for byte. Neither ROM contains Commodore
firmware.

For initial counts 0, 1, 7, 255, 256 and 65535, the guest samples the timer
shortly after loading, after each of two full counter wraps, and after a
high-byte write rearms the interrupt. Each sample records IFR, counter low,
counter high and IFR after acknowledgement; IFR is masked to bit 5. Between
the long waits, a low-latch write and an IFR write must not rearm the timer.
The program terminates at $E407 and writes results to $0200..$025F.

Generate and capture with the Emu198x docs repository's
`plans/2026-10-10-via-timer2-underflow/probe.py --output <new-directory>`.
The accompanying plan retains the generator, reference hashes, native logs
and failing/passing regression evidence.

# Reference Captures

This directory is reserved for independently produced Paula-audio capture
packages.

A package is admissible only when it records the exact corpus artifact,
producer family and revision, complete machine configuration, firmware hash,
capture domain, filtering, resampling, raw capture hash, and semantic
measurements required by `../schema/capture-v1.schema.json`.

Reference emulators must be grouped by implementation family. Multiple
frontends around the same emulation core do not provide independent
agreement. Physical machines must record board revision, output connection,
capture interface, sample rate, and any calibration or level adjustment.

No capture is currently promoted as an expected result. An empty references
directory means that the corpus is runnable, not that Emu198x is conformant.

## Documentary basis

The register addresses, audio-channel routing, signed sample format, period,
volume, DMA gates, and `ADKCON.FAST` meaning were checked against the *Amiga
Hardware Reference Manual*, Third Edition. In particular, `FAST=1` selects the
normal 2 µs MFM bit-cell clock and `FAST=0` selects the 4 µs
GCR-compatible clock.

The portable-disk timing was cross-checked against vAmiga's asynchronous
`DiskController`, which derives one incoming byte from a 12,668-byte track at
300 RPM, and WinUAE's normal-floppy definition of seven colour clocks per
MFM bit. These implementations informed the controlled stimulus and exposed a
factor-of-two Emu198x regression. They do not define the expected audio
waveform.

No third-party reference text, emulator source, emulator binary, or firmware
is redistributed in this corpus.

## Related files

- [`../README.md`](../README.md) defines the capture contract.
- [`../schema/capture-v1.schema.json`](../schema/capture-v1.schema.json)
  defines one capture record.
- [`../../../../../knowledge/decisions/amiga-accuracy-closure-campaign.md`](../../../../../knowledge/decisions/amiga-accuracy-closure-campaign.md)
  defines the evidence required by the active accuracy campaign.
- [`../../../../../knowledge/processes/amiga-paula-audio-conformance.md`](../../../../../knowledge/processes/amiga-paula-audio-conformance.md)
  defines how captures are admitted and interpreted.

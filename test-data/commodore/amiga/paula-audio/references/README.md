# Reference Captures

This directory contains independently produced Paula-audio capture packages
that have passed the corpus's provenance and measurement checks.

A package is admissible only when it records the exact corpus artifact,
producer family and revision, complete machine configuration, firmware hash,
capture domain, filtering, resampling, raw capture hash, and semantic
measurements required by `../schema/capture-v1.schema.json`.

Reference emulators must be grouped by implementation family. Multiple
frontends around the same emulation core do not provide independent
agreement. Physical machines must record board revision, output connection,
capture interface, sample rate, and any calibration or level adjustment.

The registered
[`vamiga-4.4b12-60fd1e6b/`](vamiga-4.4b12-60fd1e6b/README.md) package records
vAmiga 4.4b12 at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. Its three source captures agree
with Emu198x on logical stereo routing and programmed cadence, and its
channel-0 half/full RMS ratio is 0.499952835 against Emu198x's 0.5.

This is one audited software implementation family. It is not physical
hardware evidence or a two-family software consensus. Exact RMS magnitude is
not compared across the producers because their filter, gain, and resampling
paths differ.

The neutral case definitions remain unresolved by design. Registered
observations are retained in producer packages and bound by consumers without
turning one implementation's samples into universal expected waveforms.

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
- [`vamiga-4.4b12-60fd1e6b/README.md`](vamiga-4.4b12-60fd1e6b/README.md)
  describes the registered vAmiga package and its evidence boundary.
- [`../schema/capture-v1.schema.json`](../schema/capture-v1.schema.json)
  defines one capture record.
- [`../../../../../knowledge/decisions/amiga-accuracy-closure-campaign.md`](../../../../../knowledge/decisions/amiga-accuracy-closure-campaign.md)
  defines the evidence required by the active accuracy campaign.
- [`../../../../../knowledge/processes/amiga-paula-audio-conformance.md`](../../../../../knowledge/processes/amiga-paula-audio-conformance.md)
  defines how captures are admitted and interpreted.

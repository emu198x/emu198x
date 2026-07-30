# Comparator Capabilities for Mid-Line HBLANK Writes

This document answers which audited implementations can produce admissible
observations for the write-timing corpus.

## FS-UAE 5.0.7

FS-UAE revision `f362278ccd4c60991caac3b4d240d4a3f751bea2` accepts all five
stimuli on both registered ECS and AGA profiles. Its underlying chipset core
identifies itself as WinUAE 6.0.1-derived, so FS-UAE and WinUAE are one UAE
implementation family.

The registered capture hook retains the UAE chipset framebuffer before
frontend cropping or processing. This producer is admissible as
software-derived evidence.

## Copperline 0.13.0

Copperline revision `eec5806287fc880a5463ece900d793f250705efc` stores the
programmable registers but applies horizontal blanking as a whole-frame
post-process from their final values. A mid-line write would therefore affect
pixels rendered before the write.

That path cannot preserve beam-ordered mutation semantics. Copperline is
unsupported for this corpus rather than a behavioural disagreement.

## vAmiga 4.4b12

vAmiga revision `60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` does not dispatch
`HBSTRT` or `HBSTOP` writes and renders a fixed horizontal-blank interval.
Its `BEAMCON0` and `BPLCON3` paths do not implement the selectors required by
these cases.

vAmiga is unsupported for this corpus rather than a behavioural
disagreement.

## Evidence status

The current matrix has one admissible software implementation family and no
physical-hardware capture. Its observations are `single-family` evidence.
They may guide an explicit Emu198x implementation choice, but they do not
establish historical hardware behaviour by consensus.

## Related documents

- [UAE event-model source audit](uae-event-model-source-audit.md)
- [Steady-state comparator audit](../../programmable-hblank/references/comparator-capabilities.md)
- [Corpus overview](../README.md)

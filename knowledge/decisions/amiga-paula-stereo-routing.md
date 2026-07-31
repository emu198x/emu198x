# Decision: Amiga Paula stereo routing

**Date:** 2026-07-31
**Status:** BINDING

## The question

Which Paula audio channels contribute to the Amiga's left and right stereo
outputs?

## Decision

Paula channels 1 and 2 contribute to the left output. Channels 0 and 3
contribute to the right output.

The component mixer, runtime audio consumer, diagnostics, tests, and
conformance cases must use that assignment. A producer may describe its
outputs with a different buffer order only when it records the conversion
explicitly; it must not silently remap channels to make observations agree.

## Evidence

The third-edition *Amiga Hardware Reference Manual*, Audio Hardware chapter,
states that channels 1 and 2 are connected to the left-side stereo output jack
and channels 0 and 3 to the right-side jack.

An independent vAmiga 4.4b12 capture of the portable Paula-audio corpus agrees:
the channel-0 cases appear only on the right output and the channel-1 case only
on the left output when vAmiga's hard-stereo configuration is retained.

Emu198x previously implemented the reverse assignment. The corpus had tested
that implementation consistently but had no independent expected result. The
vAmiga comparison exposed the disagreement, and the primary manual resolved
it without treating either implementation as authoritative.

## Consequences

The mixer now combines channels 1 and 2 for its left sample and channels 0 and
3 for its right sample. Directed tests exercise all four channels. The
machine-level corpus retains its cadence, equal-channel, and volume-ratio
checks while asserting the corrected output assignment.

This decision settles logical stereo routing. It does not establish the
analogue transfer function between Paula and a particular motherboard's
physical output, nor crosstalk, noise, clipping, DC offset, or component
tolerances. Those claims still require physical measurements.

## Related documents

- [Amiga Paula-audio conformance](../processes/amiga-paula-audio-conformance.md)
- [Portable Paula-audio corpus](../../test-data/commodore/amiga/paula-audio/README.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)

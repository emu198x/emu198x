# Verifying Paula audio waveforms

This process answers how Paula audio captures are produced, compared, and
admitted as accuracy evidence.

## Evidence boundary

The first slice measures a steady repeating waveform from one audio channel at
a time. It asks whether the configured sample cadence, stereo routing, and
volume relationship survive the complete machine, DMA, mixer, filter, and
capture path.

It does not certify:

- Paula's exact analogue transfer function;
- a motherboard revision's component tolerances or noise floor;
- interpolation or host-resampler identity;
- clipping, distortion, crosstalk, or DC offset;
- minimum-period and period-write edge cases;
- attached period or volume modulation; or
- physical hardware without a registered line-output capture.

Those require separate probes and narrower assertions.

## Portable stimulus

The portable corpus is
[`test-data/commodore/amiga/paula-audio/`](../../test-data/commodore/amiga/paula-audio/).
It contains project-authored CC0-1.0 source, case metadata, deterministic ADF
build tools, and evidence schemas. It contains no firmware, third-party
software, expected waveform, or Emu198x runner.

Each case boots through Kickstart, disables unrelated DMA and interrupts,
disables the switchable LED filter, and loops the signed sample word `0x7f81`
through one Paula channel. The programmed period is 512 colour clocks. The
resulting fundamental is approximately 3.464 kHz on a PAL machine, low enough
to remain measurable after ordinary motherboard filtering and 44.1 or 48 kHz
capture.

The `PAUD` record at `0x0002ff00` identifies the case, register values, sample
buffer, and elapsed fields. Producers capture only after the field counter
reaches eight.

## Comparison method

Raw sample equality is not an admissible cross-producer assertion. Different
emulators and physical capture chains may apply declared filters, resampling,
gain, and phase choices while retaining the same Paula behaviour.

Each recording is reduced to:

- dominant-channel fundamental frequency;
- left and right AC RMS levels;
- channel-dominance ratio; and
- amplitude ratio against a declared paired case from the same producer and
  machine configuration.

The analysis window must be recorded. Automatic gain control, noise
suppression, channel remapping, and time stretching are prohibited. Filtering
and resampling are retained and described in the capture record.

A volume ratio may compare two files only when producer revision, machine,
firmware, capture domain, sample rate, filtering, resampling, and analysis
procedure are identical.

## Reference admissibility

A reference package records the exact ADF and payload hashes, producer family
and revision, full machine and firmware identity, capture domain, unmodified
recording hash, and semantic observations required by the capture schema.

WinUAE and FS-UAE are one implementation family. vAmiga is independent.
Physical machines identify model, board revision, output connection, capture
interface, sample rate, and any calibration or level adjustment. Agreement
between two software families is useful comparator evidence but is not a
physical-hardware claim.

Cases begin unresolved. A stable observation from one audited implementation
family is single-family evidence. Agreement across independent families may
support a software consensus. Hardware status requires a registered physical
capture.

## Emu198x consumer

[`scripts/verify-amiga-paula-audio.sh`](../../scripts/verify-amiga-paula-audio.sh)
rebuilds the corpus, verifies Kickstart 1.3 and every artifact by SHA-256,
boots each case on the A500 OCS PAL profile, waits for the ready record,
discards boot audio, and measures three settled fields.

The current self-consistency gate observes:

| Case | Dominant output | AC RMS | Fundamental |
| --- | --- | ---: | ---: |
| `channel-0-full` | right | 0.354971423 | 3463.398 Hz |
| `channel-1-full` | left | 0.354971299 | 3463.398 Hz |
| `channel-0-half` | right | 0.177485636 | 3463.398 Hz |

The inactive output is exactly silent in all three current captures. The
half/full RMS ratio is 0.5 within the recorded precision.

These are Emu198x observations, not independent expectations. The gate proves
that the portable probe executes and that Emu198x remains internally
consistent. Step 3 of the Amiga accuracy closure campaign remains open until
an independent producer records and agrees or a disagreement is classified.

## Regressions found by the corpus

### Disk stream timing

The first current-source run failed before the audio probe executed. The
rotational disk path was delivering one complete MFM word every 56 PAL colour
clocks while `ADKCON.FAST` was set. The hardware definition makes 56 colour
clocks the encoded-byte interval at the normal 2 µs MFM bit-cell rate; a word
takes 112.

The doubled stream exceeded Agnus's three disk-DMA cells per raster line,
overflowed Paula's three-word FIFO, and corrupted Kickstart track reads.
Correcting the byte/word distinction restored both the pre-existing HBLANK
boot path and this audio corpus. Directed component and machine tests retain
the corrected `FAST` interpretation.

### Stereo output assignment

The first independent vAmiga capture disagreed with Emu198x about which output
carried each channel. The third-edition *Amiga Hardware Reference Manual*
resolved the disagreement: channels 1 and 2 reach the left output, while
channels 0 and 3 reach the right output. Emu198x had implemented the reverse
assignment.

The component mixer, all four directed channel tests, and the machine-level
consumer now use the hardware assignment. The full gate observes channel 0 on
the right, channel 1 on the left, and an unchanged half/full RMS ratio of 0.5.

This evidence resolves the logical jack assignment without a physical
machine. It does not resolve the analogue transfer, noise, crosstalk, clipping,
or motherboard-component behaviour excluded above.

## Related documents

- [Portable Paula-audio corpus](../../test-data/commodore/amiga/paula-audio/README.md)
- [Amiga accuracy closure campaign](../decisions/amiga-accuracy-closure-campaign.md)
- [Amiga Paula stereo routing](../decisions/amiga-paula-stereo-routing.md)
- [Amiga disk rotation and DMA arbitration](../decisions/amiga-disk-dma-fifo-arbitration.md)
- [Amiga programmable-HBLANK conformance](amiga-programmable-hblank-conformance.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)

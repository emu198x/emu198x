# Verifying Amiga programmable horizontal blanking

This process answers how programmable horizontal blanking is tested and how
the resulting evidence may be interpreted.

The directed probe is intended to be useful outside Emu198x. Its machine code,
case definitions, build tools, schemas, and reference packages form a neutral
corpus. Emu198x-specific launch code and assertions remain separate consumers.

## Scope

The first conformance slice covers the horizontal blanking path selected by
`ECSENA`, `EXTBLKEN`, and `BLANKEN` on standard PAL lines. It includes fixed
blanking controls, central and wrapping programmed windows, equal comparator
values, and AGA fine-position inputs at lores, hires, and superhires
resolutions.

The first slice does not certify:

- programmable vertical blanking or sync;
- variable line or field totals;
- monitor-driver geometry;
- analogue RGB, composite, genlock, or sync-pin levels;
- sub-pixel AGA placement that the captured sample grid cannot represent;
- register writes made after a relevant comparator position on the current
  line;
- physical hardware behaviour without a registered hardware capture.

Those questions require separate cases and evidence. Mid-line register writes
are covered by the separate
[`write-timing process`](amiga-programmable-hblank-write-timing.md).

## Portable corpus boundary

The portable corpus is
[`test-data/commodore/amiga/programmable-hblank/`](../../test-data/commodore/amiga/programmable-hblank/).
It contains only project-authored CC0-1.0 material:

- 68000 probe and boot sources;
- declarative case inputs;
- deterministic ADF build tools;
- suite and capture schemas;
- reference-package metadata and independently produced captures.

It contains no Kickstart image, Workbench component, AmigaOS monitor driver,
third-party emulator code, or Emu198x expected image. Each generated ADF is one
static case, so a producer does not need to automate a menu or keyboard.

Emu198x adapters belong in runtime tests and scripts outside the corpus.
Producer-specific capture patches and runners also remain outside the CC0
subtree under their applicable software licence; a reference package binds
them by revision and SHA-256. This separation allows the corpus subtree to be
published or extracted without carrying implementation code into another
emulator's test harness.

## Build contract

The builder validates the case file, assembles the boot block and one payload
per case, creates an exact 901,120-byte ADF, verifies the Amiga boot-block
checksum, and emits a versioned suite manifest plus sorted SHA-256 sums.

A release candidate is reproducible only when two clean builds using the
declared toolchain produce identical ADF and payload hashes. A generated
artifact is not a reference result: it is the stimulus whose identity every
capture must record.

Commercial firmware remains external. A capture manifest records its revision
and SHA-256 without copying its bytes.

## Reference admissibility

A reference package is admissible when it records:

- exact suite, case, ADF, and payload identities;
- producer, version, revision, and implementation family;
- complete machine, chipset, region, memory, and firmware configuration;
- cold-boot and settling procedure;
- raw capture method and unfiltered pixel geometry;
- explicit coordinate normalisation without alignment search;
- source-file, decoded-pixel, and canonical-image hashes;
- observed blank edges, ordering, wrap behaviour, and uncertainty.

Frontend screenshots that crop blanking, scale the image, apply a shader, or
omit their source geometry are diagnostic material, not canonical references.
An Emu198x-produced image may be retained below `target/accuracy/` for
diagnosis, but it cannot become an independent expected result.

## Comparator roles

WinUAE and FS-UAE count as one UAE implementation family. FS-UAE remains useful
as an executable cross-platform route to that family, but agreement between the
two is not an independent vote.

The registered FS-UAE 5.0.7 package captures the current WinUAE 6.0.1-derived
path before frontend processing. Its host-HIRES raster represents AGA fine
positions at paired superhires phases. It establishes a UAE-family observation
but does not establish physical hardware behaviour.

Copperline is an independent software implementation with OCS, ECS, and AGA
profiles. Its result supplies a second AGA comparison when the exact revision
and configuration are recorded. It is not physical-hardware evidence.

vAmiga is an independent OCS/ECS control producer. A missing programmable
HBLANK path is recorded as unsupported, not as a behavioural disagreement.

Minimig, MAME, and other implementations may supply diagnostic comparisons
only after the relevant path and capture geometry have been audited. No
implementation is promoted to ground truth solely because it is written in
hardware-description language or has broad system coverage.

## Promoting an expected observation

Each case begins with `expected.status` set to `unresolved`. Status changes are
evidence classifications, not majority votes:

- `single-family` means one audited implementation family produced a stable
  observation;
- `consensus` means at least two independent audited families agreed after the
  declared normalisation;
- `hardware` means a registered physical-hardware capture supports the
  observation.

A disagreement remains visible in the reference packages and leaves the
expected observation unresolved. It must not be averaged away or converted
into a tolerance.

Only semantic observations are promoted: selected blank source, first blank
sample, first non-blank sample, comparator ordering, and carried state. An
entire emulator-produced frame is not the specification.

## Emu198x conformance lane

The Emu198x adapter must verify the corpus manifest and artifact hashes before
booting a case. It must run the declared machine profile, wait in emulated
fields, capture three adjacent settled fields, and confirm that a static case
is stable.

Assertions consume semantic observations on which the registered UAE and
Copperline implementation families agree. The current CCK-aligned gate checks
the fixed-control, programmed-central, programmed-wrap, and programmed-equal
cases on ECS and AGA, plus the ECS `BLANKEN`-clear case. `ECSENA`,
`EXTBLKEN`, and AGA `BLANKEN` remain measurement-only disagreements. This
consumer-side table does not modify the neutral corpus or turn either emulator
family into hardware truth.

A failure records the Emu198x frame, edge measurements, relevant
custom-register writes, and comparison details below
`target/accuracy/amiga-programmable-hblank/`. There is no golden-update mode.

CCK-aligned behaviour is the first implementation gate. AGA 70 ns and 35 ns
placement requires a capture or trace grid capable of representing those
positions; passing a coarser framebuffer comparison must not be described as
proof of finer timing. The registered UAE host-HIRES captures exercise the
three fine-position cases but cannot distinguish the final 35 ns phase bit.
Emu198x represents paired Lisa fine phases in its four-sample-per-CCK renderer,
but the current portable integration gate deliberately collapses horizontally
duplicated pairs and does not claim fine-position conformance.

## Monitor-mode integration

The AmigaOS monitor binaries exercise authentic register traffic and remain a
separate integration lane. DblPAL, DblNTSC, Multiscan, Euro36, Euro72, and
related modes can establish that Emu198x accepts and runs real programming
sequences.

They are not directed endpoint or ordering oracles. Their execution does not
replace the portable cases, and a standard fixed-size framebuffer cannot by
itself certify their complete monitor geometry.

## Result interpretation

A passing case establishes only that Emu198x matched the asserted cross-family
consensus observations for the declared corpus version, artifact, machine
profile, firmware identity, capture grid, and field rule.

It does not establish general Amiga video accuracy, another chipset revision,
another region, untested register-write timing, analogue output, or physical
hardware agreement beyond any hardware capture explicitly registered for that
case.

## Related documents

- [Portable programmable-HBLANK corpus](../../test-data/commodore/amiga/programmable-hblank/README.md)
- [Programmable-HBLANK write timing](amiga-programmable-hblank-write-timing.md)
- [Current UAE-family capture](../../test-data/commodore/amiga/programmable-hblank/references/fs-uae-5.0.7-f362278c/README.md)
- [Amiga Test Kit v1.21 video conformance](amiga-test-kit-video-conformance.md)
- [Amiga Test Kit verification](amiga-test-kit-verification.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Test ROM bundling policy](../decisions/test-rom-policy.md)

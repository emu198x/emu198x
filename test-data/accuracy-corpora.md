# Accuracy corpora — fixtures manifest

Single source of truth for the external CPU test corpora that the
[`nightly-accuracy`](../.github/workflows/nightly-accuracy.yml) workflow runs.
Each corpus has item-specific access and redistribution terms. The nightly does
**not** fetch from the upstream locations directly — it pulls from a **mirror
you control** (see § The mirror), so configured runs are hermetic and
upstream-independent. A private mirror controls access; it does not establish
permission to copy an upstream corpus.

`scripts/check-fixtures.sh` reads this table to report which corpora are present
locally; the workflow uses the same env-var contract.

## Corpora

| Corpus | Crate · test file | Env var (points at the extracted dir) | Upstream source | Licence | Needs firmware? |
|--------|-------------------|----------------------------------------|-----------------|---------|-----------------|
| Tom Harte 6502 | `mos-6502` · `single_step_tests` | `EMU198X_6502_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`6502/v1`) | MIT-like (see repo) | no |
| Tom Harte Z80 | `zilog-z80` · `single_step_tests` | `EMU198X_Z80_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`z80/v1`) | MIT-like | no |
| SingleStepTests 68000 | `motorola-68000` · `tom_harte` | `EMU198X_68000_TOM_HARTE_ROOT` | github.com/SingleStepTests/680x0 (`68000/v1`) | unknown; no tracked licence at `e0d5ece` | no |
| SM83 (Tennant) | `sharp-lr35902` · `single_step_tests` | `EMU198X_SM83_TENNANT_DIR` | github.com/adtennant/sm83-test-data | see repo | no |
| Klaus Dormann 6502 | `mos-6502` · `dormann_tests` | `EMU198X_6502_DORMANN_DIR` | github.com/Klaus2m5/6502_65C02_functional_tests | GPL-3.0 | no |
| FUSE Z80 | `zilog-z80` · `z80_fuse` | `EMU198X_FUSE_Z80_TESTS_DIR` | FUSE emulator (`fuse-emulator-fuse/z80/tests`) | GPL-2.0-or-later | no |
| Wolfgang Lorenz 6502 | `mos-6502` · `lorenz_tests` | `EMU198X_6502_LORENZ_DIR` | Wolfgang Lorenz C64 test suite (via VICE `bin/`) | freeware | no — uses a synthetic free KERNAL |
| ZEXDOC + ZEXALL | `zilog-z80` · `zex_tests` | `EMU198X_ZEX_DIR` | Frank Cringle Z80 exerciser (`*.com`) | freeware | no |
| Spectrum system tests | `machine-sinclair-zx-spectrum-48k` · `float_bus`, `tape_smoke`; `machine-sinclair-zx-spectrum-128k` · `float_bus` | `EMU198X_SPECTRUM_SYSTEM_TESTS_DIR` (tapes) — the Spectron screens are **checked in**, see below | tapes are third-party programs Spectron bundles — RAMSOFT floatspy v0.33 and Woody's Float48k/Float128k | tapes are long-circulated freeware, not covered by Spectron's licence, redistributed in the **private** store only | 48K Spectrum ROM — reuses the one in the `z80test` tarball |
| C-BIOS (MSX) | `machine-msx` · `cbios_boot` | `EMU198X_ROMS_ROOT` (joins `microsoft-msx/`) | github.com/cbios/cbios, built with Pasmo | BSD — redistribution in binary form permitted, notice ships beside the ROMs | is firmware — a clean-room MSX BIOS, not Microsoft's |
| AROS m68k (Amiga) | `machine-commodore-amiga-ocs` · `aros_boot` | `EMU198X_ROMS_ROOT` (joins `commodore-amiga/`) | Copperline's build of AROS master + two upstream PRs | AROS Public License 1.1 — redistribution permitted; licence and build notes ship beside the ROMs | is firmware — a reimplemented AmigaOS, not Commodore's |
| AltirraOS (800XL) | `machine-atari-800xl` · `altirraos_boot` | `EMU198X_ROMS_ROOT` (joins `atari-800xl/`) | Avery Lee's XL/XE OS + Altirra BASIC, via atari800's vendored copy | all-permissive notice of its own — **not** the emulator's GPLv2; notice ships beside the ROMs | is firmware — a reimplemented Atari OS, not Atari's |
| Open ROMs (C64) | `runtime-commodore-c64` · `openroms_boot` | `EMU198X_ROMS_ROOT` (joins `commodore-c64/`) | github.com/MEGA65/open-roms, prebuilt `bin/` images | GPL-3.0 / LGPL-3.0 — redistribution permitted, licence texts and a source pointer ship beside the ROMs | is firmware — a clean-room C64 BASIC and KERNAL, not Commodore's |
| Spectrum ROMs (128K, +2, +3) | `machine-sinclair-zx-spectrum-128k` · `boot_test`; `-plus2`, `-plus2a`, `-plus2b`, `-plus3` · `boot_test` | `EMU198X_ROMS_ROOT` (firmware root; each machine joins its own directory onto it) | the machines' own firmware | free to distribute (Amstrad), the same permission the 48K ROM ships under | is firmware — these are the ROMs |
| z80test | `machine-sinclair-zx-spectrum-48k` · `z80test` | `EMU198X_Z80TEST_DIR` (+ `EMU198X_SPECTRUM_48K_ROM`) | raxoft/z80test (`*.tap`) | MIT | 48K Spectrum ROM — free (Amstrad), shipped in the tarball |

## The Spectrum line boots its own firmware

Almost every machine here needs its manufacturer's ROM to reach a prompt, and
cannot be checked in public CI because that ROM cannot be distributed. The
Spectrum is the exception: Amstrad permits its ROMs to be distributed, which
is why the 48K ROM already travels inside the `z80test` tarball and why
`spectrum-roms.tar.zst` can carry the 128K, +2 and +3 sets.

**The permission covers the Spectrum, and stops there.** It reads as a
statement about "the Sinclair and Amstrad line", which invites two
extensions it does not support:

- **The ZX80 and ZX81 are outside it.** Amstrad bought the rights to the
  Spectrum 48/128 and built the `+` machines; it never held the ZX80/ZX81
  copyrights, which stayed with Nine Tiles. Amstrad's own statement of the
  permission excludes them by name. Debian's review of this same permission
  draws the line in the same place: its `spectrum-roms` package covers 48K,
  128K, +2, +3 and TC2048, and no earlier machine.
- **The CPC needs a second permission.** Amstrad's grant extends to the CPC
  ROMs, but parts of that firmware are Locomotive Software's, whose terms
  are their own and stricter. A CPC ROM is one image containing both, so it
  cannot be split into the covered half.

Neither is a claim that those ROMs may not be used — only that *this*
permission is not what makes it so, and no other has been established here.
Route them through synthetic firmware, which needs no permission at all,
unless and until a specific grant is obtained and recorded above.

Sourced from Amstrad's statement of the permission and from Debian's
independent review of it. Read both directly before citing either: this
paragraph was written from summaries, and the wording of a rights grant is
exactly the place not to trust one.

The mirror is still private, and the manifest's rule still holds — a private
mirror controls access, it does not establish permission. The permission is
what makes these redistributable; the mirror is only how they are served.

`EMU198X_ROMS_ROOT` is the firmware root each machine's test joins its own
directory onto (`sinclair-zx-spectrum-128k/`, `amstrad-zx-spectrum-plus3/`,
and so on). One variable, one staging directory, every machine finding its
own ROMs — rather than a variable per machine.

## C-BIOS: real firmware nobody needs permission for

The MSX has a second route. [C-BIOS](https://github.com/cbios/cbios) is a
clean-room MSX BIOS under a BSD licence, written so that MSX emulators can
ship without a manufacturer's ROM — exactly the problem this manifest is
otherwise negotiating one machine at a time.

`cbios.tar.zst` carries `cbios_main_msx1.rom` and `cbios_logo_msx1.rom`
built from source with Pasmo, together with the licence text and a
provenance note. The BSD terms require the notice to travel with the
binaries, so it does.

It is **not** Microsoft's BIOS, and a title leaning on undocumented BIOS
internals may behave differently. For "does this machine start", that does
not matter — and it is one of three machines outside the Spectrum whose boot
evidence is a real firmware cold start rather than a synthetic stand-in. The
others are the C64 and the 800XL, below.

## The C64 boots Open ROMs

The same move as C-BIOS, on the machine where the licence problem is
sharpest: Commodore's BASIC, KERNAL and character ROMs cannot be
distributed, and every C64 waypoint this project has depends on them.
[Open ROMs](https://github.com/MEGA65/open-roms) is a clean-room BASIC and
KERNAL written against the documented `$FF81` jump table and the published
VIC-II/CIA registers, released under the GPL so emulators can ship legal
firmware.

`openroms-c64.tar.zst` carries the three images the C64 profile requires —
8 KiB BASIC, 8 KiB KERNAL, 4 KiB character generator — with the GPL and
LGPL texts and a provenance note.

These are the prebuilt images committed upstream in `bin/`, copied rather
than rebuilt, and the note records the commit that last produced *each*
one rather than the repository's HEAD:

| Image | Commit | Date |
| --- | --- | --- |
| `basic_generic.rom` | `5192c683a098` | 2021-08-23 |
| `kernal_generic.rom` | `5192c683a098` | 2021-08-23 |
| `chargen_openroms.rom` | `b96618115794` | 2020-03-06 |

The pairing was checked rather than assumed: the banner these print on boot
reads `RELEASE DEV.210823.FC.1`, which matches the BASIC/KERNAL commit date.
HEAD was recorded first and was wrong — the binaries in `bin/` are years
older than the branch they sit on.

The GPL wants source to accompany a binary or a written offer for it. The
source is the upstream repository at those commits, and the note says so
and says what to do if that ever stops being reachable.

It is **not** Commodore's KERNAL. Open ROMs says plainly that it is
incomplete, and software reaching past the documented interface may behave
differently — so this establishes that the machine starts, and nothing
about compatibility. What it renders is the full banner, the sized-RAM
count and the `READY.` prompt, and the test asserts all three: a machine
that hung shows none of them.

## The 800XL boots AltirraOS

[AltirraOS](https://www.virtualdub.org/altirra.html) is Avery Lee's
reimplementation of the XL/XE operating system, written so his emulator
needs no Atari ROM. Altirra BASIC stands in for Atari BASIC beside it.

**The licence is not the one the project page states.** Altirra the emulator
is GPLv2, and three separate places — the project page, the repository root
and the kernel directory — say only that. The kernel ROM carries its own
notice, in `src/Kernel/source/main.xasm`:

> Copying and distribution of this file, with or without modification, are
> permitted in any medium without royalty provided the copyright notice and
> this notice are preserved. This file is offered as-is, without any
> warranty.

That is all-permissive, and considerably easier than the GPL's
source-availability condition. It was found by reading the source file
headers after concluding the opposite from the project page — the lesson
being that a project's stated licence need not govern every artefact it
produces.

`altirraos-800xl.tar.zst` carries `altirraos_xl.rom` (16 KiB) and
`altirra_basic.rom` (8 KiB), the notice, and a provenance note.

**The chain is second-hand and the note says so.** AltirraOS ships embedded
inside `Altirra.exe`; no ROM image exists as a file in either the binary or
the source archive, and building it needs `atcompiler.exe`, a Windows-only
assembler built as part of Altirra. So it cannot be rebuilt here the way
C-BIOS was. The bytes come instead from the atari800 emulator, which has
vendored both ROMs as C arrays for years — parsed to bytes, lengths checked
against what the 800XL profile requires, nothing else changed. If a
first-hand route appears, prefer it.

atari800's header comments say kernel 3.11 and BASIC 1.58; the BASIC ROM's
own banner prints `Altirra 8K BASIC 1.59`, so the comment is stale against
the array beside it. The banner is what the machine reports and is the
version to quote.

The test asserts the banner and the `Ready` prompt by decoding the text
window, not by counting pixels. Pixel counting was tried first and was
actively misleading: at 300 frames both AltirraOS and Atari's own ROM
produce an identical histogram — a blue field and 64 pixels of cursor — which
reads as a hung machine, and is really a prompt that has not been reached
yet. It arrives between 200 and 400 frames; the test runs 600.

Decoding needs the Atari's *internal* character codes rather than ATASCII —
`$00-$3F` are ATASCII `$20-$5F`. Applying a C64-style screen-code mapping
renders a perfectly good boot screen as line noise, which is how the first
reading of it was misdiagnosed.

## The Amiga boots AROS — nightly, not per-PR

Commodore's Kickstart cannot be distributed, and until #1022 the Amiga could
not use the free alternative either: **AROS m68k spans two ROM windows** and
this emulator decoded one. `aros-amiga-m68k-rom.bin` at `$F80000` and
`aros-amiga-m68k-ext.bin` at `$E00000`, 512 KiB each. With only the first,
half an operating system runs and behaves like one.

With both, it boots: 15 distinct colours over 106 rows, against 2 colours and
a flat field. Both states are asserted, the second as the control.

**This one runs nightly.** 1500 frames of a 68000 costs ~140s in a debug
build against 8s in release, and the per-PR firmware-boot job is deliberately
under ten seconds in total. So the Amiga's per-push evidence remains
synthetic and its real-firmware evidence is nightly. That is a genuine
difference in what is checked how often, and it should not be flattened when
the evidence is reported.

**The build is patched, and the note says so.** These are not an AROS
release: Copperline built them from upstream master `d0370bd757` on
2026-07-28 with two not-yet-merged pull requests applied — 876, an NTSC boot
fix, and 878, an input-event-loss fix. Both are named, dated and upstream
rather than private, but the bytes are not reproducible from a single
upstream commit, which is worth knowing before a behavioural difference is
blamed on AROS. The chain is Copperline's, not the AROS project's; prefer a
release carrying these fixes if one appears.

AROS is a reimplementation of the AmigaOS API and is documented as less
compatible than a real Kickstart. It establishes that the machine starts and
renders, and nothing else — the Kickstart-backed tests are unchanged.

## The one exception: Spectron's reference screens

Every corpus above comes from the private mirror. Spectron's expected-output
screens do not — they are checked in at
[`spectrum/spectron-results/`](spectrum/spectron-results/), because they are
MIT-licensed (© 2026 Wojciech Sobieszek), small (116 KB), and because keeping
them out of the tree cost more than it saved.

`assert_screen_matches_spectron` skipped when it could not find them, and
`emu198x-test-skip` reports a skip as `ok` unless `EMU198X_STRICT_FIXTURES` is
set. No developer machine had the screens, so the comparison silently did
nothing locally while the nightly — which provisions them and does set that
variable — failed every night from 2026-08-11. The 48K floating-bus regression
(#939) was reproducible on any machine the whole time; nothing local would say
so. `EMU198X_SPECTRON_RESULTS_DIR` still overrides, so the nightly keeps using
its own bundle.

The *tapes* stay in the private store: they are freeware, not Spectron's to
license, and far larger.

The SingleStepTests 68000 fixture bytes are pinned by
[`singlesteptests-680x0-e0d5ece.sha256`](singlesteptests-680x0-e0d5ece.sha256).
The manifest contains one SHA-256 for each of the 124 compressed fixtures in
registered revision `e0d5ece9670205cc84a0101081837deb446f86a3`. The nightly
checks this manifest after extraction in addition to checking the mirror's
tarball checksum. It covers the fixture inputs consumed by the harness, not
the repository README files or opcode map.

**ZEX and z80test moved here 2026-07-04** for consistency — every external
corpus now runs from this one nightly. ZEX previously ran from checked-in
binaries via a dedicated `zex.yml` workflow, which was retired when the binaries
were removed. z80test runs Patrik Rak's exerciser on a full 48K Spectrum, so its
tarball also carries the free Amstrad-permissioned 48K ROM.

### `spectrum-system-tests` layout

The `floating-bus` nightly job expects this shape inside the tarball:

```text
spectrum-system-tests/
  tapes/              # Float48k.tap, Float128k.tap, floatspy.tap
  spectron-results/   # floatspy_48.png, floatspy_128.png
```

Both come from the vendored Spectron checkout at
`emulators/zx-spectrum/Spectron/tests/` in the umbrella tree — the tapes
from `Spectron.Integration.Tests/TestFiles/`, the screens from `Results/`.
They live in the umbrella rather than this repo, which is why CI needs
them staged into the corpora store.

**Why this corpus is worth the setup.** These are the strongest
floating-bus oracles in the tree, and until 2026-08-10 none of them ran
in CI. A ULA contention fix landed that improved the timing survey,
passed the catalogue and passed every unit test, while silently
regressing the floating bus — floatspy caught it, but only because
someone ran it by hand for an unrelated reason. The job asserts its
inputs exist before running, because these tests return early when a tape
is missing and would otherwise report a vacuous pass.

## Project-authored system-level corpus

| Corpus | Consumer | Corpus path | Strict wrapper | Licence | Required firmware |
|---|---|---|---|---|---|
| Amiga programmable HBLANK | `runtime-commodore-amiga` · `amiga_programmable_hblank` | [`commodore/amiga/programmable-hblank/`](commodore/amiga/programmable-hblank/) | [`scripts/verify-amiga-programmable-hblank.sh`](../scripts/verify-amiga-programmable-hblank.sh) | CC0-1.0 | Kickstart images for the selected ECS and AGA profiles, supplied externally |
| Amiga programmable HBLANK write timing | `runtime-commodore-amiga` · `amiga_programmable_hblank_write_timing` | [`commodore/amiga/programmable-hblank-write-timing/`](commodore/amiga/programmable-hblank-write-timing/) | [`scripts/verify-amiga-programmable-hblank-write-timing.sh`](../scripts/verify-amiga-programmable-hblank-write-timing.sh) | CC0-1.0 | Kickstart images for the selected ECS and AGA profiles, supplied externally |
| Amiga Paula audio | `runtime-commodore-amiga` · `amiga_paula_audio` | [`commodore/amiga/paula-audio/`](commodore/amiga/paula-audio/) | [`scripts/verify-amiga-paula-audio.sh`](../scripts/verify-amiga-paula-audio.sh) | CC0-1.0 | Kickstart 1.3 r34.005, supplied externally |
| Amiga sprite horizontal phase | No strict consumer yet; unresolved evidence fixture | [`commodore/amiga/sprite-horizontal-phase/`](commodore/amiga/sprite-horizontal-phase/) | None; build and semantic validation are corpus-local | CC0-1.0 | Kickstart images for the selected OCS, ECS and AGA profiles, supplied externally |

The programmable-HBLANK corpus is project-authored and emulator-neutral.
Sources, case definitions, schemas, and deterministic build tools are retained
in the corpus directory. Generated ADFs, payloads, and the suite manifest below
`dist/` are ignored and rebuilt by the strict wrapper. Commercial Kickstart
ROMs are not included.

The source cases currently leave expected observations unresolved. The
Emu198x lane therefore verifies identities, boots the probes, and reports
stable measurements; it does not claim semantic conformance until independent
evidence promotes an expected observation. The first gate covers CCK-aligned
cases on ECS and AGA profiles. The AGA fine-position cases are excluded until
the capture grid can represent their 70 ns and 35 ns placement.

The write-timing corpus is a separate five-case suite because it asks about
state changes within a line rather than settled output geometry. Its
registered FS-UAE package contains ten stable ECS and AGA observations from
the UAE implementation family. Copperline 0.13.0 and vAmiga 4.4b12 cannot
answer the question through an admissible path. The observations therefore
remain single-family evidence. The strict Emu198x consumer verifies the
corpus, artifacts, package, and referenced evidence, boots the complete
ten-run matrix, proves the scheduled writes through the Copper MOVE log, and
compares all three semantic lines at fixed coordinates without tolerance or
alignment search. It writes structured success and failure reports keyed by
the full source revision. A passing result means agreement with the
registered UAE-family observations; it does not establish physical-hardware
conformance.

The Paula-audio corpus is a three-case steady-waveform suite. Its Emu198x
consumer verifies corpus identity, boots each case, and compares routing,
cadence, and the paired-volume relationship with the registered vAmiga 4.4b12
package at revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. The consumer verifies the package
hashes and provenance and remeasures the retained source WAVs. Emu198x agrees
with that single independent software family within the declared semantic
boundary. Exact RMS magnitude is not compared because the producers use
different filter, gain, and resampling paths. The neutral source cases remain
unresolved, and the package does not establish physical-hardware evidence or
a two-family software consensus.

The sprite-horizontal-phase corpus asks one unresolved question about a fixed
low-resolution sprite edge relative to retained hardwired HBLANK and a
bitplane marker. One deterministic ADF serves PAL OCS, ECS and AGA profiles.
The corpus supplies strict suite and capture schemas plus semantic validation,
but no producer capture, expected interval or Emu198x assertion is registered
yet. It must not be treated as a conformance gate until independently reviewed
observations and a strict consumer are added.

## System-level external gates

| Fixture | Consumer | Env var | Upstream source | Licence | Required firmware |
|---|---|---|---|---|---|
| Amiga Test Kit v1.12 | `runtime-commodore-amiga` · `amiga_test_kit` | `EMU198X_AMIGA_TEST_KIT_ADF` | keirf/amiga-stuff tag `testkit-v1.12` | Public domain / Unlicense | Kickstart 1.3 r34.005 through `EMU198X_AMIGA_KICKSTART_13_ROM` |
| Amiga Test Kit v1.21 A500 video | `runtime-commodore-amiga` · `amiga_test_kit_video` | `EMU198X_AMIGA_TEST_KIT_V121_ADF` | keirf/amiga-stuff tag `testkit-v1.21` | Public domain / Unlicense | Kickstart 1.3 r34.005 through `EMU198X_AMIGA_KICKSTART_13_ROM` |
| Amiga Test Kit v1.21 A1200 video | `runtime-commodore-amiga` · `amiga_test_kit_video` | `EMU198X_AMIGA_TEST_KIT_V121_ADF` | keirf/amiga-stuff tag `testkit-v1.21` | Public domain / Unlicense | A1200 Kickstart 3.1 r40.068 through `EMU198X_AMIGA_KICKSTART_31_A1200_ROM` |
| A1000 Kickstart disk v1.2 r33.180 | `machine-commodore-amiga-ocs` · `a1000_bootstrap_trace`; `runtime-commodore-amiga` · `diag_a1000_bootstrap_swap`, `golden_matrix` | `EMU198X_AMIGA_A1000_KICKSTART_DISK` | Commodore; the disk an A1000 loads into WOM at boot | Commercial — not redistributable, supplied externally like Kickstart itself | A1000 bootstrap ROM at `~/.emu198x/roms/commodore-amiga/a1000-bootstrap.rom` |
| C64 VIC-II PAL 6569 survey | `runtime-commodore-c64` · `vicii_testbench` | `EMU198X_C64_VICII_TESTBENCH_DIR` | VICE VIC-II testbench staging; exact upstream revision unresolved | Unresolved; externally supplied | C64 KERNAL, BASIC and character ROMs through `EMU198X_C64_ROM_DIR` |
| SHAKER 2.6 (CPC) | `machine-amstrad-cpc` · `shaker` | `EMU198X_CPC_SHAKER_DSK` | shaker.logonsystem.eu (`Shaker_CSL/shaker26.dsk`) | Creative Commons; attribution requested — cite the CRTC Compendium | CPC464 firmware through `EMU198X_CPC_ROM`; CPC6128 firmware through `EMU198X_CPC_6128_ROM` |

The Test Kit ADFs and their profile-specific Kickstart images are pinned by
[`amiga-test-kit-v1.12.sha256`](amiga-test-kit-v1.12.sha256) and
[`amiga-test-kit-v1.21.sha256`](amiga-test-kit-v1.21.sha256) for the A500, plus
[`amiga-test-kit-v1.21-a1200-aga-pal.sha256`](amiga-test-kit-v1.21-a1200-aga-pal.sha256)
for the A1200. An ADF may be delivered raw or in a ZIP; each manifest applies
to the normalised ADF bytes. The public-domain ADFs remain externally supplied,
and the proprietary Kickstart ROMs must not be added to the corpus store.

The C64 VIC-II survey consumes 17 PAL 6569 programs and reference images from
13 testbench categories. It registers all five colour-fetch-bug programs and
one representative from each other category. Its tracked
[`assets-v1.json`](commodore/c64/vicii-vice-survey/assets-v1.json) manifest pins
the 34 external testbench files and three ROMs by byte count and SHA-256 while
leaving their bytes external. The wrapper compares nearest C64 palette indices,
records exact integer pixel counts and writes a revision-keyed report. The
results are diagnostic fractions, not pass rates, and the reference
images do not share one uniform hardware-provenance claim.

SHAKER is Longshot's CPC hardware-accuracy suite, aimed at the Gate Array and
the CRTC across their manufacturing variants. It ships as an Extended DSK, and
the CPC464 has no FDC — the drive arrives with the 6128 — so the harness lifts
the AMSDOS binaries out of the image with `format-amstrad-dsk`, boots the
firmware to its BASIC prompt, and enters the module on an instruction boundary.
That bypasses AMSDOS, so anything the suite expects the firmware to have set up
must hold without it; module D does. The DSK and the firmware are pinned by
[`amstrad-cpc-shaker-2.6.sha256`](amstrad-cpc-shaker-2.6.sha256). The firmware
is externally supplied and falls under Amstrad's standing permission to
redistribute its ROMs with emulators, the same grant the z80test tarball's 48K
Spectrum ROM relies on.

**What this gate asserts, and what it does not.** It asserts that the modules
come out of the catalogue with a valid AMSDOS header checksum and the load and
entry addresses the suite expects; that module D takes over the machine and
draws its menu; that SHAKER's *own* CRTC detection reports type 0, which is
real software checking the claim that a 464 carries an HD6845S rather than a
comment asserting it; and that SHAKER KILLER 2 reaches and reports its four
interrupt measurements.

**On a 464 the values cannot be read at all.** SHAKER saves the screen into
expanded RAM before running KILLER 2, a 464 has none, so the copy lands on the
suite's own byte-to-hex table and every value prints as `<` or as a
string-terminating `$00`. That is faithful — a real unexpanded 464 does the
same — and it is pinned by
`killer_2_saves_the_screen_over_its_own_hex_table_on_a_464`.

**On a 6128 they are read and scored.** With banked RAM the table survives,
and SHAKER prints each measurement beside the value it expects.
`shaker_killer_2_scores_on_a_6128` pins all six exactly, value included, so a
change in either direction fails. **All six agree.** Three did not until
`/WAIT` stretching landed (#959), and the three that did not were exactly the
measurements of where the interrupt lands relative to an instruction — the ones
an unstretched instruction length moves. Modelling the pin moved those three
onto SHAKER's expected values and left the other three alone.

That makes this the CPC's strongest conformance gate: real period software
scoring the machine against figures its author took from hardware. It also
chose the free T-state — at 0, `DEC DE` reports SHAKER's CRTC 3/4 expectation
instead of the CRTC 0 one, so only one phase satisfies every line.

SHAKER's own page header is the second reason for restraint
(`SK 2-UNRELIABLE INTERRUPT SYSTEM BETWEEN CPCs`): a disagreement is not
automatically a defect until a target CPC variant is named.

The v1.12 gate is invoked through
[`scripts/verify-amiga-test-kit.sh`](../scripts/verify-amiga-test-kit.sh). The
v1.21 A500 and A1200 video gates are invoked through
[`scripts/verify-amiga-test-kit-video.sh`](../scripts/verify-amiga-test-kit-video.sh)
and
[`scripts/verify-amiga-test-kit-video-a1200.sh`](../scripts/verify-amiga-test-kit-video-a1200.sh),
respectively.
None of these gates is part of the CPU-corpus matrix or the current
private-mirror contract below. Their assertion boundaries are documented in
[`Amiga Test Kit verification`](../knowledge/processes/amiga-test-kit-verification.md)
and
[`Amiga Test Kit v1.21 video conformance`](../knowledge/processes/amiga-test-kit-video-conformance.md).

Directory layout each env var points at: the extracted corpus directory. The
6502 and Z80 SingleStepTests corpora and the SM83 corpus use per-opcode JSON
files (`ab.json` → opcode 0xAB, plus `cb.json` for the SM83 CB table). The
68000 corpus uses compressed instruction-group files such as
`ADD.b.json.gz`. Dormann is a single `.bin`; FUSE is its `tests.in` /
`tests.expected` pair; Lorenz is the suite's case files plus a `kernal.rom`.

**Lorenz uses a synthetic, fully-free KERNAL — no commercial ROM.** The Lorenz
harness traps CHROUT and installs its own reset/IRQ vectors, so the only KERNAL
code the suite executes is the interrupt handlers at `$EA31`/`$FE66`/NMI. A
hand-authored 8 KB KERNAL supplying compatible minimal handlers there (filler
elsewhere) reproduces the real-KERNAL result on all 265 cases — verified
2026-07-04. It is generated by
[`commodore/c64/synthetic-kernal/`](commodore/c64/synthetic-kernal/); the
`lorenz-6502` tarball carries it as `kernal.rom`. This keeps the Cloanto C64
KERNAL entirely off CI.

## The mirror

The nightly pulls each configured corpus from a **private GitHub repo's release
assets** — the "dedicated assets store" — via the `gh` CLI. This keeps the
corpora hermetic when the store has been populated.

The store's privacy is an access control, not a licence. Each asset requires a
recorded basis for the intended mirroring. In particular, the registered
SingleStepTests `680x0` revision contains no tracked licence, so its
redistribution remains unknown. Its existing private delivery asset must not be
published or made more widely accessible without a rights review.

**Store contract** (what the workflow expects):

- A repo named by the `ACCURACY_CORPORA_REPO` Actions **variable** (e.g.
  `emu198x/accuracy-corpora`), with a release tagged by `ACCURACY_CORPORA_TAG`
  (default `v1`).
- One `zstd` tarball asset per corpus, named `<artifact>.tar.zst`:
  `harte-6502`, `harte-z80`, `harte-68000`, `sm83`, `dormann-6502`,
  `fuse-z80`, `lorenz-6502` (the Lorenz tarball includes the KERNAL),
  `z80test`, `spectrum-system-tests`, `zx-spectrum-tests`.

  The last three carry ROMs as well as cases: `z80test` ships the free
  (Amstrad-permissioned) 48K ROM the exerciser boots on, and
  `spectrum-system-tests` ships `roms/128-0.rom` and `roms/128-1.rom`
  alongside `tapes/`. Jobs read the ROM paths straight out of the extracted
  tree rather than expecting them staged separately.
- A `SHA256SUMS` asset listing each tarball's checksum — the workflow verifies
  against it, so checksums live in the store, not hard-coded here.
- The `harte-68000` asset must contain files matching the in-repository
  `singlesteptests-680x0-e0d5ece.sha256` manifest.
- An Actions **secret** `ACCURACY_CORPORA_TOKEN`: a fine-grained PAT with
  read-only `contents` access to the store repo.

Each tarball extracts to a directory whose path becomes the corpus's env var.

## First-time setup (one-off, yours to do)

1. Create the private store repo; add the `ACCURACY_CORPORA_TOKEN` secret and the
   `ACCURACY_CORPORA_REPO` / `ACCURACY_CORPORA_TAG` variables to *this* repo
   (Settings → Secrets and variables → Actions).
2. For each corpus whose intended mirroring has been reviewed, assemble it from
   its upstream (table above) into `<artifact>/` and pack it:
   `tar --zstd -cf <artifact>.tar.zst <artifact>/`.
   For Lorenz, drop the synthetic `kernal.rom` (from
   `commodore/c64/synthetic-kernal/`) into the tarball alongside the cases.
3. `sha256sum *.tar.zst > SHA256SUMS`.
4. Create the release and upload the tarballs + `SHA256SUMS` as assets:
   `gh release create v1 -R <store-repo> *.tar.zst SHA256SUMS`.

Until the store exists and the secret is set, the canonical
`emu198x/emu198x` workflow fails in `preflight`. A fork without the store
configuration reports the missing assets and skips the corpus jobs.

Re-run on demand from the Actions tab (`workflow_dispatch`) once the store is
live.

## Staging a corpus locally

Pull from the same store the nightly uses, so a local run and a CI run are
comparing the same bytes:

```sh
mkdir -p ~/.emu198x/test-data && cd /tmp
gh release download v1 -R emu198x/accuracy-corpora \
  -p 'zx-spectrum-tests.tar.zst' -p 'SHA256SUMS'
grep ' zx-spectrum-tests.tar.zst$' SHA256SUMS | shasum -a 256 -c -
tar --zstd -xf zx-spectrum-tests.tar.zst -C ~/.emu198x/test-data/
```

Verify the checksum rather than trusting the download — the workflow does,
and a corpus that silently differs from CI's is worse than no corpus, because
it makes a local result look authoritative when it is not. Note `shasum -a
256 -c` on macOS where the workflow uses GNU `sha256sum`.

Then point the job's env var at it. The Spectrum corpora and their variables:

| Artifact | Variable | Feeds |
|---|---|---|
| `zx-spectrum-tests` | `EMU198X_ZX_SPECTRUM_TESTS_DIR` | 128K timing survey, SZX real-file reader |
| `spectrum-system-tests` | `EMU198X_SPECTRUM_SYSTEM_TESTS_DIR` (→ `tapes/`) | tape smokes, floating-bus oracles |
| `z80test` | `EMU198X_Z80TEST_DIR` | Patrik Rak's Z80 exerciser |

Set `EMU198X_STRICT_FIXTURES=1` alongside them. Without it a corpus you
failed to stage turns into a skip, which libtest prints as `ok` — and a
green run that asserted nothing is the failure mode this whole file exists
to prevent. The nightly sets it for exactly that reason; a developer run
that omits it is not reproducing the nightly.

Two of these are nightly-only and slow — the 48K survey takes about 29
minutes, the 128K about 6.5 — which is precisely why their records go stale:
a PR that earns an improvement cannot see it. If you change contention,
`/INT` timing or anything else that moves instruction cost, run the surveys
before assuming the constants still describe reality.

## Related documents

- [Amiga Test Kit v1.12 fixture identity](amiga-test-kit-v1.12.md)
- [Amiga Test Kit v1.21 fixture identity](amiga-test-kit-v1.21.md)
- [Amiga Test Kit verification](../knowledge/processes/amiga-test-kit-verification.md)
- [Amiga Test Kit v1.21 video conformance](../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Portable programmable-HBLANK corpus](commodore/amiga/programmable-hblank/README.md)
- [Amiga programmable-HBLANK conformance](../knowledge/processes/amiga-programmable-hblank-conformance.md)
- [Portable programmable-HBLANK write-timing corpus](commodore/amiga/programmable-hblank-write-timing/README.md)
- [Amiga programmable-HBLANK write timing](../knowledge/processes/amiga-programmable-hblank-write-timing.md)
- [Portable Paula-audio corpus](commodore/amiga/paula-audio/README.md)
- [Registered vAmiga Paula-audio package](commodore/amiga/paula-audio/references/vamiga-4.4b12-60fd1e6b/README.md)
- [Amiga Paula-audio conformance](../knowledge/processes/amiga-paula-audio-conformance.md)
- [Amiga Paula stereo routing](../knowledge/decisions/amiga-paula-stereo-routing.md)
- [Test ROM bundling policy](../knowledge/decisions/test-rom-policy.md)

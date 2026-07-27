# Decision: Amiga full-family architecture review — preparing the seams for OCS / ECS / AGA / CDTV / CD32 / future

**Date:** 2026-05-21
**Status:** Seam 1 landed 2026-05-21 (commits `f7392a0`..`ded7b2c`).
Seams 2–5 proposed.

## What this is

A forward-looking review of the Amiga implementation against the
declared scope: **every Amiga model and variant**, from the A1000
through to AGA hardware (A1200, A4000, CD32), CD-ROM consoles (CDTV,
CD32), and future-Amiga targets (Apollo Vampire FPGA + AC68080,
PiStorm, RTG framebuffer expansions).

The first Amiga architecture review
([`amiga-architecture-review.md`](amiga-architecture-review.md))
closed out 2026-05-21 with all five seams landed. That review's
scope was the OCS-only boot blocker — Workbench 1.3 to desktop and
the load-bearing structural issues that surfaced during the
Kickstart push. This review picks up where that one closed: the
spine is correct, the OCS seams are tightened, but the implementation
is shaped for a single chipset family and needs to widen to
accommodate the rest of the Amiga zoo.

The spine still stays. The seams that need work are different.

This document mirrors the structure of
[`spectrum-architecture-review.md`](spectrum-architecture-review.md),
[`c64-architecture-review.md`](c64-architecture-review.md), and
[`nes-architecture-review.md`](nes-architecture-review.md). The seam
findings are grounded against the OCS implementation in
`machine-commodore-amiga-ocs`, the ECS sibling in
`machine-commodore-amiga-ecs`, the 68k-family substrate in
`motorola-68k-common` / `motorola-68000` / `motorola-68030` /
`motorola-68040`, and the AGA-scaffold material in
`Emu198x-Oldest/`.

## What we are *not* changing

These decisions are load-bearing and validated by Tom Harte 100% on
the 68000 (1,000,058 vectors), Workbench 1.3 and 2.04 booting to
desktop, the catalogue's anchor families (A1000 / A500 / A500+ /
A500-maxed) passing, and the snapshot round-trip discipline locked
in `runtime-commodore-amiga/tests/snapshot_roundtrip.rs`. Nothing in
this review revisits them:

- **Master oscillator drives the loop** (`tick_cck` for the Amiga;
  one CCK = 280 ns PAL = ~3.546 MHz). The crystal is the only time
  anchor; chips tick from it.
- **Pin-level CPU bus interface for every CPU.** `Cpu68000` exposes
  `addr`, `data`, `as_n`, `uds_n`, `lds_n`, `r_w_n`, `fc0..fc2`,
  `dtack_n`, `berr_n`, `vpa_n`, `ipl0_n..ipl2_n`, `reset_n` as pins.
  The machine layer reads pins between half-cycles. No Bus trait.
  Same shape will be used for 68010 / 68020 / 68030 / 68040 / 68060
  / AC68080 with the bus-protocol differences (sync vs async, 16-bit
  vs 32-bit) modelled by the per-CPU crate.
- **Chip-as-trait/struct-with-pins.** Each chip — Paula, Agnus,
  Denise, the two CIAs, Gary, autoconfig boards — is a struct with
  the silicon's pin surface as fields. One implementation per
  silicon revision. The OCS / ECS Agnus pair already follow this:
  `Agnus` (OCS) and `AgnusEcs` (ECS) live side-by-side in
  `commodore-agnus-ocs` and `commodore-agnus-ecs`.
- **Manufacturer-chipname crate naming.** `commodore-agnus-ocs`,
  `commodore-agnus-ecs`, `commodore-paula-8364`, `commodore-gary`.
  AGA additions land as `commodore-agnus-aga`,
  `commodore-denise-aga`, and `commodore-akiko-7xxx` (CD32's
  chunky-to-planar). AHI-class RTG boards would be
  `picasso-iv-86c764`, `gvp-spectrum-cl-gd5446`, etc.
- **Per-system run loops.** No universal tick pattern across
  systems. The Amiga's `tick_cck` is allowed to be distinctly
  shaped from the Spectrum's pixel-tick or NES's master-clock
  divider.
- **`AmigaMachine` trait + `AmigaRuntime<M: AmigaMachine>`
  generic.** This pair (added in commits `3c15873` and `a40e313`)
  is the right level of abstraction across chipset variants. The
  trait has 13 methods covering tick / framebuffer / audio / input /
  snapshot / query. We tighten *within* it, never replace it. A
  future AGA-class machine will plug in as `AmigaMachine` impl, not
  as a third sibling pattern.
- **Tom Harte 68000 100% pass.** 1,000,058 / 1,000,058. The CPU
  correctness bar holds.

If the review below appears to require revisiting any of these, the
review is wrong and the decision wins.

## The five seams

### Seam 1 — Shared chip substrate across OCS / ECS / AGA / future

**Current state.** Two machine crates side-by-side:

- `machine-commodore-amiga-ocs` (2016 lines)
- `machine-commodore-amiga-ecs` (1953 lines)

Both contain copies of `cia.rs` (identical), `copper.rs` (identical),
`rtc.rs` (identical), `memory.rs` (765 lines each, ~99% identical),
`denise.rs` (716 vs 713 lines, ~99% identical), and `agnus.rs`
(31 vs 33 lines, ~94% identical). The two crates' `lib.rs` files
have nearly identical function counts (35 vs 36) and similar shapes.

**Friction observed.** Adding AGA as a third sibling crate would
require a third copy of the same five files. The ECS crate's
`lib.rs` still has copy-paste vestiges from its OCS origin (the
header comment says "OCS chipset" inside the ECS crate). With AGA +
CDTV + CD32 + Vampire on the roadmap, this pattern produces a
6+-way clone with manually-maintained drift.

**Diagnosis.** The OCS / ECS variants differ in specific places
(Agnus chip wiring, Denise rendering depth, memory map for ECS's
1 MiB chip RAM upgrade) but share the vast majority of board-level
behaviour. The right shape is the Spectrum's per-class crate
pattern: a `common-commodore-amiga` substrate crate holding the
identical board logic, plus per-chipset *machine* crates that wire
in their specific chipset chips and the deltas.

**Proposed change.**

1. Extract `common-commodore-amiga-machine` (or
   `common-commodore-amiga-board`). Move the identical /
   near-identical modules: `cia.rs`, `copper.rs`, `rtc.rs`,
   `memory.rs` (with chip-RAM-size parameterised), and the
   chipset-agnostic parts of `denise.rs` and `agnus.rs`.
2. `machine-commodore-amiga-ocs` becomes a thin wrapper that
   imports the substrate + wires the OCS-specific chip variants
   (`commodore-agnus-ocs`, `commodore-denise-ocs`).
3. `machine-commodore-amiga-ecs` becomes the same wrapper with
   the ECS chips (`commodore-agnus-ecs`, `commodore-denise-ecs`).
4. `machine-commodore-amiga-aga` lands as a third wrapper when
   AGA chip crates exist.
5. CDTV reuses OCS substrate + adds the CD-ROM peripheral. CD32
   reuses AGA substrate + adds AKIKO + CD-ROM. Vampire-class
   Amigas reuse AGA substrate + swap the CPU.

**Silicon evidence (Reference library):**

- *Amiga Hardware Reference Manual* (3rd ed., Commodore-Amiga
  Technical Reference Series) — OCS chipset reference. ECS / AGA
  are documented as silicon supersets with the same board-level
  bus arbitration and memory map (chip RAM size and bitplane depth
  excepted).
- *Service manuals for A500, A500+, A1200, A4000* — schematic-level
  confirmation that CIA / RTC / copper integration is identical
  across the chipset boundary. The differences live in Agnus /
  Denise.

**Cross-validation references.** WinUAE's chipset selection is the
canonical implementation pattern: a single board substrate with
selectable chipset chips. FS-UAE inherits the same shape. Our
per-machine-crate split is heavier; the substrate extraction brings
us closer to the proven pattern without losing the "chip as
struct-with-pins" rule.

**Scope.** ~1500 lines moved from each per-chipset crate into a new
`common-commodore-amiga-*` crate. Net reduction across the
workspace as the duplicates collapse into one home. AGA machine
crate becomes a ~500-line wrapper (chipset-specific wiring only)
instead of a 2000-line fork.

**Status: landed 2026-05-21** (commits `f7392a0`..`ded7b2c`).
`common-commodore-amiga` crate created. Six modules moved in
incremental commits: `rtc`, `cia`, `memory` (all byte-identical or
~99% identical), `denise_chip` (new — trait + impls for both chip
variants), `denise` (generic over `C: DeniseChip`), `copper`. The
load-bearing piece is the `Denise<C: DeniseChip>` generic wrapper
— per-chipset machine crates instantiate it via type alias
(`pub type Denise = common::denise::Denise<DeniseOcs>;` etc.) and
AGA's future `commodore-denise-aga` need only impl `DeniseChip`
to plug in unchanged. Net workspace reduction: ~2000 lines of
duplication eliminated. The `agnus.rs` module stays per-chipset by
design (a 30-line re-export of the chipset-specific Agnus chip
type). The remaining `lib.rs` duplication (~2000 lines each in OCS
and ECS) is chipset-specific wiring rather than byte-identical
clones and is not a Seam 1 concern; deeper architectural seams on
the machine layer were already covered by
[`amiga-architecture-review.md`](amiga-architecture-review.md)
Seam 1 (`service_cpu_bus` → `BusTransaction`/`BusResponse`).
Tests deduplicated; all crate-level test suites green.

**Why this matters for other systems.** Same pattern the Spectrum
proved with `common-sinclair-zx-spectrum` + per-class crates. The
Amiga family is the densest example in the workspace; getting it
right here generalises to any system with multiple chipset
revisions (Atari ST/STE/TT, Acorn Archimedes A3000 / A5000 /
RiscPC, BBC Master series).

### Seam 2 — 68k family completion (68020 / 68030 / 68040 / 68060 / AC68080)

**Current state.** The 68k family substrate is in place:

- `motorola-68k-common` — shared substrate (addressing, ALU, bus
  pin types, prefetch micro-op queue, register file, status flags,
  CpuModel / TimingClass / CpuCapabilities metadata). 52 lines as
  the re-export hub.
- `motorola-68000` — the only concrete CPU. Tom Harte 100%.
  `Cpu68000` is the production type.
- `motorola-68030` (122 lines) — skeleton with detailed inline
  notes for what the implementation needs. No state machine.
- `motorola-68040` (162 lines) — same shape as 68030. No state
  machine.

No `motorola-68020`, `motorola-68060`, `motorola-ac68080`,
or `motorola-68010` crate exists. The 68000 crate's lib.rs
documents that higher-variant crates "currently re-export this type
as a stand-in until each variant's state machine is built out" —
but the re-exports aren't actually present yet.

**Friction observed.** Every Amiga variant beyond A500/A1000/A2000
needs a different CPU:

- **A1200, CD32**: 68EC020 (14 MHz). Different bus protocol from
  68000 — 32-bit synchronous instead of 16-bit asynchronous.
- **A3000**: 68030 (16-25 MHz).
- **A4000**: 68040 (25-40 MHz). Some A4000T variants ship with
  68060.
- **Vampire V2 / V4**: AC68080 (FPGA-implemented 68060-class with
  vector extensions).

The 68020+ family represents a fundamental architectural shift: the
68000-style asynchronous bus (DTACK-handshaked, 4-cycle minimum
instruction) gives way to a 68020+ synchronous bus (clock-edge
sampling, instruction cache, pipelined execution). A machine
written against `Cpu68000`'s pin surface can't trivially swap in a
68020.

**Diagnosis.** The skeleton crates document the architectural
deltas but don't implement them. The deltas are well-bounded:

- **68010**: minor — adds VBR, loop mode. Drop-in for 68000 in
  most cases.
- **68020**: major — 32-bit data bus, instruction cache, full
  32-bit ALU, new addressing modes (memory indirect, full PC
  displacement), coprocessor interface (FPU / MMU). EC variant
  omits MMU; LC variant omits FPU coprocessor.
- **68030**: superset of 68020 plus on-die MMU + data cache.
- **68040**: superset of 68030 plus on-die FPU + harvard-style
  caches.
- **68060**: superset plus superscalar dispatch.
- **AC68080**: 68060 superset plus Apollo extensions.

**Proposed change.** Implement in the order matching variant
demand:

1. **68EC020 first.** Unlocks A1200, CD32, the most-requested
   AGA targets. Crate `motorola-68020` lives in the workspace
   with a `Cpu68020` struct exposing the 68020's synchronous-bus
   pin surface. Re-uses `motorola-68k-common` for ALU /
   addressing / register file. New microcode for 32-bit
   memory-indirect addressing modes and coprocessor interface
   (stubbed for EC variant).
2. **68030.** Unlocks A3000 and accelerator boards (CSA Magnum,
   GVP Accelerator). On-die MMU implementation lives in this
   crate; 68040 re-uses it via `MmuMode::M68040` per the existing
   skeleton plan.
3. **68040.** Unlocks A4000 base model.
4. **68060.** Optional A4000T accelerator coverage.
5. **AC68080.** Apollo Vampire targets. May land outside Motorola
   namespace (`apollo-ac68080`) since it's a clean-room FPGA
   reimplementation with vector extensions.

Each variant follows the same shape: own crate, own pin surface
(may differ between variants), own state machine, own Tom Harte
slice. The 68k-common substrate grows only as new instructions are
added; existing 68000 code stays unchanged.

**Silicon evidence (Reference library):**

- *MC68020 User's Manual* (Motorola). The canonical 68020 spec —
  bus cycles, addressing modes, coprocessor interface.
- *MC68040 User's Manual* (Motorola). Harvard caches, MMU details.
- *Apollo AC68080 reference* (Gunnar von Boehn et al.,
  Apollo-Team / GitHub). Vector extension docs and FPGA-specific
  cycle accuracy targets.

**Cross-validation references.** Musashi (68000-family C
implementation, vendored at `Emu198x-Unclean/emulators/cpu-libs/`)
covers 68000 through 68040 in production form. WinUAE's 68k-family
implementation is more cycle-accurate. Tom Harte tests now exist
for 68020 / 68030 / 68040 (community-generated, separate corpus
from the canonical 68000 vectors).

**Scope.** Per-CPU: ~3000-5000 lines (microcode + decode tables +
state machine + tests). Largest scope of any seam in this review.
Phaseable: 68020 unlocks the most variants, then iteration to 68030
/ 68040 as A3000 / A4000 catalogue entries land.

### Seam 3 — Display output surface (chipset + AKIKO + RTG + dual-display)

**Current state.** `AmigaMachine::chipset_framebuffer() -> &[u32]`
returns one framebuffer of fixed dimensions. Denise renders to it
on every chipset cycle. The runtime's frame-sink consumes it. One
framebuffer in, one framebuffer out.

**Friction observed.** Three future display surfaces don't fit:

1. **AGA's HAM-8 / SuperHires / 256-colour 320x256 modes.** Same
   conceptual surface (one chipset framebuffer) but variant pixel
   formats. Need format negotiation between the machine and the
   runtime / native verifier.
2. **AKIKO (CD32) chunky-to-planar conversion.** AKIKO converts
   chunky pixel data to bitplanes inline. The framebuffer is still
   Denise's, but there's an upstream transformation that the
   machine layer needs to model. Doesn't change the trait surface
   per se — it's an internal Denise concern — but the chipset
   identity (AGA + AKIKO) matters for variant selection.
3. **RTG (Re-Targetable Graphics).** Picasso II/IV, Spectrum, OPAL
   Vision, etc. are Zorro-II/III cards that expose their own
   framebuffer in addition to (or instead of) the chipset's. Some
   setups run dual-display: chipset video on the Amiga's RGB out
   + RTG on a second monitor.

The current trait surface assumes exactly one chipset framebuffer.
RTG adds zero, one, or two extra framebuffers per machine. Vampire
V4 boards have RTG built into the FPGA and can drive HDMI directly.

**Diagnosis.** "One chipset framebuffer" is correct for OCS / ECS /
AGA without RTG. For RTG-equipped variants, the surface needs to
emit 0..N additional framebuffers per frame. The natural shape is
an iterator-style "draw outputs" method that yields each active
display target.

**Proposed change.**

1. Keep `chipset_framebuffer()` as the chipset-native output —
   it's the load-bearing default.
2. Add `display_outputs(&self) -> &[DisplayOutput]` returning a
   slice of named outputs (`"chipset"`, `"rtg-0"`, etc.) with
   their framebuffer + dimensions + pixel format.
3. The runtime's frame-sink picks an output by name or routes all
   active outputs to host-side displays.
4. AKIKO lives inside the AGA Denise (chipset internal). Not
   exposed at the trait surface.
5. Pixel format declaration on each output (`RGBA32`, `RGB565`,
   `Indexed8WithPalette`, etc.) so the runtime can convert or
   pass through.

**Scope.** ~100 lines in the trait + per-machine impl. Two new
crates pending RTG demand: `picasso-ii-cl-gd5426`,
`picasso-iv-86c764`. AKIKO scope is folded into the AGA Denise
implementation.

**Why this matters for other systems.** Any system with optional
add-on graphics (Atari ST + ICD, Acorn Archimedes + VIDC slots,
Apple II + RGB cards) hits the same shape. Doing it right on the
Amiga first means the others copy a known pattern.

### Seam 4 — Storage zoo (multi-floppy + IDE + SCSI + CD-ROM + AKIKO + Vampire SD)

**Current state.** `AmigaMachine::insert_floppy0(adf, change_pending)`
— single-floppy single-slot. The runtime takes one ADF and inserts
it into DF0.

**Friction observed.** Real Amigas have a much wider storage zoo:

- **DF0-DF3**: four floppy slots on A500/A2000/A4000.
- **IDE on A1200/A4000**: HDF format + IDE controller + Buddha
  expansion variants.
- **SCSI on A2000/A3000/A4000T**: Commodore A2091, GVP variants,
  Phase 5 variants.
- **CD-ROM**: CDTV (Mitsumi-style), CD32 (Akiko-coupled), A570
  add-on.
- **AKIKO chunky-to-planar**: CD32-specific but storage-adjacent
  (lives on the same DMA path).
- **Vampire SD card**: FPGA-resident, exposed as a custom
  peripheral.
- **PiStorm host file system**: the Raspberry Pi exposes folders
  as virtual storage; not a real silicon model but a real user
  scenario.

The single-slot API doesn't scale, and the catalogue's current
test harness only exercises one slot.

**Diagnosis.** The API needs to be slot-indexed and media-typed.
Inserting a CD-ROM into a CD32 should fail loudly if the machine
doesn't have a CD-ROM slot; inserting an ADF into DF2 should work
on A500 but fail on CD32.

**Proposed change.**

1. Define a `StorageSlot` enum:
   `Floppy(DF0..DF3)`, `Ide(Channel0..Channel1, Master/Slave)`,
   `Scsi(Id0..Id7)`, `CdRom`, `SdCard`, `HostMount`.
2. `AmigaMachine::insert_media(slot, media) -> Result<...>`
   replaces the single-floppy method. Machines declare which
   slots they support; unsupported slots return a clear error.
3. Media types as their own enum:
   `Adf(Bytes)`, `Hdf(Bytes)`, `IsoImage(Bytes)`,
   `CueBin{cue, bin}`, `SdImage(Bytes)`.
4. Per-variant storage layout declared in the machine's
   profile / model metadata. CDTV declares CdRom only; A500
   declares DF0-DF3; A1200 declares DF0-DF1 + IDE.
5. Catalogue script extends with `Mount` / `Eject` steps
   alongside `Insert`.

**Scope.** ~50 lines in the trait, ~200 lines per supported media
type (IDE, SCSI, CD-ROM, SD). New peripheral crates as needed:
`peripheral-commodore-amiga-ide`, `peripheral-commodore-amiga-scsi`,
`peripheral-commodore-amiga-cdrom`,
`peripheral-cd32-akiko`,
`peripheral-vampire-sd`.

**Why this matters for other systems.** Atari ST has the same shape
(SF314 floppy + ACSI hard drive + Megafile + CD-ROM on TT-class).
NeXT Cube has Optical / SCSI / Ethernet. Getting the storage
abstraction right on the Amiga generalises.

### Seam 5 — Variant catalogue + per-model boot CI

**Current state.** `runtime-commodore-amiga/tests/boot_invariants.rs`
covers four anchor families (A1000, A500, A500+A501, A500+,
A500-maxed). The catalogue (`crates/emu198x-catalogue/manifest/`)
has an `amiga.toml` with N entries. Workbench 1.3 and Workbench
2.04 desktop both prove. NTSC variants exist for OCS.

**Friction observed.** As the Amiga family expands beyond OCS-PAL,
the test matrix multiplies:

- **Models**: A1000, A500, A500+, A500-maxed, A1000-NTSC,
  A500-NTSC × N variants, A600, A1200, A2000, A3000, A4000,
  A4000T, CDTV, CD32, Vampire V2 / V4. Many of these have NTSC
  / PAL variants. Some have multiple Kickstart versions.
- **Software proofs**: each model needs at least one bootable
  workload (Workbench, demo, game) to confirm end-to-end. CDTV
  needs a CDTV-bootable title. CD32 needs a CD32 title. AGA
  needs an AGA-aware title (Lemmings 2, Beneath a Steel Sky,
  Aladdin AGA).
- **Per-chipset regressions**: a Denise change that breaks OCS
  shouldn't take ECS / AGA's existing tests with it. The current
  boot_invariants suite is shared across families; per-chipset
  per-model failure isolation is weak.

**Diagnosis.** The current single-file `boot_invariants.rs`
worked for 4 anchors. With ~20 anchors planned across chipsets +
regions + Kickstart versions, a flat file becomes unmaintainable.

**Proposed change.**

1. Split `boot_invariants.rs` into per-chipset modules:
   `boot_invariants_ocs.rs`, `boot_invariants_ecs.rs`,
   `boot_invariants_aga.rs`. Hermetic invariants (snapshot
   round-trip, frame-tick advancement) stay in a shared module.
   ROM-backed waypoints split by chipset.
2. Each per-chipset file declares its anchor models as
   `#[test]` functions, exercising a representative variant. The
   catalogue is the regression bench for breadth (Workbench
   booting on every model); the boot_invariants are the
   waypoints for *behaviour* (raster IRQ on the right line,
   Paula audio mixed correctly, etc.).
3. Per-variant `audio_routing_version` /
   `frame_routing_version` lock — extend the catalogue's Seam-4
   oracle work to make per-chipset routing-version bumps
   trigger per-chipset re-captures rather than full-family
   re-captures.
4. CD-ROM models gain a CD-fixture mode where a tiny synthetic
   CD-ROM image proves the CD-ROM peripheral wiring without
   needing real BIOS-cracked CD images.

**Scope.** ~100 lines per per-chipset boot_invariants file (3
files initially: OCS / ECS / AGA). Catalogue manifest updates
incrementally. Per-variant routing-version constants added as
chipset crates land.

**Why this matters for other systems.** Same as Seam 1 — the
Amiga family is the densest example. The pattern translates to
Atari ST/STE/TT/Falcon, Acorn Archimedes A3000/A5000/RiscPC, and
the BBC Master series.

## Verified non-issues

Recorded here because the audit examined them and they are not
seams. Future sessions should not re-discover.

### The old review's 5 seams

`amiga-architecture-review.md` documents the previous round of
seam work. All five landed (`service_cpu_bus` → `BusTransaction` /
`BusResponse`, disk DMA path into Paula, byte-write merge
locality, byte-lane response conventions, boot invariants suite).
The closeout in that document is the authoritative record.

### `AmigaMachine` trait surface

13 methods covering tick / framebuffer / audio / input / snapshot /
query. Reviewed and judged correct as-is. Adding RTG (Seam 3) will
add `display_outputs()` but not replace `chipset_framebuffer()`.
Adding multi-storage (Seam 4) will replace `insert_floppy0` with
`insert_media` but the per-variant declaration shape stays. The
trait is the right level of abstraction.

### `AmigaRuntime<M: AmigaMachine>` generic

Commit `3c15873`. Runtime is parameterised over machine type. ECS
runtime is `AmigaRuntime<AmigaEcs>`; OCS is `AmigaRuntime<AmigaOcs>`;
AGA will be `AmigaRuntime<AmigaAga>`. This is the right shape for
the family expansion. Don't revisit.

### Snapshot round-trip discipline

Commit `09a265c`. Postcard envelope with model + version validation;
fixed-point round-trip locked by
`runtime-commodore-amiga/tests/snapshot_roundtrip.rs`. Extending to
new chipsets just adds to the model enum.

### NTSC support

Commit `f8606e2`. Chip-layer line alternation, 5 NTSC OCS variants.
Region is per-Agnus, not per-machine; the substrate model handles
this cleanly. Don't revisit.

## Fidelity findings deferred beyond October

Real silicon-level fidelity gaps that don't affect the catalogue
regression bench and don't block October-public.

### Paula audio interpolation modes

Real Paula has period-driven sample-and-hold; some games depend on
specific phase relationships when periods are very short (< 124).
Tracked but not blocking.

### Copper wait-with-blitter-priority

The blitter-start side is now bounded: Copper `WAIT` and `SKIP` consume
the externally visible busy signal, BFD applies to both instructions,
the comparison samples that signal after instruction fetch, and the
A1000 exposes busy on its first accepted startup CCK. See
[Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
and
[Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md).

The completion-side busy inputs are now bounded separately. Pre-AGA
main finish, BZERO and final D are serialized; Alice delays its source
finish until final D; and `DMACONR` and Copper BFD have distinct
first-idle CCKs. See
[Blitter completion pipeline](amiga-blitter-completion-pipeline.md).

Line-mode ONEDOT is also bounded: one write is permitted per horizontal
row, suppressed results still reach BZERO and completion, and their
would-be D cells remain available to the CPU. Standard line texture now
uses preloaded `BLTBDAT` independently of B DMA and advances from the
selected BSH bit. See
[Amiga blitter line-mode ONEDOT](amiga-blitter-line-onedot.md) and
[Amiga blitter line texture phase](amiga-blitter-line-texture-phase.md).

The remaining accuracy edge is the Copper's first request and fetch
after a completion-dependent `WAIT` becomes eligible, including
same-CCK cancellation and ownership. It does not block catalogue
entries.

### CIA SP / SDR shift register

CIA-A's SP pin is wired to the keyboard serial protocol; CIA-B's SP
is the parallel-port handshake. Both are partially implemented;
edge cases (mid-shift reset, double-rate writes) deferred.

### A1200 PCMCIA expansion

The CC0 / CC1 PCMCIA bus on A1200 / A600 supports network cards,
SRAM cards, and modem cards. Real silicon: Gayle-routed. Not in
scope until a PCMCIA-required title surfaces.

### FastIDE / Buddha / Buddha-Flash variants

A1200 / A4000 / accelerator-resident IDE controllers each have
their own register quirks. Standard A1200 IDE is what the seam
covers; Buddha-class accelerator variants deferred.

## Order of work

In order of leverage for unblocking the full Amiga family:

1. **Seam 1 (shared chip substrate)** — biggest cleanup, biggest
   unblock for adding AGA. Land first so AGA arrives as a
   third-class crate (~500 lines wrapper) instead of a 2000-line
   fork. Catches drift between OCS / ECS that already accumulates.
2. **Seam 2 (68k family — 68EC020 first)** — unlocks AGA + CD32.
   The biggest single scope but gates the most variants. Phaseable
   per CPU revision.
3. **Seam 5 (variant catalogue + per-chipset CI)** — incremental,
   lands per-chipset as Seams 1 + 2 produce new chipset machines.
   Begin once OCS / ECS sit on the shared substrate.
4. **Seam 4 (storage zoo)** — needed for CD32, CDTV. Can land in
   parallel with Seam 2 once the trait surface is designed.
5. **Seam 3 (display output surface)** — deferred until first RTG
   or AKIKO target surfaces. Lowest urgency since current
   single-framebuffer surface works for OCS / ECS / non-RTG AGA.

## Done criteria

- **Seam 1**: `common-commodore-amiga-machine` crate exists.
  `machine-commodore-amiga-ocs` and `-ecs` shrink to ~500 lines
  each (chipset wiring only). All identical / near-identical
  modules collapse to single homes.
- **Seam 2**: `motorola-68020` (or `motorola-68ec020`) implements
  the 68EC020 instruction set, passes its Tom Harte slice. AGA
  machine crate boots with 68EC020 in place of 68000.
- **Seam 3**: `display_outputs()` lives on `AmigaMachine`,
  defaults to `&[DisplayOutput { kind: "chipset", ... }]` for
  non-RTG machines.
- **Seam 4**: `insert_media(slot, media)` replaces
  `insert_floppy0`. Machines declare supported slots.
  CD32 / CDTV catalogue entries can insert CD-ROMs.
- **Seam 5**: per-chipset boot_invariants files exist (OCS / ECS /
  AGA). Per-variant routing-version constants gate re-capture.
- This document is updated with implementation status and links
  to commits as each seam lands.

## Non-goals

- Replacing the existing `AmigaMachine` trait. Tighten within it,
  never replace.
- Refactoring Paula. The disk DMA work landed in the previous
  review; Paula audio is correct enough for current catalogue
  entries; further fidelity work is per-test, not seam-class.
- Splitting `commodore-paula-8364` into per-revision crates. Paula
  is identical across OCS / ECS / AGA — one crate is correct.
- 68LC040 / 68LC060 (FPU-less variants). Cover 68040 / 68060
  fully; LC variants are a per-feature gate in the same crate.
- PiStorm-specific host-file-system semantics. PiStorm's value
  prop is a real CPU, not new chipset behaviour; the seam work
  here is sufficient.
- Vampire FPGA-internal RTG specifics beyond the trait surface.
  Treat the Vampire as a 68080-class CPU + RTG output; the FPGA
  vendor-specific implementation isn't a project concern.

## Related

- [`amiga-architecture-review.md`](amiga-architecture-review.md) —
  the previous review, all 5 seams landed 2026-05-21
- [`spectrum-architecture-review.md`](spectrum-architecture-review.md)
  — the per-class crate pattern (Seam 1's precedent)
- [`c64-architecture-review.md`](c64-architecture-review.md) —
  sibling review for the more mature Commodore family
- [`nes-architecture-review.md`](nes-architecture-review.md) —
  sibling review for the engineering-bar third system
- [`cpu-bus-interface.md`](cpu-bus-interface.md) — universal
  pin-level CPU rule
- [`within-family-layering.md`](within-family-layering.md) —
  chip-per-crate the seam fixes respect
- [`runtime-internal-shape.md`](runtime-internal-shape.md) —
  runtime layer the seam fixes build on
- [`amiga-port-plan.md`](amiga-port-plan.md) — staged port plan
  the OCS / ECS / AGA progression follows
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
  — shared startup pipeline, visible busy and Copper BFD boundary
- [Blitter completion pipeline](amiga-blitter-completion-pipeline.md)
  — revision-specific finish, final-D and observer boundaries
- [Amiga blitter line-mode ONEDOT](amiga-blitter-line-onedot.md)
  — per-row suppression, BZERO, completion and free-cell behaviour
- [Amiga blitter line texture phase](amiga-blitter-line-texture-phase.md)
  — preloaded B pattern and BSH selection
- [Copper WAIT and SKIP comparison phase](amiga-copper-wait-skip-comparison.md)
  — post-fetch beam and visible-busy sampling boundary
- [`october-catalogue.md`](october-catalogue.md) — Amiga is
  engineering-bar; no October deadline

## Reference library cross-links

The Amiga reference surface is rich. The most relevant material:

| Reference | Topic | Relevance |
|---|---|---|
| *Amiga Hardware Reference Manual* (3rd ed.) | OCS chipset spec | Seams 1, 3 |
| *Amiga ROM Kernel Reference Manual: Libraries* | Exec / Intuition / DOS APIs | Seam 5 |
| *AGA Hardware Reference* (Commodore-Amiga, draft) | AGA chipset deltas | Seams 1, 3 |
| *MC68020 / 68030 / 68040 / 68060 User's Manuals* | Per-CPU silicon spec | Seam 2 |
| *Apollo AC68080 reference* (Apollo-Team) | Vampire FPGA spec | Seam 2 |
| WinUAE source | Most accurate open Amiga emulator | Cross-validation for all seams |
| FS-UAE source | Sibling open implementation | Cross-validation |
| vAmiga source (vendored) | C++ Amiga emulator | Cross-validation |
| Musashi (vendored) | 68k-family C implementation | Seam 2 cross-validation |
| Tom Harte 68000 (in CI) | 68000 instruction-vector corpus | Locked at 100% |
| Tom Harte 68020 / 68030 / 68040 (community) | Higher-variant vectors | Seam 2 incoming |

### Cross-cutting global KB

- `~/knowledge/retro-peripheral-architecture-is-pin-budget-not-design-choice.md`
  — the 68k bus is the entire bus-arbitration vocabulary. Every
  Amiga chipset's behaviour (chip RAM contention, blitter DMA,
  copper waits, sprite DMA) is built on the pin budget the 68000
  exposes. Pin budget shapes architecture.

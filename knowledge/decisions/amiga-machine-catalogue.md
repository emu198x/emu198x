# Decision: Amiga machine catalogue + family-MCP migration

**Date:** 2026-05-26

## What this is

The shape we use to enumerate every Amiga model and subvariant
(A1000 through CD32, plus future Vampire / PiStorm) within the
chipset-axis abstraction already established by
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md)
and sequenced by
[`amiga-machine-rollout-plan.md`](amiga-machine-rollout-plan.md), and
the migration sequencing that brings the Amiga MCP layer to parity
with the Spectrum family-MCP pattern established by
[`spectrum-architecture-review.md`](spectrum-architecture-review.md).

This document is **catalogue + sequencing**. It does not revisit:

- The chipset axis (`AmigaRuntimeKind::{Ocs, Ecs, Aga, …}` — chipset
  is the only axis that changes the chip stack's structural shape).
- The `AmigaMachine` trait (every concrete machine type plugs in
  through this; the trait already exists and is load-bearing).
- The 68k variant pattern (wrap-don't-clone, hooks + flags — settled
  in [`motorola-68k-variant-pattern.md`](motorola-68k-variant-pattern.md)).
- The chip-extraction queue (Gayle → Alice + Lisa → Fat Gary + Ramsey
  → DMAC → Akiko → Buster — settled in the rollout plan).

If anything below appears to contradict those, the existing decisions
win and this one is wrong.

## The decision

### 1. Three chipset variants, period

`AmigaRuntimeKind` discriminates **only at the chipset axis** and
carries exactly three variants:

```rust
enum AmigaRuntimeKind {
    Ocs(AmigaRuntime<OcsCore>),  // A1000, A500 (all revs), A2000, CDTV
    Ecs(AmigaRuntime<EcsCore>),  // A500+, A600, A3000
    Aga(AmigaRuntime<AgaCore>),  // A1200, A4000, CD32
    // SAGA (Vampire's FPGA chipset) lands as a fourth variant when it does
}
```

**Fat Agnus 8372A goes under Ocs.** The 1 MB chip-RAM Fat Agnus is
paired with OCS Denise; the chip-stack *shape* is OCS, only the
chip-RAM ceiling moves. The model catalogue distinguishes
`A500::REV_C_PAL` (256 KB chip, pre-Fat-Agnus) from `A500::REV_G_PAL`
(1 MB chip, Fat-Agnus 8372A) — but both run on `OcsCore`.

### 2. Per-machine variation is configuration, not a Rust type

Each chipset core carries an `AmigaModel` constant plus the
configuration fields the model resolves to:

```rust
struct OcsCore {
    model: AmigaModel,           // amiga_model::a500::REV_G_PAL, …
    cpu: Cpu68000,               // stock CPU is hardcoded per chipset
    accelerator: Option<Accelerator>,
    memory: MemoryMap,           // chip / slow / fast layout + KS ROM split
    chipset: OcsChipset,         // Agnus + Denise + Paula + 2× CIA + Gary
    storage: FloppyController,   // floppy-only today; bigger storage zoo deferred
    region: Region,              // Pal | Ntsc
    kickstart: KickstartVersion, // 1.2 / 1.3 / 2.04 / …
    board_rev: BoardRevision,    // RevC / RevG / Rev6A / …
}
```

Adding a new machine within an existing chipset is **new
`AmigaModel` constant + new factory path**, not a new Rust type. The
chip stack and memory bus stay the same; the model resolves to
different ROM/RAM/storage config.

### 3. Stock CPU per chipset; accelerator as override

The chipset core hardcodes its stock CPU:

- `OcsCore.cpu: Cpu68000`
- `EcsCore.cpu: Cpu68000`
- `AgaCore.cpu: Cpu68EC020`

Accelerator boards (Blizzard, GVP A530, Phase 5 PPC, Apollo Vampire,
PiStorm) sit as an **optional override layer**:

```rust
enum Accelerator {
    // Reserved: GvpA530, BlizzardII, Blizzard1230, BlizzardPpc,
    //           Vampire, PiStorm, …
    // No variants implemented today — the type exists to lock in
    // the bus-dispatch hook for when accelerator work lands.
}
```

`Option<Accelerator>` on each core is always `None` until the
first accelerator implementation arrives. The bus-dispatch layer
checks `accelerator.is_some()` once per tick and routes either to
stock CPU or accelerator CPU. Curriculum tools (`query_cpu`,
`query_chipset`, etc.) view the *active* CPU — they don't care
whether it's stock or accelerated.

This shape was chosen over (a) a `CpuVariant` enum embedded per chip
core and (b) generic `OcsCore<C: Cpu>` because:

- A `CpuVariant` enum forces every chip core to embed every CPU
  type, balloons the snapshot envelope, and burns an enum-match per
  instruction.
- A generic `OcsCore<C: Cpu>` Cartesian-explodes `AmigaRuntimeKind`
  (chipset × CPU = 3 × 6 = 18 variants once accelerators land).

The stock-CPU + override shape stays at three chipset variants no
matter how many accelerators ship.

### 4. AmigaModel catalogue: hierarchical naming

Each machine + revision is one `AmigaModel` constant, organised
under per-family submodules so call sites read as
`amiga_model::a500::REV_C_PAL`:

```rust
pub enum AmigaModel { /* internal tag — opaque to callers */ }

pub mod amiga_model {
    pub mod a1000 {
        pub const PAL: AmigaModel = …;
        pub const NTSC: AmigaModel = …;
    }
    pub mod a500 {
        pub const REV_C_PAL: AmigaModel = …;   // 256 KB, pre-Fat-Agnus
        pub const REV_G_PAL: AmigaModel = …;   // 1 MB chip, Fat Agnus 8372A
        pub const REV_C_NTSC: AmigaModel = …;
        pub const REV_G_NTSC: AmigaModel = …;
    }
    pub mod a500plus {
        pub const PAL: AmigaModel = …;
        pub const NTSC: AmigaModel = …;
    }
    pub mod a600 {
        pub const PAL: AmigaModel = …;
        pub const HD_PAL: AmigaModel = …;       // factory-fitted IDE
    }
    pub mod a1200 {
        pub const PAL: AmigaModel = …;
        pub const HD_PAL: AmigaModel = …;
        pub const NTSC: AmigaModel = …;
    }
    pub mod a2000 {
        pub const REV_A_PAL: AmigaModel = …;
        pub const REV_B_PAL: AmigaModel = …;
    }
    pub mod a3000 {
        pub const DESKTOP_PAL: AmigaModel = …;
        pub const TOWER_PAL: AmigaModel = …;
    }
    pub mod a4000 {
        pub const A030_PAL: AmigaModel = …;
        pub const A040_PAL: AmigaModel = …;
        pub const TOWER_PAL: AmigaModel = …;
    }
    pub mod cdtv {
        pub const PAL: AmigaModel = …;
        pub const NTSC: AmigaModel = …;
    }
    pub mod cd32 {
        pub const PAL: AmigaModel = …;
        pub const NTSC: AmigaModel = …;
    }
}

impl AmigaModel {
    pub fn chipset(self) -> ChipsetKind { … }
    pub fn cpu(self) -> CpuKind { … }
    pub fn region(self) -> Region { … }
    pub fn chip_ram_kb(self) -> u32 { … }
    pub fn slow_ram_kb(self) -> u32 { … }
    pub fn default_kickstart(self) -> KickstartVersion { … }
    pub fn board_rev(self) -> BoardRevision { … }
    pub fn display_name(self) -> &'static str { … }
    pub fn profile_id(self) -> &'static str { … }
}
```

Factory functions on `AmigaRuntimeKind` accept a model + firmware:

```rust
impl AmigaRuntimeKind {
    pub fn from_model(
        model: AmigaModel,
        firmware: &FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        match model.chipset() {
            ChipsetKind::Ocs => Ok(Self::Ocs(AmigaRuntime::new(
                OcsCore::from_model(model, firmware)?,
            ))),
            ChipsetKind::Ecs => Ok(Self::Ecs(AmigaRuntime::new(
                EcsCore::from_model(model, firmware)?,
            ))),
            ChipsetKind::Aga => Ok(Self::Aga(AmigaRuntime::new(
                AgaCore::from_model(model, firmware)?,
            ))),
        }
    }
}
```

### 5. MCP family migration sequencing

The Spectrum's `SpectrumRuntimeKind` + `SpectrumLiveAccess` pattern
applies here, with one wrinkle: A1200 today uses a hand-rolled
`AmigaA1200Session` (not `HeadlessSession`) precisely because the
chip-level debug surface wanted direct access to CPU/Agnus/Denise/
Paula/CIA/copper state that the generic shell session doesn't
surface. The migration trades that direct access for a typed
`AmigaLiveAccess` trait that exposes the same accessors through the
kind enum.

Sequencing:

1. **Step 0 (this doc).** No code.
2. **Step 1 — catalogue.** Land `AmigaModel`, `ChipsetKind`,
   `CpuKind`, `Region`, `BoardRevision`, `KickstartVersion`,
   `MemoryMap`, empty `Accelerator` enum. Pure additive. No
   existing code changes.
3. **Step 2 — A1200 trait lift.** Make `AmigaA1200` impl
   `AmigaMachine`. Add `AmigaRuntimeKind::Aga(AmigaRuntime<AmigaA1200>)`.
   A1200 reachable through the kind enum for script-mode. MCP layer
   untouched.
4. **Step 3 — MCP migration.** Convert `AmigaA1200Session` to
   `HeadlessSession<AmigaRuntimeKind, AmigaSessionQueryProvider>`.
   Define `AmigaLiveAccess` trait with chip-level accessors
   (`cpu_registers`, `copper_list`, `agnus_registers`,
   `denise_registers`, `paula_registers`, `cia_a` / `cia_b`,
   `chip_ram`, `chipset_framebuffer`, etc.). Migrate the 33 MCP
   tools through `match_kind!` dispatch.
5. **Step 4+ — fill the catalogue.** Add `AmigaModel` constants
   and factory paths per machine, in the order set by the rollout
   plan: A1200 → A600 → CDTV → A4000/030 → CD32 → A3000 → A4000/040.
6. **Step 5+ — refactor `AmigaA1200` → `AgaCore`.** Once a second
   AGA machine lands (A4000/030 per the rollout), the A1200's
   machine type generalises into a shared `AgaCore` with the
   model carrying the per-machine config. Same shape as `OcsCore`
   today shared across the A1000 / A500 / A2000 / CDTV variants.
7. **Step 6+ — accelerator support.** First `Accelerator` enum
   variant arrives when Vampire AC68080 or PiStorm work begins.

Steps 0–3 are the "Amiga Phase 1 + Phase 2" of the family-MCP
migration. Steps 4+ are open-ended; they queue per-machine.

### 6. Where the MCP tools land

Migrated through `AmigaLiveAccess`:

- `run_frames`, `run_ticks`, `run_until_pc`, `reset`,
  `query_cpu`, `query_chipset`, `query_paula`, `query_cia`,
  `query_agnus`, `query_copper`, `query_memory`,
  `read_long`, `read_byte`, `poke_byte`, `poke_word`, `poke_long`,
  `watch_memory`, `watch_memory_clear`, `watch_memory_log`,
  `start_video_recording`, `stop_video_recording`,
  `start_audio_recording`, `stop_audio_recording`,
  `save_screenshot`, `save_snapshot`, `load_snapshot`.

Family-MCP–generic (already in shell):

- `wait_for_query_contains`, `wait_for_query_bool`,
  `query`, `query_paths`, `restart`, `load_media`, `media_transport`.

A1200-specific tools that don't generalise to OCS / ECS (e.g.
Akiko-specific tools when CD32 lands) stay as A1200 / CD32-specific
extensions; the family-MCP server dispatches them only when the
active kind variant is `Aga`.

## Drift triggers

Stop and re-consult before:

- **"Let me just add A1200 to the Ocs variant"** — no. A1200 is AGA.
  AGA Alice + Lisa have structurally different register sets and
  bitplane handling from OCS Denise. Chip-stack *shape* is the only
  thing the kind enum discriminates.
- **"The kind enum should have one variant per machine"** — no. Kind
  discriminates *chipset* (= chip-stack shape). Machine identity
  lives in `AmigaModel`, which is config.
- **"Treat A500+ as OCS because it shipped with KS 2.04"** — no.
  Kickstart is config. A500+ uses the ECS Agnus 8372B (productivity
  modes, SUPERHIRES), so it's ECS.
- **"Treat Fat Agnus 8372A A500s as ECS"** — no. 8372A is paired
  with OCS Denise. The chip-stack shape is OCS; only the chip-RAM
  ceiling moves.
- **"Add a `CpuVariant` enum inside `OcsCore` so we can swap CPUs at
  config time"** — no. Stock CPU is hardcoded per chipset.
  Accelerator boards swap the CPU via `Option<Accelerator>`, not by
  re-typing the core.
- **"Make `OcsCore` generic over CPU type"** — no. Cartesian
  explosion in `AmigaRuntimeKind` (chipset × CPU). The accelerator
  override layer is the escape hatch.
- **"Skip the model catalogue, hardcode A1200 first and figure out
  the catalogue once a second machine lands"** — no. Step 1 (the
  catalogue) is the gating commit; step 2 (A1200 trait lift)
  consumes it. Hardcoding the first machine without the catalogue
  would force a second catalogue retrofit later.
- **"Replace `AmigaMachine` trait — it doesn't fit A1200's debug
  surface"** — no. The trait is load-bearing per the architecture
  review. The chip-level debug surface gets its own
  `AmigaLiveAccess` trait sitting alongside (not replacing)
  `AmigaMachine`, same shape as Spectrum's `SpectrumLiveAccess`.
- **"Keep `AmigaA1200Session` hand-rolled forever — the kind enum
  doesn't need it"** — no. Step 3 of the sequencing is the family
  MCP migration. Until that lands, A1200's MCP path can't host
  scripts that target other Amiga variants.

## Cross-references

- [`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md)
  — chipset axis, `AmigaMachine` trait, 68k family substrate. Binding.
- [`amiga-machine-rollout-plan.md`](amiga-machine-rollout-plan.md) —
  rollout order, chip-extraction queue. Binding.
- [`spectrum-architecture-review.md`](spectrum-architecture-review.md)
  — the pattern this Amiga migration mirrors (kind enum +
  `SpectrumLiveAccess` + match_kind! dispatch).
- [`motorola-68k-variant-pattern.md`](motorola-68k-variant-pattern.md)
  — wrap-don't-clone for CPU variants. Binding.

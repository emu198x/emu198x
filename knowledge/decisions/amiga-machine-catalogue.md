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
paired with OCS Denise, so the chip-stack *shape* remains OCS.
Its Agnus-side revision capabilities—including the wider chip-RAM
address space, ten-bit sprite vertical comparators, extended blits and
programmable timing—are explicit configuration rather than properties
inferred from installed RAM. The machine composes the existing ECS
Agnus extension layer with OCS Denise; it does not duplicate those
register handlers or promote the complete chip stack to ECS.
The model catalogue distinguishes
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

> **Partially superseded 2026-06-12** — see
> [Amendment: composable configuration model](#amendment-2026-06-12-composable-configuration-model).
> The *mechanism* below (runtime active-CPU dispatch at the bus boundary,
> three chipset variants, no `OcsCore<C: Cpu>` generic) stands. What changes:
> CPU is now a first-class **runtime configuration axis** (`ActiveCpu`
> enum), because stock CPUs vary per model (A3000 = 68030, A4000 = 68040)
> and accelerators make CPU orthogonal to chipset across the whole range.
> "Stock CPU hardcoded per chipset" as a *type* is the part that's wrong.

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
  with OCS Denise. The chip-stack shape is OCS; individual Agnus-side
  revision capabilities still differ from early OCS Agnus.
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

---

## Amendment 2026-06-12: composable configuration model

**Driver:** the emulator must let the user **select a machine by independent
axes**, not only pick a named preset — machine type, chip RAM, slow RAM, fast
RAM, CPU, accelerator card, RTG (later), and any other attached peripherals.
Guiding star: **maximum authenticity and accuracy** (when a choice is between
an authentic model and a convenient shortcut, take the authentic one).

This amends rule 3 and extends rule 2. Rules 1 (three chipset variants), 4
(hierarchical `AmigaModel` catalogue), and the `AmigaMachine`/`AmigaLiveAccess`
trait shape are **unchanged and reinforced**.

### What stays (the load-bearing invariant)

**Chipset is the only axis that changes the chip stack's structural *type*.**
`AmigaRuntimeKind` keeps exactly its three variants (`Ocs`/`Ecs`/`Aga`, plus
`Saga` when Vampire lands). Everything else a user selects is **configuration
resolved at runtime**, carried on the core — never a new Rust type, never a
generic type parameter on the core. This is what keeps the variant set
*additive* instead of Cartesian.

### What changes: CPU is a runtime configuration axis, orthogonal to chipset

Rule 3 hardcoded one stock CPU type per chipset core (`OcsCore.cpu: Cpu68000`).
That under-models reality on two counts:

1. **Stock CPUs already vary within a chipset.** A3000 is ECS **+ 68030**;
   A4000 is AGA **+ 68040**; A1200 is AGA **+ 68EC020**. The chipset does not
   determine the CPU.
2. **Accelerators make CPU orthogonal to chipset across the whole range.** A
   stock-68000 A500 runs a Blizzard 1230 ('030), 1260 ('060), or PiStorm
   (ARM-hosted); an A1200 runs '040/'060/PPC/Vampire. *Any* chipset pairs with
   *almost any* CPU.

So the active CPU is a **runtime-resolved value**, held as a closed enum:

```rust
enum ActiveCpu {
    M68000(Cpu68000),
    M68010(Cpu68010),
    M68EC020(Cpu68020),  // EC = no coprocessor/MMU pins (A1200/CD32)
    M68020(Cpu68020),
    M68030(Cpu68030),
    M68040(Cpu68040),
    M68060(Cpu68060),    // when it lands
    // Vampire AC68080 / PiStorm-hosted land here too
}
```

The model resolves the **stock** `ActiveCpu`; an `Accelerator` overrides it
(and may add its own fast RAM + MMU/FPU). The bus-dispatch layer routes to the
active variant once per instruction — exactly the hook rule 3 already reserved
for `Option<Accelerator>`, now generalised so the *stock* CPU can also be
anything.

**Why an enum, not the alternatives the original rejected:**
- `OcsCore<C: Cpu>` generic — still rejected, for the original's reason: it
  Cartesian-explodes `AmigaRuntimeKind` (chipset × CPU). The enum keeps CPU
  *additive*.
- `Box<dyn Cpu>` — rejected, consistent with the codebase's closed-enum-over-
  heap-dispatch stance ([`runtime-internal-shape.md`](runtime-internal-shape.md)).
- The original's objections to a CPU enum (snapshot size, enum-match per
  instruction) are now acceptable: the active CPU occupies the same memory
  whether it's a typed field or an enum variant, and one match at the
  instruction-dispatch boundary is negligible. The objections only held while
  CPU *didn't need to vary* — the new requirement removes that premise.

### Extends rule 2: the full configuration surface

The core's configuration grows to the axes the user selects. Sketch:

```rust
struct AmigaConfig {
    model: AmigaModel,            // resolves the stock defaults below
    region: Region,              // Pal | Ntsc
    kickstart: KickstartVersion, // 1.2 … 3.1 / 3.2
    cpu: ActiveCpu,              // stock per model; accelerator overrides
    accelerator: Option<Accelerator>,
    ram: RamLayout {            // each independently sized within hw limits
        chip_kb,                //   capped by the Agnus (512K/1M/2M)
        slow_kb,                //   $C0_0000 trapdoor
        fast_kb,                //   Zorro-II/III / accelerator-local
    },
    rtg: Option<RtgBoard>,      // Picasso II / uaegfx — later (#117); dual-display
    peripherals: Vec<Peripheral>, // IDE/SCSI HDF, PCMCIA card, parallel,
                                  // serial bridge, network, mouse/joystick, …
}
```

`chipset` is *not* in the config struct — it is the `AmigaRuntimeKind` type the
config selects into. RTG and peripherals are optional/additive and default
empty, so today's machines are unaffected.

### User-facing selection: presets over a config space

The named-model catalogue (rule 4) becomes a set of **presets** that fill an
`AmigaConfig` with authentic stock values. The user can start from a preset and
override any axis (more fast RAM, an '040 accelerator, an RTG board, an HDF),
or build a config from scratch — subject to **authenticity validation** (e.g. a
2 MB chip-RAM selection requires an AGA/ECS Agnus; an RTG board requires a Zorro
slot the chipset/model provides). Invalid combinations are rejected, not
silently coerced.

### Drift triggers

- Reaching for `OcsCore<C: Cpu>` or `Box<dyn Cpu>` — no. CPU is the `ActiveCpu`
  enum; chipset is the only type axis.
- Adding a fourth `AmigaRuntimeKind` arm for a CPU or RAM difference — no. Only
  a new *chipset structural shape* (SAGA) earns a variant.
- Hardcoding "this chipset implies this CPU" anywhere — no. The model resolves
  the stock CPU; accelerators override; they are orthogonal.
- Silently coercing an impossible config (e.g. fast RAM with no slot, 2 MB chip
  on a 512 KB Agnus) instead of rejecting it — no. Validate against the real
  hardware envelope.
- Treating RTG as a chipset variant — no. It is an optional peripheral board
  with its own framebuffer (dual-display aware), per
  [[emu198x:project_amiga_long_term_scope]].

### Bearing on open issues

- **#110 (A3000) / #111 (A4000):** the catalogue collision (ECS+68030 vs
  ECS+68000; AGA+68040 vs AGA+68EC020) is resolved by this amendment — the model
  resolves the stock `ActiveCpu`; no new chipset variant.
- **#34 (unified driver):** the generic machine driver is parameterised over the
  chipset variant + an `ActiveCpu`-dispatching bus seam; this amendment defines
  the CPU half of that seam.
- **#117 (RTG):** RTG enters as an `Option<RtgBoard>` config axis (dual-display),
  not a chipset change.
- **#102/#43 (storage), #100 (serial), accelerators:** all land as `peripherals`
  / `accelerator` config, additive.

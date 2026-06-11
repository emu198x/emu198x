# Decision: Runtime crate internal shape

**Date:** April 2026

**Related:** [within-family-layering.md](within-family-layering.md) covers the crate-level layout (`common-{family}` / `{vendor}-{chip}` / `format-{family}-{format}` / `machine-{family}-{variant}` / `runtime-{family}`). This document is about the file layout *inside* one `runtime-{family}` crate.

## The decision

Every `runtime-{family}` crate splits production code across four per-concern modules — `runtime.rs`, `queries.rs`, `snapshot.rs`, `input.rs` — plus `profiles.rs` and any family-specific extras (`autoload.rs`, `file_loader.rs`). Tests live under `tests/` as per-topic integration files with a shared `tests/common/mod.rs` for helpers.

```
src/
  lib.rs              module declarations + re-exports
  runtime.rs          Runtime struct, MachineCore impl, lifecycle.
                      snapshot/restore are one-line delegators.
  queries.rs          SessionQueryProvider impl + path catalogue +
                      query-only helpers (boot status, screen-text
                      decode, etc).
  snapshot.rs         postcard envelope types + encode/decode.
  input.rs            free `apply_input_event(machine, event)` +
                      key/button lookup tables.
  profiles.rs         Model enum + per-variant MachineProfile data.
  autoload.rs         optional — boot-from-tape/disk autoloaders.
  file_loader.rs      optional — host-side `.prg` / `.cas` etc imports.
tests/
  common/mod.rs       shared helpers (FrameCollector, AudioCollector,
                      blank_firmware, ROM-size constants hardcoded
                      where pub(crate) doesn't reach).
  lifecycle.rs        construction, audio, drive attach, run_until.
  snapshot_roundtrip.rs
  queries.rs
  ...                 family-specific topics
```

When a family supports more than one machine variant (Spectrum's 7, the Amiga's eventual ECS/AGA/SAGA, a future C128 alongside the C64), the runtime is *additionally* generic over an `M: FamilyMachine` trait. The four-concern split still applies — it threads the `<M>` parameter through `runtime.rs` / `snapshot.rs` / `input.rs`, and a small `variants.rs` carries the per-variant `impl FamilyMachine for X` blocks plus the public type aliases. **This is layered on top of the four-concern split, not an alternative to it.**

## State of the family

As of April 2026:

| Runtime | Per-concern split | Generic over machine? |
|---|---|---|
| `runtime-commodore-c64` | ✅ | not yet (no second variant in this generation) |
| `runtime-nintendo-game-boy` | ✅ | no (single machine type) |
| `runtime-nintendo-nes` | ✅ | no (single machine type) |
| `runtime-commodore-amiga` | ✅ | ✅ — `AmigaRuntime<M: AmigaMachine>` with `AmigaOcsRuntime` alias (May 2026). The trait surface is designed for the full long-term scope: chipset variants (OCS / ECS / AGA / SAGA), CPU variants (Cpu68000 → Cpu68020/30/40 → Apollo AC68080 / PiStorm-host 68k), and RTG framebuffer expansion. NTSC + ECS/AGA Commodore variants land next; Vampire / SAGA / PiStorm / RTG are research-first. |
| `runtime-sinclair-zx-spectrum` | ✅ (hybrid) | ✅ — `SpectrumRuntime<M: SpectrumMachine>` with 7 variant aliases. |
| `runtime-dragon` | not normalised — Codex-owned, off-limits to this track | no |

## Why this shape

Two reasons compounding.

**1. Each concern has independent test gravity.** Lifecycle tests want to drive `MachineCore::run_until`; query tests want to walk the path catalogue against a runtime; snapshot tests want round-trip + error-path coverage; input tests want lookup-table completeness. Putting all four in one file forces every test to share the same setup boilerplate and hides per-module coverage gaps. The C64 Cov-4 work proved this: per-module coverage went from "1225 uncovered lines in `runtime.rs`" (one undifferentiated number) to actionable per-module gaps after the split, with `queries.rs` jumping 64% → 98% line coverage and 25% → 100% function coverage in one focused commit.

**2. The per-concern axis is orthogonal to the per-variant axis.** Spectrum proved this. Adding Pentagon128 to the family is a `variants.rs` edit; adding a screen-text-decode helper is a `queries.rs` edit; adding tape-block import is an `autoload.rs` edit. Each contributor knows where their change goes without having to navigate a 3000-line monolith. Without the split, a Spectrum maintainer landing a Pentagon128 timing fix and a contributor landing a query-path catalogue extension would conflict on the same file every time.

## Drift triggers

If you catch yourself doing any of these, **stop and re-consult this decision record**:

- **Adding `apply_input_event` as a method on the Runtime struct.** The shape says it's a free `fn` in `input.rs`. The four splits we did (C64 / GB / NES / Amiga) each had to undo this. The free-fn shape lets the function be generic-over-`M` later without churn.
- **Adding a new `impl SessionQueryProvider<...>` block at the end of `runtime.rs`.** Move it to `queries.rs` instead. If the file doesn't exist yet, that's the signal to do the split.
- **Inlining the snapshot envelope inside `MachineCore::snapshot` and `MachineCore::restore`.** The envelope type and its encode/decode functions live in `snapshot.rs`; the trait methods are one-line delegators.
- **Letting `runtime.rs` grow past ~700 production lines.** That's the size at which the C64 felt the smell. If you're approaching it, the split is overdue.
- **Letting `#[cfg(test)] mod tests` grow past ~150 lines inside a `src/` file.** Move tests to `tests/` integration files. Inline tests should only cover *private* symbols.
- **Adding a sibling `runtime-{family}-{variant}` crate** when the variant shares its chip stack with an existing runtime. The right shape is generic-over-`M` *within the existing runtime crate* — not a new crate.
- **Adding an `apply_input_event` as a method** (`&mut self`) on the Runtime struct. The shape is a *free fn* in `input.rs`. It can take `&mut <Machine>`, `&mut Option<Machine>` (Game Boy, where the machine is loaded lazily), or `&mut Runtime<M>` (Spectrum, where the runtime owns an input buffer that survives across `run_until` calls because the keyboard matrix is a whole-matrix-snapshot model rather than per-key events) — pick the smallest argument that lets the function do its job. What's not allowed is the method form: that doesn't generalise to the per-variant generic case and it tangles input handling into the lifecycle module.
- **Promoting struct fields to `pub` because a test or sibling module needs them.** Add a `pub(crate)` accessor instead. Match the C64 pattern — every cross-module read goes through a small named accessor.
- **Hand-rolling the variant-swap body** (build a new variant → install it → re-pace the session → reset) inside a `set_machine` tool or the script runner. That shape lives once in the shell as `HeadlessSession::swap_machine`; the runtime crate's job is only to `impl FamilyRuntime` for its dispatcher enum. See *The variant-dispatch shape lives in the shell* below.
- **Hard-typing a binary's session to one concrete variant** (`HeadlessSession<Spectrum48kRuntime, …>`) when the family has a dispatcher enum. Hold the enum (`SpectrumRuntimeKind`) instead, so mid-session `SetMachine` works in `--script` exactly as it does in MCP. The script runner picks the *initial* variant (e.g. from a snapshot's model); it does not lock the session to it.

## The variant-dispatch shape lives in the shell (`FamilyRuntime`)

**Date:** June 2026. Part of #456 (MCP / script / UI parity).

A multi-variant family carries a **dispatcher enum** — `SpectrumRuntimeKind` (OCS-style 13 models), `AmigaRuntimeKind` (OCS / ECS / AGA) — a single concrete type that holds any one variant and forwards `MachineCore` + the family live-access trait + `SessionQueryProvider` to the active case. That enum is the per-family part and stays in the `runtime-{family}` crate (`family_runtime.rs` / `variants.rs`).

What is **not** per-family is the *shape* of constructing-a-variant, knowing-its-pacing, and swapping-it. That lives once at the top, in `emu198x-shell`:

```rust
// machine.rs — sits ABOVE MachineCore; never touches the run loop.
pub trait FamilyRuntime: MachineCore + Sized {
    type Model: Copy;
    fn from_firmware(model: Self::Model, firmware: &FirmwareSet<'_>)
        -> Result<Self, MachineError>;
    fn native_frame_ticks(&self) -> u64;
}

// session.rs — the one swap body, generic over any family.
impl<M: FamilyRuntime, Q: SessionQueryProvider<M>> HeadlessSession<M, Q> {
    pub fn swap_machine(&mut self, model: M::Model, firmware: &FirmwareSet<'_>)
        -> Result<(), SessionError> {
        let new = M::from_firmware(model, firmware)?;
        let ticks = new.native_frame_ticks();
        *self.machine_mut() = new;
        self.set_native_frame_ticks(ticks);   // re-pace to the new variant's frame
        self.reset(ResetKind::Hard)
    }
}
```

Each family enum does `impl FamilyRuntime` (a 3-to-13-arm `from_firmware` dispatch + a `native_frame_ticks` reading the active variant's frame length). The MCP `set_machine` tool and the `--script` `SetMachine` step both call `swap_machine` — one implementation, so the two modes can't drift. `from_firmware` is the trait method *only* (not also inherent) — same name on both makes `Type::from_firmware(…)` calls ambiguous (E0034), so call sites that build the enum bring `FamilyRuntime` into scope.

**Why a trait + enum and not `Box<dyn>`.** The variant set is closed and known at compile time — exactly what an enum is for. Eliminating the enum via `Box<dyn MachineCore>` was considered and rejected: it contradicts this record's "generic-over-`M` within the crate" stance, reintroduces the heap indirection the Amiga enum explicitly rejects (`#[allow(clippy::large_enum_variant)]` — one instance per session, held for its lifetime, so boxing only adds per-tick indirection on the hot forwarding path), and `SpectrumMachine::variant_query_paths()` is a static (no-`self`) method, making the query surface not object-safe without a wrapper rewrite across every machine impl + MCP handler + test. The trait lifts the *shared shape*; the enum keeps the *closed dispatch*.

## When the per-variant generic axis applies

Add the `<M: FamilyMachine>` parameter when **two or more concrete machines share a chip stack with only timing / IO / memory-map differences**. The Spectrum family is the canonical example. The Amiga family is the next one — ECS/AGA/SAGA share the OCS chip stack with progressive enhancements.

Don't add it speculatively — convert at the point a second variant arrives. The conversion is mechanical when the four-concern split is already in place: introduce the trait, implement it for the existing concrete machine, parameterise the four files, add aliases. Doing the per-concern split *first* (even before a generic conversion is planned) buys this future cheapness; doing them together is harder.

## Honest exclusions

- **`autoload.rs`, `file_loader.rs`, `profiles.rs`** are family-specific and don't have to exist in every runtime. Add them only when there's something to put in them.
- **Dragon (`runtime-dragon`) is intentionally outside this normalization** while Codex iterates on its CSS pipeline / VDG timing. The shape will apply once Codex hands it back.
- **`spectrum_48k.rs` is not renamed `queries_48k.rs`.** It carries a deliberately variant-specific `SessionQueryProvider` impl (boot detection via 48K ROM glyphs) that has not been generalised to other Spectrum variants. That's a feature-completeness scope, not an architectural smell. If we ever generalise the query provider across variants, the file converges into `queries.rs` then.

## How to update this record

If you propose a runtime-internal shape that contradicts this document — for example, a fifth concern module, or a different file naming scheme — update this record first with the rationale and the migration plan, then change the code. Do not silently grow the shape.

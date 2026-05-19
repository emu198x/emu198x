# Decision: Within-family layering

**Date:** April 2026

## The decision

Every system family in this project lays out the same five-piece structure:

```
common-{family}            — shared timing, traits, helper types (hardware-only)
{vendor}-{chip}            — one crate per real silicon part (CPU, ULA, sound, FDC, …)
format-{family}-{format}   — one crate per file format (snapshots, tape, disk)
machine-{family}-{variant} — one crate per machine (composes chips + memory + bus)
runtime-{family}           — generic runtime + variant type aliases + profile catalogue
```

Within that structure, each family carries:

- A small set of **shared hardware traits** in `common-{family}` (always `MemoryBus`-equivalent; sometimes a family-driver trait when ≥ 2 machines share cadence).
- One **generic `Runtime<M>`** in `runtime-{family}` exposing `MachineCore` to the cross-family shell.
- One **`profiles.rs` catalogue** with the `Model` enum and per-variant `MachineProfile` data.
- **Type aliases per variant** (`Spectrum128kRuntime = SpectrumRuntime<Spectrum128K>`, etc.).

The cross-family contract — `MachineCore`, `HostIo`, `MediaSet`, `FirmwareSet`, `MachineProfile`, `SessionQueryProvider` — is the **stable boundary** above this. Nothing about the pattern below changes how the shell talks to a machine; it just makes adding the second, third, and Nth machine in a family cheap.

## Why

The Spectrum family proved this empirically. Eleven profiles run through seven machine crates through one generic runtime. Adding the second machine cost ~70 LOC of trait glue plus a one-line type alias. Each subsequent variant cost the same. Architectural cleanups (moving `KeyboardMatrix` to `common`, swapping in a `SpeakerMixer`, defaulting `advance_halfcycles` on the driver) landed as local edits that touched every variant uniformly without bending the shape.

The Amiga family has one machine today (OCS), so it skips the generic runtime — the `runtime-commodore-amiga` wrapper goes straight to the AmigaOcs machine. When AGA / ECS land, the same lift will apply: extract a family-driver trait, generic the runtime, add variant aliases.

## The five pieces

### `common-{family}`

Hardware-only types and traits shared across every machine in the family. **No host-boundary types** — the runtime layer maps host events into hardware terms, not common.

For Spectrum: timing constants (`TIMING_48K`, `TIMING_128K`, …), `MemoryBus` trait, `SpectrumDriver` trait, `Bank16K`, `KeyboardMatrix` + `SpectrumKey`, `BeeperAudio` + `SpeakerMixer`, palette, snapshot helpers (`apply_z80_registers`, `apply_128k_bank_pages`, `apply_ay_registers`), tape player.

For NES: would hold timing constants, the master-clock topology, `MemoryBus` over the cartridge bus, `Mapper` trait, palette, controller-state helpers.

For Game Boy: timing for DMG/CGB, `MemoryBus`, audio mixer (`SquareChannel` × 2, `WaveChannel`, `NoiseChannel`), tile-decode helpers, joypad matrix.

### Chip crates

One crate per real silicon part. `zilog-z80`, `mos-6502`, `sharp-lr35902`, `mos-vic-ii`, `ricoh-2c02`, `ferranti-ula-6c001e`, `nec-upd765a`. Cycle-accurate from day one — see [Half-cycle signals](half-cycle-signals.md) and [CPU bus interface](cpu-bus-interface.md). **Reuse across families is real**: the `mos-6502` covers C64 + NES + BBC + Atari + Apple II + …; the `gi-ay-3-8912` covers Spectrum 128K-family + MSX + CPC; the `zilog-z80` covers Spectrum + MSX + CPC + Game Gear + Master System.

### Format crates

`format-sinclair-zx-spectrum-z80`, `format-sinclair-zx-spectrum-tap`, `format-sinclair-zx-spectrum-tzx`, `format-amstrad-dsk`, `format-nintendo-nes-ines`. Pure parsers / encoders. Don't reach into machine state.

### Machine crates

One crate per machine. Composes a CPU, video chip, sound chip, memory map, bus dispatch, port decode, peripheral wiring. Implements the family driver trait if there is one. The machine struct derives `Serialize`/`Deserialize` for whole-state snapshots.

### `runtime-{family}`

Three-file shape:

- `runtime-{family}/src/{family}_runtime.rs` — defines the family `Machine` trait (`fn run_frame`, `fn framebuffer`, etc.) and the generic `Runtime<M>` that implements `MachineCore`.
- `runtime-{family}/src/variants.rs` — `impl Machine for {Variant}` blocks plus `pub type {Variant}Runtime = Runtime<{Variant}>;` aliases.
- `runtime-{family}/src/profiles.rs` — `Model` enum + `profile_for(model)` + `profiles()` catalogue.

Plus per-machine constructor helpers (`from_firmware`, `from_rom_bytes`) hung off type aliases via inherent impls. Plus any rich query providers (e.g. Spectrum's ROM-glyph text extraction) in their own module.

## When the family driver trait pays for itself

Spectrum needed `SpectrumDriver` because seven machines shared the same per-frame cadence (ULA half-cycle tick + CPU clock gate + 3.5 MHz T-state hooks). Without it, every cadence fix had to land in seven places.

The trait pays for itself when:

- ≥ 2 machines in the family share the same run-loop shape
- The variation between machines is in chip composition (which chips tick when), not loop structure

It does **not** pay for itself when:

- Only one machine in the family exists yet
- Machines genuinely have different timing models (Atari 2600 beam-racing vs. ZX80 NMI-driven display would never share a driver, even though both are "Z80 / 6502 family")
- The "shared" loop body would need so many hooks that it's harder to read than the per-machine code

When in doubt, write per-machine first; lift to a trait when the second machine arrives and the duplication is real. The Amiga is currently in this state (one machine, no driver trait). When AGA and ECS land, the lift will be local.

## Snapshot helpers are family-scoped

`apply_z80_registers` / `apply_128k_bank_pages` / `apply_ay_registers` work because `.z80` / `.sna` are Spectrum formats. Game Boy's `.sna` (Game Boy DataDel) is unrelated. NES doesn't really have a community snapshot format. Spectrum Next's `.NEX` format is its own thing.

Each family owns its snapshot helpers. They live in `common-{family}::snapshot`. Don't try to make them generic.

## Generic `Runtime<M>` is family-specific

The shape is the same — trait + generic struct + variant impls + aliases — but the trait's hook surface and the runtime's `run_until` body are family-shaped. Spectrum's `SpectrumMachine` knows about beeper audio, tape streams, Spectrum keyboard rows, and a single 16-bit framebuffer. A NES `NesMachine` would expose two-controller state, NTSC/PAL frame timing, and an APU audio mixer.

You can't reuse the trait body across families. You **can** reuse the structural pattern (three files, generic over a family trait, type aliases per variant). That's what the next family copies.

## Profile catalogue belongs in `profiles.rs`, not `lib.rs`

Spectrum ran into this: `runtime-sinclair-zx-spectrum/src/lib.rs` was 1,147 LOC, of which ~800 was per-variant `MachineProfile { … }` data. After the split, `lib.rs` is 28 LOC of module decls and re-exports. The actual public API is readable at a glance. Apply the same split to every family from day one.

## Adding a new family — concrete steps

For Game Boy (full implementation in archive) or Sega SG-1000 (archived):

1. **Read the archive first.** Apply [archive-port methodology](archive-port-methodology.md) — three phases: characterise, port-with-tests, integrate. The archived code is the source of truth for what the hardware does; the port lifts it into the fresh-workspace shape.
2. **Create the chip crates.** Look up which CPU + chips the family uses. Reuse what already exists (`zilog-z80`, `mos-6502`). Write fresh crates for new chips (`sharp-lr35902` for the Game Boy CPU, `texas-tms9918` for the SG-1000 video chip). Cycle-accurate from day one.
3. **Create `common-{family}`.** Lift in shared timing constants, the `MemoryBus` trait (if memory shape is shared across machines), palette, audio helpers. Don't create a family-driver trait yet.
4. **Create the first `machine-{family}` crate.** Compose the chips. Write `pub fn run_frame()` directly on the machine. Get it booting.
5. **Create `runtime-{family}`.** One bespoke runtime wrapping the one machine, implementing `MachineCore`. Add the `Model` enum + `profile_for` + `profiles()` in `profiles.rs` (even with one entry — it's the right shape). Wire to the cross-system shell catalogue.
6. **When the second machine arrives** (Game Boy Color after DMG, SG-1000 II after SG-1000), lift:
   - Move shared loop into a `{Family}Driver` trait in `common-{family}`
   - Extract a `{Family}Machine` trait in `runtime-{family}/src/{family}_runtime.rs`
   - Make the runtime generic
   - Add a type alias for the new variant

The lift in step 6 is small (~hours, not days) because the structural pieces are already in place. That's the test of whether the family pattern is set up right.

## Drift triggers

- **"This family only has one machine, so I'll skip the runtime crate."** The runtime crate is where `MachineCore` lives. The shell can't reach the machine without it. Always create `runtime-{family}` even for single-machine families.
- **"I'll put the profile data in `lib.rs` for now."** It will sit there for years. Spectrum proved 800-line profile blocks hide the public API. Start with `profiles.rs` from day one.
- **"This new chip is only used by one machine, so I'll inline it."** Chip-per-crate from day one. The next system you add probably uses the same chip — the AY-3-8912 is in Spectrum 128K + MSX + CPC + Apple Mockingboard. The 6502 is everywhere. Discover the reuse later, not refactor for it later.
- **"Let me make the family driver trait now even though there's only one machine."** Per [SpectrumDriver](spectrum-driver.md), the trait is justified by ≥ 2 machines sharing cadence. With one machine, the trait is speculation. Wait.
- **"I'll generalise the snapshot helpers across families."** Spectrum's `.z80` helpers don't apply to Game Boy `.sna`. Each family's snapshot world is its own.
- **"I'll let `common-{family}` depend on `emu198x-shell` for `InputEvent`."** No. `common` is hardware-only. The runtime layer maps host events into hardware terms. (Spectrum learned this and reverted the dep.)
- **"I'll make `Runtime<M>` cross-family."** No. It's family-shaped; the per-family hook surface differs (audio shape, controller shape, framebuffer shape, snapshot shape). The cross-family contract is `MachineCore`, not the runtime body. See [System-specific run loops](system-specific-run-loops.md).

## What this is *not*

This isn't a universal pattern that applies to every project structure decision. It's specifically the within-family shape that emerged once the cross-family contract was stable enough to support it. For one-off chip crates, format parsers, or shell-level concerns, ignore this entirely.

## Related decisions

- [System-specific run loops](system-specific-run-loops.md) — why the run loop is per-system, not universal
- [SpectrumDriver](spectrum-driver.md) — within the Spectrum family, one shared driver trait
- [Crate naming](crate-naming.md) — `manufacturer-chipname` for chip crates, `format-{family}-{format}` for parsers
- [Save state format](save-state-format.md) — postcard + serde, derive on everything from day one
- [Archive-port methodology](archive-port-methodology.md) — three-phase port discipline for lifting archived code into the fresh workspace

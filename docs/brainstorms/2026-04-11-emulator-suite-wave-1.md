# Emulator Suite Wave 1

**Status:** Draft
**Date:** 2026-04-11

## Goal

Define a first implementation wave that is broad enough to prove the suite architecture, but narrow enough to avoid dragging `D`-bucket systems and modern platform complexity into the foundation.

Wave 1 should prove four execution styles:

- `T` tick-driven
- `S` slot/contention
- `B` beam-driven
- `H` DMA/arbiter hybrid

Wave 1 should not try to prove `D` dynarec+events yet.

The machine-readable target list is in [emulator-suite-wave-1.csv](./emulator-suite-wave-1.csv).

Related planning files:

- [Wave 1 milestones](./2026-04-11-emulator-suite-wave-1-milestones.md)
- [Wave 1 ticket list](./emulator-suite-wave-1-tickets.csv)
- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x commercial technique taxonomy](./2026-04-12-code-like-198x-commercial-technique-taxonomy.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)
- [Waves 2-6 roadmap](./2026-04-11-emulator-suite-roadmap-waves-2-6.md)

## Shipping Model

- The suite ships one binary per system family.
- Shared crates and tooling cover media loading, pacing, validation, tracing, save states, and host I/O where reuse is real.
- Nothing in this plan assumes one umbrella executable or one monolithic host runtime.

## Success Criteria

- Each selected family ships as its own binary or family-local executable target.
- Each system keeps its own scheduler policy internally.
- Media loading, input, video, audio, save states, and tracing go through shared support crates or conventions where reuse is real.
- No system is forced through a generic CPU, bus, PPU, or mapper abstraction.
- The first wave reaches one clean, testable profile per family before variants and add-ons expand.

## Non-Goals

- No late-1990s or 2000s 3D console work in wave 1.
- No universal event heap forced on the simple machines.
- No CD add-ons in wave 1 unless the base target explicitly needs them.
- No family-complete support matrix in wave 1.
- No attempt to unify cartridge boards, floppy controllers, DMA engines, or video chips across unrelated families.

## Target Set

### Wave 1A: Foundation

These systems define the baseline shared support layer and per-family binary shape.

1. `gameboy_family`
   - Start with `DMG-01`.
   - Defer `CGB`, `SGB`, and unusual MBCs.
   - Reason: clean `T` reference machine and already a good architectural template.
2. `sega_8bit_family`
   - Start with `SG-1000` or `Master System 1`.
   - Defer `Game Gear`, FM audio, and the broader peripheral matrix.
   - Reason: validates a reusable `Z80 + VDP + PSG` style family without heavy contention.
3. `spectrum_family`
   - Start with `48K`, then `128K` once the baseline is stable.
   - Defer disk-drive families and clone-specific timing.
   - Reason: canonical `S` machine for CPU/video contention and tape-era computer input.
4. `famicom_nes_family`
   - Start with `NES/Famicom` plus a small mapper set: `NROM`, `UxROM`, `CNROM`, `MMC1`.
   - Defer `MMC3`, `VRC`, `FDS`, and expansion audio.
   - Reason: validates the cartridge-subsystem story and a more demanding slot-driven console.

### Wave 1B: Stress

These systems force the shared support layer to handle cases that are still retro-core, but no longer simple.

1. `commodore_64_128_family`
   - Start with `C64 PAL breadbin`.
   - Defer `C128`, REU, fast loaders, and full drive emulation if a simpler loader path is available first.
   - Reason: flagship `S` machine for bus steals, raster effects, and precise video/CPU interaction.
2. `atari_2600_family`
   - Start with `VCS` and a minimal bankswitch set such as `2K`, `4K`, `F8`.
   - Defer unusual controller hardware and rare mapper schemes.
   - Reason: proves the suite can host a true `B` machine without corrupting the simpler execution styles.

### Wave 1C: Capstone

These systems confirm that the shared support layer can scale into hybrid machines without becoming a dynarec-era framework.

1. `pc_engine_family`
   - Start with `CoreGrafx / base HuCard`.
   - Defer `CD-ROM`, `SuperGrafx`, and multi-system shells.
   - Reason: a relatively clean console-side `H` target.
2. `amiga_ocs_ecs_family`
   - Start with `Amiga 500 OCS`, `Kickstart`, and floppy boot.
   - Defer `ECS`, `AGA`, hard disks, accelerators, and expansion-heavy configs.
   - Reason: capstone `H` machine for DMA arbitration, Copper/Blitter behavior, and computer-style I/O.

## Why These Families

- They cover all four execution-policy shapes that matter before `D`.
- They include both console and computer targets.
- They include both cartridge and tape/floppy media.
- They include the systems already most likely to surface bad abstractions early: `Spectrum`, `NES`, `C64`, `Atari 2600`, `Amiga`.
- They avoid jumping into 3D, dynarec, GPU translation, or OS-heavy platforms before the shared support layer is stable.

## Shared Support Abstractions

These should be shared across the suite where they materially reduce duplicated host and tooling work. They do not imply one universal launcher or one umbrella executable.

### 1. Machine Identity

- `MachineId`
- `ModelId`
- `RegionId`
- `MachineProfile`

Purpose:

- choose the concrete machine profile at startup inside a family binary or shared harness
- bind default clocks, ROM requirements, input layout, and media slots
- keep family variants out of ad-hoc booleans

### 2. Time

- `MachineTime` as a monotonic `u64`
- `ClockDesc` that records the machine's authoritative unit and frequency
- `run_until(target_time)` as the common contract

Purpose:

- share debugger, tracing, save-state, and pacing infrastructure across binaries
- let each machine keep its own scheduler policy behind one outer contract

### 3. Media

- `MediaSet`
- `MediaSlot`
- `MediaImage`
- `BootRomPolicy`

Purpose:

- support cartridge, cassette, floppy, and firmware loading without creating one fake universal cartridge API
- keep media ownership, hashing, persistence, and metadata shared

### 4. Input

- `InputPortId`
- `InputState`
- machine-specific input adapters that map host input to machine lines

Purpose:

- share frontend input plumbing
- keep keyboard matrices, joystick ports, paddles, and console pads machine-local

### 5. Video

- `FrameSink`
- `FrameFormat`
- optional scanline or beam event sink for machines that need it

Purpose:

- share output transport and capture tools
- avoid forcing every machine to produce the same internal rendering model

### 6. Audio

- `AudioSink`
- `AudioFormat`
- timestamped audio packet emission

Purpose:

- share host audio plumbing, recording, and resampling boundaries
- let each core decide how and when to synthesize samples

### 7. Save State

- `SnapshotFormatVersion`
- `snapshot()`
- `restore()`

Purpose:

- standardize persistence and compatibility checks
- keep the contents machine-specific

### 8. Trace and Debug

- `TraceSink`
- `DebugProbe`
- timestamped events with machine-local payloads

Purpose:

- share logging, trace capture, and debugger UI
- avoid designing a fake universal CPU trace schema before it is needed

### 9. Validation Harness

- test ROM runner
- screenshot / framebuffer hash support
- audio sample capture
- scripted input playback

Purpose:

- make cross-system regression testing part of the substrate, not an afterthought

## Rust-Facing Shape

This is the level of shared interface that is worth enforcing early across binaries, harnesses, and shared support crates:

```rust
trait MachineCore {
    fn identity(&self) -> MachineProfile;
    fn reset(&mut self, kind: ResetKind);
    fn load_media(&mut self, media: &MediaSet) -> Result<(), MachineError>;
    fn run_until(&mut self, target: MachineTime, io: &mut HostIo) -> RunResult;
    fn snapshot(&self) -> Result<Vec<u8>, MachineError>;
    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError>;
}
```

`HostIo` should expose:

- current host input state
- frame sink
- audio sink
- trace/debug sink
- host services like file callbacks only if a given machine actually needs them

That is enough shared structure for the suite. It is a contract between cores, family binaries, and tooling. It is not a mandate to put every family under one executable or to make chips, buses, and media controllers generic.

## Do Not Abstract Yet

These should stay machine-local until repeated concrete implementations prove otherwise:

- CPU execution traits shared across unrelated processors
- one generic address bus trait for every system
- one generic `Ppu` or `VideoChip` trait
- one generic `Mapper` trait across all cartridge systems
- one global event queue used by every scheduler bucket
- one universal disk/tape API inside the core
- one universal interrupt controller model

If two systems later converge naturally, extract the common piece then.

## Suggested Module Shape

For a Rust suite, a pragmatic split would look like:

- `crates/support/time/...`
- `crates/support/media/...`
- `crates/support/input/...`
- `crates/support/video/...`
- `crates/support/audio/...`
- `crates/support/state/...`
- `crates/support/trace/...`
- `crates/support/validation/...`
- `systems/gameboy_core/...`
- `systems/sega8_core/...`
- `systems/spectrum_core/...`
- `systems/nes_core/...`
- `systems/c64_core/...`
- `systems/atari2600_core/...`
- `systems/pce_core/...`
- `systems/amiga_core/...`
- `bin/gameboy/...`
- `bin/sega8/...`
- `bin/spectrum/...`
- `bin/nes/...`
- `bin/c64/...`
- `bin/atari2600/...`
- `bin/pce/...`
- `bin/amiga/...`

Shared support crates own the host-facing contracts and reusable tooling surfaces.

Each family binary and core own:

- startup flow and profile selection inside the family shell
- scheduler policy
- chips
- memory map
- bus arbitration
- media semantics
- machine-specific tests

## Likely Wave 2

If wave 1 lands cleanly, the next systems to add are the ones that mostly extend an existing family or bucket:

- `Game Boy Color`
- `Game Gear`
- `C128`
- `MSX1/2`
- `Apple IIe`
- `Atari 800XL`
- `Mega Drive / Genesis`
- `SNES`

Those are better second-wave targets than jumping directly to `PlayStation`, `Saturn`, `N64`, or `PS2`.

# Emulator Suite Wave 1 Milestones

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Turn the wave 1 target set into implementation milestones and ticket-sized work items.

This file is the human-readable plan.

The machine-readable ticket list is in [emulator-suite-wave-1-tickets.csv](./emulator-suite-wave-1-tickets.csv).

These milestones assume one binary per system family. `shared_support` tickets describe reusable crates, harnesses, and host tooling, not a monolithic launcher.

## Ticket Format

Each ticket has:

- a stable ID
- one owning family or `shared_support`
- one primary subsystem
- explicit dependencies
- a short "done when" definition

The tickets are deliberately flat. If a ticket starts hiding several weeks of work, it should split.

## Milestone Map

- `M0` Shared support substrate
- `M1` Tick-family bring-up
- `M2` Contention-family foundation
- `M3` Stress timing and beam systems
- `M4` Hybrid console
- `M5` Hybrid computer capstone
- `M6` Wave 1 hardening

## M0 Shared Support Substrate

Goal:

- establish the shared support contracts before multiple systems start pulling them in different directions

Tickets:

- `RT-001` Machine profile registry
  - Subsystem: identity
  - Done when: family binaries and shared tooling can select a machine by `MachineId` and `ModelId`, and each selected profile declares clocks, firmware requirements, media slots, and default input layout.
- `RT-002` Time contract
  - Subsystem: time
  - Depends on: `RT-001`
  - Done when: `MachineTime`, `ClockDesc`, and `run_until(target_time)` exist and one headless harness can drive a core without knowing its scheduler internals.
- `RT-003` Media model
  - Subsystem: media
  - Depends on: `RT-001`
  - Done when: cartridge, tape, floppy, and firmware images can be described uniformly as host-owned media objects without forcing a shared in-core media API.
- `RT-004` Video sink
  - Subsystem: video
  - Depends on: `RT-002`
  - Done when: a core can emit completed frames, and beam/scanline-oriented machines have an optional lower-level event sink.
- `RT-005` Audio sink
  - Subsystem: audio
  - Depends on: `RT-002`
  - Done when: cores can emit timestamped audio packets into a shared host sink.
- `RT-006` Input model
  - Subsystem: input
  - Depends on: `RT-001`
  - Done when: host input can be sampled and mapped into machine-local input adapters without shared keyboard matrix or controller abstractions.
- `RT-007` Snapshot contract
  - Subsystem: state
  - Depends on: `RT-001`, `RT-002`, `RT-003`
  - Done when: a core can serialize and restore a versioned snapshot blob under one shared envelope format.
- `RT-008` Trace and validation harness
  - Subsystem: trace and validation
  - Depends on: `RT-002`, `RT-004`, `RT-005`, `RT-006`
  - Done when: scripted input playback, framebuffer capture, audio capture, and timestamped trace events all exist in one reusable harness.

## M1 Tick-Family Bring-Up

Goal:

- prove the shared support layer on machines that should remain simple

Families:

- `gameboy_family`
- `sega_8bit_family`

Tickets:

- `GB-001` DMG profile and media path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: `DMG-01` can boot from configured boot ROM policy and cartridge media in its family binary through the shared media-loading support.
- `GB-002` SM83 and interrupt baseline
  - Subsystem: CPU
  - Depends on: `GB-001`, `RT-002`
  - Done when: CPU fetch/execute, interrupts, and reset sequencing run under the shared support contract.
- `GB-003` DMG memory, timer, joypad, serial stub
  - Subsystem: bus and I/O
  - Depends on: `GB-002`
  - Done when: baseline memory map, timer behavior, joypad reads, and a non-blocking serial placeholder exist.
- `GB-004` PPU to frame sink
  - Subsystem: video
  - Depends on: `GB-003`, `RT-004`
  - Done when: visible frame output, VBlank signaling, and framebuffer capture work through the shared sink.
- `GB-005` APU and validation pass
  - Subsystem: audio and validation
  - Depends on: `GB-003`, `RT-005`, `RT-008`
  - Done when: audio packets are emitted through the shared audio sink and the baseline validation set passes in the shared harness.

- `S8-001` SG-1000 profile and cartridge path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: `SG-1000` is pinned as the initial Sega 8-bit profile and cartridges load through the shared media-loading support.
- `S8-002` Z80 integration and memory map
  - Subsystem: CPU
  - Depends on: `S8-001`, `RT-002`
  - Done when: CPU execution, memory mapping, and interrupt entry run under the shared support contract.
- `S8-003` VDP baseline
  - Subsystem: video
  - Depends on: `S8-002`, `RT-004`
  - Done when: tile/sprite display reaches the frame sink and a first screenshot-based regression set exists.
- `S8-004` PSG, controllers, and pause/input path
  - Subsystem: audio and input
  - Depends on: `S8-002`, `RT-005`, `RT-006`
  - Done when: controller state and audio emission work through shared sinks without platform-local host hacks.
- `S8-005` Validation and Master System delta backlog
  - Subsystem: validation
  - Depends on: `S8-003`, `S8-004`, `RT-008`
  - Done when: SG-1000 baseline tests run in the shared harness and the `Master System 1` delta is captured as explicit follow-up tickets rather than assumptions.

## M2 Contention-Family Foundation

Goal:

- force the shared support layer to survive visible timing and cartridge complexity without over-abstracting

Families:

- `spectrum_family`
- `famicom_nes_family`

Tickets:

- `SP-001` Spectrum 48K profile, ROM, and tape path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: `Spectrum 48K` boots its ROM and can consume a basic tape image path through shared media plumbing.
- `SP-002` Z80 plus ULA contention
  - Subsystem: CPU and bus
  - Depends on: `SP-001`, `RT-002`
  - Done when: CPU execution observes ULA-visible contention rules rather than a pure unconstrained tick loop.
- `SP-003` Bitmap video, keyboard matrix, and beeper
  - Subsystem: video, input, audio
  - Depends on: `SP-002`, `RT-004`, `RT-005`, `RT-006`
  - Done when: keyboard input, beeper output, and visible display all flow through the shared support interfaces.
- `SP-004` Tape timing and validation
  - Subsystem: media timing and validation
  - Depends on: `SP-003`, `RT-008`
  - Done when: at least one deterministic tape-load validation case passes in the shared harness.
- `SP-005` Spectrum 128K extension
  - Subsystem: paging and audio
  - Depends on: `SP-004`
  - Done when: the 128K paging model and AY audio path exist as a second supported profile within the same family shell.

- `NES-001` Front-loader NTSC profile and mapper-limited cartridge path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: an NTSC `NES/Famicom` baseline profile loads `NROM`, `UxROM`, `CNROM`, and `MMC1` media through one cartridge path.
- `NES-002` 2A03 CPU, interrupts, and DMA foundation
  - Subsystem: CPU and bus
  - Depends on: `NES-001`, `RT-002`
  - Done when: CPU execution, interrupts, OAM DMA, and core bus timing run under the shared support contract.
- `NES-003` PPU baseline to frame sink
  - Subsystem: video
  - Depends on: `NES-002`, `RT-004`
  - Done when: background/sprite output, VBlank timing, and framebuffer capture work through the common video path.
- `NES-004` APU and controller path
  - Subsystem: audio and input
  - Depends on: `NES-002`, `RT-005`, `RT-006`
  - Done when: baseline APU channels and controller polling reach the shared support sinks.
- `NES-005` Mapper and validation pass
  - Subsystem: cartridge and validation
  - Depends on: `NES-003`, `NES-004`, `RT-008`
  - Done when: the wave-1 mapper set passes a shared regression harness and no generic cross-system mapper API has leaked into the architecture.

## M3 Stress Timing And Beam Systems

Goal:

- prove the shared support layer can serve machines that most quickly punish sloppy timing models

Families:

- `commodore_64_128_family`
- `atari_2600_family`

Tickets:

- `C64-001` C64 PAL profile and media path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: a PAL `C64` profile loads cartridge and one practical program-loading path through the shared media-loading support.
- `C64-002` 6510, banking, CIA baseline
  - Subsystem: CPU and I/O
  - Depends on: `C64-001`, `RT-002`
  - Done when: CPU execution, banking, timers, and keyboard/joystick-visible CIA behavior exist.
- `C64-003` VIC-II badlines, raster IRQs, and frame output
  - Subsystem: video
  - Depends on: `C64-002`, `RT-004`
  - Done when: badlines, raster IRQ timing, and visible output all operate through the shared video contract.
- `C64-004` SID and input
  - Subsystem: audio and input
  - Depends on: `C64-002`, `RT-005`, `RT-006`
  - Done when: SID output and keyboard/joystick input reach the shared sinks without family-specific frontend hacks.
- `C64-005` Validation pass
  - Subsystem: validation
  - Depends on: `C64-003`, `C64-004`, `RT-008`
  - Done when: a first raster- and timing-sensitive regression set passes under the shared harness.

- `A26-001` Atari 2600 NTSC profile and minimal bankswitch path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: `2K`, `4K`, and `F8` cartridges load through the shared media-loading support.
- `A26-002` 6507 and RIOT baseline
  - Subsystem: CPU and I/O
  - Depends on: `A26-001`, `RT-002`
  - Done when: CPU execution, RIOT timer behavior, and input-visible state run under the common time contract.
- `A26-003` TIA beam, collisions, and audio
  - Subsystem: video and audio
  - Depends on: `A26-002`, `RT-004`, `RT-005`
  - Done when: beam-driven rendering, collision state, and audio emission all operate through shared sinks.
- `A26-004` Controller set and validation
  - Subsystem: input and validation
  - Depends on: `A26-003`, `RT-006`, `RT-008`
  - Done when: joystick baseline input and one deterministic validation set run under the common harness.

## M4 Hybrid Console

Goal:

- add one console-side hybrid machine before the Amiga capstone

Family:

- `pc_engine_family`

Tickets:

- `PCE-001` CoreGrafx profile and HuCard media path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: `CoreGrafx` is the pinned wave-1 profile and HuCards load through the shared media-loading support.
- `PCE-002` HuC6280, memory map, timer, and IRQ baseline
  - Subsystem: CPU and bus
  - Depends on: `PCE-001`, `RT-002`
  - Done when: CPU execution, timing, interrupts, and baseline bank mapping run correctly under the common time contract.
- `PCE-003` VDC/VCE video path
  - Subsystem: video
  - Depends on: `PCE-002`, `RT-004`
  - Done when: visible output reaches the shared frame sink with timing-aware scanline behavior.
- `PCE-004` PSG and controller path
  - Subsystem: audio and input
  - Depends on: `PCE-002`, `RT-005`, `RT-006`
  - Done when: audio packets and controller state run through shared support plumbing.
- `PCE-005` Validation pass
  - Subsystem: validation
  - Depends on: `PCE-003`, `PCE-004`, `RT-008`
  - Done when: a first regression set passes and all CD-related work remains explicitly deferred.

## M5 Hybrid Computer Capstone

Goal:

- prove that the shared support layer can serve a machine where DMA arbitration matters as much as the CPU

Family:

- `amiga_ocs_ecs_family`

Tickets:

- `AMI-001` Amiga 500 OCS PAL profile, Kickstart, and floppy path
  - Subsystem: identity and media
  - Depends on: `RT-001`, `RT-003`
  - Done when: an `Amiga 500 OCS PAL` profile can boot with declared firmware and floppy media through the shared host support layer.
- `AMI-002` 68000, memory map, and interrupt foundation
  - Subsystem: CPU and bus
  - Depends on: `AMI-001`, `RT-002`
  - Done when: CPU execution, exceptions, and baseline memory behavior operate under the shared time contract.
- `AMI-003` DMA scheduler skeleton
  - Subsystem: scheduler and arbitration
  - Depends on: `AMI-002`
  - Done when: the Amiga core has an explicit DMA/arbiter scheduling model rather than a CPU-centric bus loop.
- `AMI-004` Agnus/Denise video path with Copper and bitplanes
  - Subsystem: video
  - Depends on: `AMI-003`, `RT-004`
  - Done when: visible display, Copper effects, and bitplane output reach the shared frame sink.
- `AMI-005` Paula audio, CIA, keyboard, and floppy timing
  - Subsystem: audio and I/O
  - Depends on: `AMI-003`, `RT-005`, `RT-006`
  - Done when: audio output and baseline machine input/peripheral timing operate through shared sinks and adapters.
- `AMI-006` Blitter and validation pass
  - Subsystem: custom chips and validation
  - Depends on: `AMI-004`, `AMI-005`, `RT-008`
  - Done when: one deterministic boot/demo validation set passes under the shared harness.

## M6 Wave 1 Hardening

Goal:

- convert "it boots" into "it is supportable"

Tickets:

- `INT-001` Shared tooling and startup conventions
  - Subsystem: host integration
  - Depends on: `M1`, `M2`
  - Done when: completed wave-1 binaries share startup conventions and can be driven by common tooling without a monolithic launcher.
- `INT-002` Snapshot and restore smoke tests across wave 1
  - Subsystem: validation
  - Depends on: `RT-007`, `M3`, `M4`, `M5`
  - Done when: every completed wave-1 family has at least one automated save-state round-trip test.
- `INT-003` Trace and regression matrix
  - Subsystem: validation and tooling
  - Depends on: `RT-008`, `M3`, `M4`, `M5`
  - Done when: each completed family participates in one shared regression matrix with artifacts produced in a uniform format.

## Sequencing Notes

- `Game Boy` first because it is the cleanest reference for the shared support shape.
- `Sega 8-bit` second because it exercises a second `T` family without immediately dragging in contention.
- `Spectrum` before `NES` because it surfaces host input, tape, and visible contention with less cartridge complexity.
- `C64` before `Atari 2600` would be a mistake if beam support is still speculative; the current order assumes `Atari 2600` lands while the shared support layer is still malleable.
- `PC Engine` exists to prove a manageable `H` machine before the `Amiga`.
- `Amiga` is intentionally late because it will punish weak scheduler assumptions more than any other wave-1 target.

## What To Avoid During Implementation

- Do not pull `Game Gear`, `FDS`, `C128`, `SuperGrafx`, `AGA`, or `CGB` into wave-1 tickets unless a blocking architectural issue genuinely requires it.
- Do not extract shared chip traits just because two systems happen to use a `Z80`, `6502`, or similar PSG family.
- Do not create one global scheduler abstraction more specific than `run_until(target_time)` until at least one `B`, one `S`, and one `H` machine exist in code.

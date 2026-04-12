# Emulator Suite Roadmap: Waves 2-6 Milestones

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Break the later roadmap into milestone groups and ticket-sized epics.

This file is the human-readable milestone plan.

The machine-readable ticket list is in [emulator-suite-roadmap-waves-2-6-tickets.csv](./emulator-suite-roadmap-waves-2-6-tickets.csv).

These milestones assume one binary per system family. `shared_support` tickets describe reusable crates, harnesses, and host tooling; they do not imply one umbrella executable.

## Ticket Size

Unlike wave 1, these tickets are roadmap epics.

They are meant to answer:

- what gets built in which wave
- what depends on what
- what "done" roughly means

When a later wave becomes active, these tickets should be split into smaller implementation tickets.

## Milestone Map

- `M7` Shared support variant and persistence expansion
- `M8` Wave-1 family expansions A
- `M9` Wave-1 family expansions B
- `M10` Adjacent handheld cluster
- `M11` Wave-2 hardening
- `M12` 8-bit computer cluster A
- `M13` 8-bit computer cluster B
- `M14` Console and vector cluster
- `M15` 16-bit console cluster
- `M16` 16-bit computer cluster
- `M17` Wave-3 hardening
- `M18` Advanced computer cluster A
- `M19` Advanced computer cluster B
- `M20` Add-on and late-hybrid cluster
- `M21` Wave-4 hardening
- `M22` D execution substrate
- `M23` First D consoles
- `M24` Second D consoles
- `M25` Handheld D cluster
- `M26` Upper-bound D cluster
- `M27` Wave-5 hardening
- `M28` Catalog framework and triage
- `M29` Early-console and handheld oddities
- `M30` Long-tail microcomputers
- `M31` Multimedia and edge consoles
- `M32` Boundary platforms
- `M33` Wave-6 archive hardening

## Wave 2

### M7 Shared Support Variant And Persistence Expansion

Goal:

- teach the shared support layer how to serve multiple profiles, save-backed media, and richer removable-media behavior without changing the wave-1 core contract

Tickets:

- `W2-RT-001` Multi-profile family registry
  - Depends on: `INT-003`
  - Done when: one family can expose several concrete profiles with declarative capability flags instead of ad-hoc conditionals.
- `W2-RT-002` Persistent media and battery-backed storage
  - Depends on: `RT-003`, `INT-003`
  - Done when: cartridges, disks, and similar media can persist mutable state through one shared host path.
- `W2-RT-003` RTC and wall-clock service layer
  - Depends on: `W2-RT-002`
  - Done when: machine-local RTC devices consume a shared host time service without leaking host APIs into core logic.
- `W2-RT-004` Removable disk and tape host services
  - Depends on: `W2-RT-001`, `W2-RT-002`
  - Done when: late wave-2 families can mount, swap, and persist removable media through shared support plumbing.

### M8 Wave-1 Family Expansions A

Goal:

- deepen the wave-1 handheld and contention-console families before the suite adds more flagship breadth

Tickets:

- `W2-GB-001` Game Boy Color and common MBC matrix
  - Depends on: `W2-RT-001`, `W2-RT-002`, `GB-005`
  - Done when: `CGB` and the common save-backed MBC families are supported under one shared Game Boy family shell.
- `W2-GB-002` Game Boy link and late-family validation
  - Depends on: `W2-GB-001`, `W2-RT-003`
  - Done when: the Game Boy family has a defined link/session story and a stronger multi-profile regression set.
- `W2-S8-001` Master System 1 baseline
  - Depends on: `W2-RT-001`, `S8-005`
  - Done when: `Master System 1` is supported as a first-class profile, not an implied SG-1000 variant.
- `W2-S8-002` Game Gear and FM/audio family variants
  - Depends on: `W2-S8-001`
  - Done when: `Game Gear` and the main FM/audio deltas are modeled within the Sega 8-bit family shell.
- `W2-SP-001` Spectrum late-family paging and AY path
  - Depends on: `SP-005`, `W2-RT-001`
  - Done when: `128K` and later paging/audio behavior are stabilized as multi-profile support rather than one-off hacks.
- `W2-SP-002` Spectrum +3 disk path and variant validation
  - Depends on: `W2-SP-001`, `W2-RT-004`
  - Done when: `+3`-class disk-backed variants are supported with shared removable-media plumbing.
- `W2-NES-001` MMC3 and scanline IRQ path
  - Depends on: `NES-005`
  - Done when: the suite supports a post-wave-1 mapper tier without inventing a generic mapper abstraction for all systems.
- `W2-NES-002` FDS, expansion audio, and profile split
  - Depends on: `W2-NES-001`, `W2-RT-004`
  - Done when: `FDS`, expansion audio, and explicit NTSC/PAL profile handling exist within the NES family.

### M9 Wave-1 Family Expansions B

Goal:

- deepen the computer and hybrid families that need richer media and chipset variation

Tickets:

- `W2-C64-001` C128 baseline profile and VDC path
  - Depends on: `C64-005`, `W2-RT-001`
  - Done when: `C128` is represented as a profile inside the Commodore 64/128 family rather than a separate ad-hoc machine.
- `W2-C64-002` IEC/1541 pragmatic disk workflow
  - Depends on: `W2-C64-001`, `W2-RT-004`
  - Done when: one practical and regression-friendly IEC/1541 path exists for the Commodore family.
- `W2-PCE-001` CD-ROM2 path
  - Depends on: `PCE-005`, `W2-RT-004`
  - Done when: the PC Engine family supports `CD-ROM2` media and timing without pulling in unrelated CD frameworks.
- `W2-PCE-002` SuperGrafx and family variant matrix
  - Depends on: `W2-PCE-001`, `W2-RT-001`
  - Done when: `SuperGrafx` and other major profile deltas are modeled cleanly inside the family shell.
- `W2-AMI-001` ECS profiles and chipset deltas
  - Depends on: `AMI-006`, `W2-RT-001`
  - Done when: `ECS`, `A500+`, and `A600`-class profiles exist with explicit chipset and platform deltas.
- `W2-AMI-002` Workbench/floppy hardening
  - Depends on: `W2-AMI-001`, `W2-RT-004`
  - Done when: the Amiga family has a more durable floppy/workbench boot workflow suitable for later waves.

### M10 Adjacent Handheld Cluster

Goal:

- add handheld families that reuse the suite's handheld and tile-based strengths without requiring a new execution-substrate tier

Tickets:

- `W2-WS-001` WonderSwan family baseline
  - Depends on: `W2-RT-001`, `RT-008`
  - Done when: `WonderSwan`, `WonderSwan Color`, and `SwanCrystal` have a coherent family shell and one validated baseline profile.
- `W2-NGP-001` Neo Geo Pocket family baseline
  - Depends on: `W2-RT-001`, `RT-008`
  - Done when: the Neo Geo Pocket family has one validated baseline profile and a clear family-variant model.
- `W2-LYN-001` Atari Lynx family baseline
  - Depends on: `W2-RT-001`, `RT-008`
  - Done when: the Lynx family is onboarded as a hybrid handheld without destabilizing the simpler handheld abstractions.
- `W2-GBA-001` Game Boy Advance family baseline
  - Depends on: `W2-RT-001`, `RT-008`
  - Done when: the GBA family is onboarded as the first ARM-era handheld within the non-`D` roadmap.

### M11 Wave-2 Hardening

Goal:

- make the expanded families supportable before the roadmap broadens further

Tickets:

- `W2-INT-001` Shared tooling conventions and persistence matrix
  - Depends on: `W2-RT-004`, `W2-GBA-001`
  - Done when: completed family binaries share startup conventions, profile selection behavior, and persistent-media handling across the expanded families.
- `W2-INT-002` Cross-family regression pass
  - Depends on: `W2-INT-001`
  - Done when: wave-2 families participate in one shared regression matrix with profile-aware artifacts.

## Wave 3

### M12 8-Bit Computer Cluster A

Goal:

- add the highest-value remaining 8-bit computer families with strong retro demand and clear architectural payoff

Tickets:

- `W3-A2-001` Apple IIe baseline
  - Depends on: `W2-INT-002`
  - Done when: `Apple IIe` boots, renders, accepts input, loads disk media, and participates in the shared validation harness.
- `W3-A8-001` Atari 800XL baseline
  - Depends on: `W2-INT-002`
  - Done when: `800XL` with ANTIC/GTIA/POKEY timing reaches the suite regression harness.
- `W3-MSX-001` MSX1 baseline
  - Depends on: `W2-INT-002`
  - Done when: `MSX1` boots and runs with an explicit slot model, cartridge media path, and shared validation artifacts.
- `W3-MSX-002` MSX2 extension
  - Depends on: `W3-MSX-001`
  - Done when: `MSX2` extends the MSX family without forcing a second incompatible architecture.

### M13 8-Bit Computer Cluster B

Goal:

- round out the mainstream 8-bit computer wave with the most historically important remaining families

Tickets:

- `W3-BBC-001` BBC Model B baseline
  - Depends on: `W2-INT-002`
  - Done when: the BBC Micro family has a validated `Model B` baseline.
- `W3-CPC-001` CPC 6128 baseline
  - Depends on: `W2-INT-002`
  - Done when: the CPC family has a validated `6128` baseline with tape/floppy-capable media support.
- `W3-COCO-001` CoCo 2 baseline
  - Depends on: `W2-INT-002`
  - Done when: the Color Computer family has a validated `CoCo 2` baseline.
- `W3-COCO-002` CoCo 3 extension
  - Depends on: `W3-COCO-001`
  - Done when: the `CoCo 3` extends the family with explicit GIME-era deltas.

### M14 Console And Vector Cluster

Goal:

- fill in the remaining mainstream early-console families, including the vector path

Tickets:

- `W3-CV-001` ColecoVision baseline
  - Depends on: `W2-INT-002`
  - Done when: the ColecoVision family reaches one validated baseline profile.
- `W3-INTV-001` Intellivision baseline
  - Depends on: `W2-INT-002`
  - Done when: the Intellivision family reaches one validated baseline profile.
- `W3-A78-001` Atari 7800 baseline
  - Depends on: `W2-INT-002`
  - Done when: the 7800 family reaches one validated baseline profile using the hybrid scheduler path.
- `W3-VEC-001` Vectrex baseline
  - Depends on: `W2-INT-002`
  - Done when: the suite hosts one validated vector-display family without introducing a second beam execution path.

### M15 16-Bit Console Cluster

Goal:

- add the major 16-bit console families still missing from the suite

Tickets:

- `W3-MD-001` Genesis cart-only baseline
  - Depends on: `W2-INT-002`
  - Done when: the Genesis family reaches a validated cart-only baseline profile.
- `W3-SNES-001` SNES baseline
  - Depends on: `W2-INT-002`
  - Done when: the SNES family reaches a validated baseline profile under the hybrid scheduler path.
- `W3-NEO-001` Neo Geo AES/MVS baseline
  - Depends on: `W2-INT-002`
  - Done when: the Neo Geo family reaches a validated cart-only baseline profile.

### M16 16-Bit Computer Cluster

Goal:

- add the most important remaining 16-bit computer family still aligned with the non-`D` roadmap

Tickets:

- `W3-ST-001` Atari ST baseline
  - Depends on: `W2-INT-002`
  - Done when: the Atari ST family reaches a validated baseline profile with shared media, audio, and video plumbing.

### M17 Wave-3 Hardening

Goal:

- stabilize the flagship-breadth wave before the roadmap moves into more schedule-sensitive territory

Tickets:

- `W3-INT-001` Flagship regression matrix
  - Depends on: `W3-ST-001`
  - Done when: wave-3 families join one uniform regression matrix with comparable artifacts and acceptance criteria.
- `W3-INT-002` Shared media import path across flagship computers and consoles
  - Depends on: `W3-INT-001`
  - Done when: the host media/import layer handles the wave-3 mix without per-family host-glue forks.

## Wave 4

### M18 Advanced Computer Cluster A

Goal:

- add advanced but still non-`D` computer families with custom chips and richer buses

Tickets:

- `W4-IIGS-001` Apple IIgs baseline
  - Depends on: `W3-INT-002`
  - Done when: the Apple IIgs family reaches one validated baseline profile.
- `W4-ARC-001` Archimedes baseline
  - Depends on: `W3-INT-002`
  - Done when: the Archimedes family reaches one validated baseline profile.
- `W4-RPC-001` Risc PC extension
  - Depends on: `W4-ARC-001`
  - Done when: the Risc PC extends the Acorn ARM family without creating a separate execution-substrate track.
- `W4-FAL-001` Atari Falcon baseline
  - Depends on: `W3-INT-002`
  - Done when: the Falcon reaches one validated baseline profile.

### M19 Advanced Computer Cluster B

Goal:

- bring in the highest-value Japanese and workstation-style computer families still short of `D`

Tickets:

- `W4-X68-001` X68000 baseline
  - Depends on: `W3-INT-002`
  - Done when: the X68000 family reaches one validated baseline profile.
- `W4-PC88-001` PC-8801 baseline
  - Depends on: `W3-INT-002`
  - Done when: the NEC PC-88/98 family reaches one practical baseline starting with `PC-8801`.
- `W4-FMT-001` FM Towns and Marty baseline
  - Depends on: `W3-INT-002`
  - Done when: the FM Towns family reaches one practical baseline profile.

### M20 Add-On And Late-Hybrid Cluster

Goal:

- add late-family add-ons and hybrids that are still below the first `D` boundary

Tickets:

- `W4-AMI-AGA-001` Amiga AGA and CD32 baseline
  - Depends on: `W2-AMI-002`
  - Done when: the Amiga family reaches one validated AGA-era baseline and one console-style AGA profile.
- `W4-MDCD-001` Mega-CD / Sega CD integration
  - Depends on: `W3-MD-001`, `W2-RT-004`
  - Done when: the Genesis family supports one CD-based baseline path.
- `W4-32X-001` 32X integration
  - Depends on: `W4-MDCD-001`
  - Done when: the Genesis family supports one 32X-era baseline path without spawning a second execution-substrate track.
- `W4-NEOCD-001` Neo Geo CD integration
  - Depends on: `W3-NEO-001`, `W2-RT-004`
  - Done when: the Neo Geo family supports one CD-based baseline path.
- `W4-VB-001` Virtual Boy baseline
  - Depends on: `W3-INT-002`
  - Done when: the Virtual Boy family reaches one validated baseline profile.
- `W4-JAG-001` Jaguar / Jaguar CD baseline
  - Depends on: `W3-INT-002`
  - Done when: the Jaguar family reaches one practical baseline profile and a clear CD-era backlog.

### M21 Wave-4 Hardening

Goal:

- stabilize the advanced non-`D` track before the roadmap crosses into true dynarec-era work

Tickets:

- `W4-INT-001` Advanced hybrid scheduler regression pass
  - Depends on: `W4-JAG-001`
  - Done when: wave-4 families share one timing-focused regression pass across advanced hybrids and workstations.
- `W4-INT-002` CD and add-on media hardening
  - Depends on: `W4-INT-001`
  - Done when: the host layer supports the wave-4 CD/add-on matrix without ad-hoc per-family media handling.

## Wave 5

### M22 D Execution Substrate

Goal:

- build the separate execution substrate needed for the first true `D`-bucket systems

Tickets:

- `W5-RT-001` Block-execution and event-sync contract
  - Depends on: `W4-INT-002`
  - Done when: the suite supports block execution plus timestamped synchronization points without replacing the non-`D` contract.
- `W5-RT-002` MMU, cache, and TLB service layer
  - Depends on: `W5-RT-001`
  - Done when: the shared support layer exposes the substrate needed by the first `D` families without forcing a universal CPU model.
- `W5-RT-003` GPU command-stream and frame-presentation contract
  - Depends on: `W5-RT-001`
  - Done when: the shared `D` support can host command-buffer-style graphics pipelines under one host-facing video path.
- `W5-RT-004` D-era validation harness
  - Depends on: `W5-RT-002`, `W5-RT-003`
  - Done when: the validation harness can capture and compare the kinds of artifacts the first `D` systems need.

### M23 First D Consoles

Goal:

- prove the `D` execution substrate on the strongest early candidates

Tickets:

- `W5-3DO-001` 3DO baseline
  - Depends on: `W5-RT-004`
  - Done when: the 3DO family reaches one validated baseline profile.
- `W5-PS1-001` PlayStation baseline
  - Depends on: `W5-RT-004`
  - Done when: the PlayStation family reaches one validated baseline profile.

### M24 Second D Consoles

Goal:

- extend the `D` execution substrate to the next tier of late-1990s platforms

Tickets:

- `W5-N64-001` Nintendo 64 baseline
  - Depends on: `W5-PS1-001`
  - Done when: the Nintendo 64 family reaches one validated baseline profile.
- `W5-DC-001` Dreamcast baseline
  - Depends on: `W5-PS1-001`
  - Done when: the Dreamcast family reaches one validated baseline profile.

### M25 Handheld D Cluster

Goal:

- prove the `D` execution substrate on late-retro handhelds

Tickets:

- `W5-DS-001` Nintendo DS baseline
  - Depends on: `W5-N64-001`
  - Done when: the Nintendo DS family reaches one validated baseline profile.
- `W5-PSP-001` PSP baseline
  - Depends on: `W5-DC-001`
  - Done when: the PSP family reaches one validated baseline profile.

### M26 Upper-Bound D Cluster

Goal:

- test the upper practical limit of the user's retro boundary while staying inside the 2000s

Tickets:

- `W5-GC-001` GameCube baseline
  - Depends on: `W5-PSP-001`
  - Done when: the GameCube family reaches one validated baseline profile.
- `W5-WII-001` Wii baseline
  - Depends on: `W5-GC-001`
  - Done when: the Wii family reaches one validated baseline profile.

### M27 Wave-5 Hardening

Goal:

- decide whether the `D` execution substrate is a durable part of the suite or a natural stopping point

Tickets:

- `W5-INT-001` D-era regression matrix and artifact capture
  - Depends on: `W5-WII-001`
  - Done when: the `D` families share one regression matrix with comparable graphics/audio artifacts.
- `W5-INT-002` D-era save-state and determinism study
  - Depends on: `W5-INT-001`
  - Done when: the suite has a documented determinism and save-state policy for the first `D` wave.

## Wave 6

### M28 Catalog Framework And Triage

Goal:

- turn the long tail into a managed backlog instead of vague future scope

Tickets:

- `W6-RT-001` Catalog intake rules and promotion criteria
  - Depends on: `W5-INT-002`
  - Done when: there is a documented policy for promoting a family from archive/intake into an implementation wave.
- `W6-RT-002` Long-tail family shell and backlog tooling
  - Depends on: `W6-RT-001`
  - Done when: the suite can record family metadata, reference media/firmware needs, and research status without pretending implementation has started.

### M29 Early-Console And Handheld Oddities

Goal:

- cluster the remaining early and handheld platforms into a tractable intake model

Tickets:

- `W6-ODD-001` Early-console cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the remaining early-console families are grouped by likely runtime fit and promotion priority.
- `W6-ODD-002` Handheld oddities cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the remaining handheld/mobile/educational families are grouped by likely runtime fit and promotion priority.

### M30 Long-Tail Microcomputers

Goal:

- manage the remaining long-tail computer families without burying them in unsorted notes

Tickets:

- `W6-MICRO-001` 6502/Z80 long-tail computer cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the remaining 6502/Z80-focused computer families are grouped into a coherent promotion backlog.
- `W6-MICRO-002` Advanced long-tail computer cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the remaining advanced and region-specific computer families are grouped into a coherent promotion backlog.

### M31 Multimedia And Edge Consoles

Goal:

- isolate the low-priority multimedia and edge consoles so they are visible but do not distort the main roadmap

Tickets:

- `W6-EDGE-001` Multimedia oddities cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the multimedia-console families are grouped with explicit reasons for promotion or deferral.
- `W6-EDGE-002` Edge-console cluster plan
  - Depends on: `W6-RT-002`
  - Done when: the educational, toy, and edge-console families are grouped with explicit reasons for promotion or deferral.

### M32 Boundary Platforms

Goal:

- make the suite's architectural boundaries explicit

Tickets:

- `W6-BND-001` Saturn and adjacent `D`-edge research
  - Depends on: `W5-INT-002`
  - Done when: the suite has a documented position on `saturn_family` and adjacent edge-`D` platforms.
- `W6-BND-002` PS2 and original Xbox boundary research
  - Depends on: `W5-INT-002`
  - Done when: the suite has a documented position on `ps2_family` and `xbox_family`.
- `W6-BND-003` PS3/Xbox 360 exclusion guardrail
  - Depends on: `W6-BND-002`
  - Done when: the suite has an explicit exclusion policy and future re-entry criteria for `ps3_family` and `xbox360_family`.

### M33 Wave-6 Archive Hardening

Goal:

- make the archive and intake wave supportable over time

Tickets:

- `W6-INT-001` Catalog coverage dashboard and unmet-family report
  - Depends on: `W6-BND-003`
  - Done when: the backlog can report what is implemented, what is planned, what is archived, and why.
- `W6-INT-002` Promotion pipeline from archive to implementation wave
  - Depends on: `W6-INT-001`
  - Done when: one archived family can be promoted cleanly into an implementation wave without rewriting the planning structure.

## Notes

- `Wave 2` and `Wave 3` are implementation-oriented.
- `Wave 4` remains implementation-oriented but should be entered only if earlier waves have held together.
- `Wave 5` depends on a successful `D` execution substrate.
- `Wave 6` is primarily an intake, research, and scope-governance wave.

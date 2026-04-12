# Emulator Suite Roadmap: Waves 2-6

**Status:** Draft
**Date:** 2026-04-11

## Purpose

Extend the wave-1 plan into a full suite roadmap covering the remaining decades and families in the system registry.

This file defines the later waves at a planning level.

The milestone and ticket breakdown is in [2026-04-11-emulator-suite-roadmap-waves-2-6-milestones.md](./2026-04-11-emulator-suite-roadmap-waves-2-6-milestones.md).

The machine-readable ticket list is in [emulator-suite-roadmap-waves-2-6-tickets.csv](./emulator-suite-roadmap-waves-2-6-tickets.csv).

## How To Read Later Waves

- The suite still ships one binary per system family; later waves assume shared support crates and tooling, not one umbrella executable.
- `Wave 2` and `Wave 3` are high-confidence implementation waves.
- `Wave 4` is still implementation-oriented, but more schedule-sensitive.
- `Wave 5` is where the suite first commits to `D`-bucket dynarec+events platforms.
- `Wave 6` is a catalog and research wave, not a promise that every listed family should be implemented immediately.

Later-wave tickets are roadmap epics, not day-to-day implementation tasks. They should split again when active work starts.

## Wave 2: Family Expansion

Goal:

- turn wave-1 reference systems into actual family platforms
- add adjacent handheld families that reuse the same shared support assumptions
- keep the roadmap inside `B/T/S/H`

Primary targets:

- `gameboy_family`
  - `CGB`
  - common save-backed MBCs and RTC-backed carts
- `sega_8bit_family`
  - `Master System 1`
  - `Game Gear`
  - FM and family-variant handling
- `spectrum_family`
  - `128K`, `+2`, `+3`
  - disk-backed late-family variants
- `famicom_nes_family`
  - `MMC3`
  - `FDS`
  - expansion audio
  - clearer NTSC/PAL profile separation
- `commodore_64_128_family`
  - `C128`
  - pragmatic `IEC/1541` path
- `pc_engine_family`
  - `CD-ROM2`
  - `SuperGrafx`
- `amiga_ocs_ecs_family`
  - `ECS`
  - `A500+`
  - `A600`
- adjacent handhelds:
  - `wonderswan_family`
  - `neo_geo_pocket_family`
  - `lynx_family`
  - `gba_family`

Why this wave exists:

- it proves that the shared support layer can handle variants, persistent media, and siblings without collapsing into per-system host code
- it expands on the exact systems that are most likely to teach the suite the right abstractions

## Wave 3: Flagship Breadth

Goal:

- add the remaining mainstream 8-bit and 16-bit flagship families that are still strongly aligned with the suite's core architecture

Primary targets:

- `apple_ii_family` starting at `Apple IIe`
- `atari_400_800_family` starting at `800XL`
- `msx_family` starting at `MSX1`, then `MSX2`
- `bbc_micro_family` starting at `Model B`
- `amstrad_cpc_family` starting at `CPC 6128`
- `coco_dragon_family` starting at `CoCo 2`, then `CoCo 3`
- `colecovision_family`
- `intellivision_family`
- `atari_7800_family`
- `vectrex_family`
- `megadrive_genesis_family` starting at `Genesis 1` cart-only
- `snes_family`
- `neo_geo_family` starting at `AES/MVS`
- `atari_st_family`

Why this wave exists:

- these are the remaining "big-name" retro platforms not covered by wave 1
- most of them still fit cleanly into the suite's core scheduler families
- this is where the suite becomes broadly credible to retro users, not just architecturally interesting

## Wave 4: Advanced Non-D Systems

Goal:

- cover the advanced late-1980s and 1990s systems that still behave like hardware-led machines, but are too complex to treat as routine wave-3 work

Primary targets:

- `amiga_aga_family`
- `atari_falcon_family`
- `apple_iigs_family`
- `acorn_archimedes_family`
- `acorn_risc_pc_family`
- `x68000_family`
- `fujitsu_fm_towns_family` and `fm_towns_marty_family`
- `nec_pc8_pc88_pc98_family` starting at `PC-8801`
- add-on and late-family expansions:
  - `megadrive_genesis_family` with `Mega-CD / Sega CD` and `32X`
  - `neo_geo_family` with `Neo Geo CD`
- late console hybrids:
  - `virtual_boy_family`
  - `jaguar_family`

Why this wave exists:

- these systems are still deterministic enough for the suite's hardware-led model
- they need richer arbitration, DMA, add-on, or computer-style subsystem support than wave 3
- they are still better aligned with the retro suite than the first `D` platforms

## Wave 5: First Dynarec/Event Platforms

Goal:

- establish the suite's first serious `D`-bucket execution substrate and use it on the strongest late-retro candidates

Primary targets:

- `3do_family`
- `playstation_family`
- `nintendo64_family`
- `dreamcast_family`
- `nintendo_ds_family`
- `psp_family`
- `gamecube_family`
- `wii_family`

Why this wave exists:

- this is the cleanest set of culturally retro but architecturally modern enough systems to justify a new execution-policy substrate
- it keeps the suite within the user's stated 2000s boundary without sliding into contemporary platform work

## Wave 6: Catalog Breadth And Boundary Research

Goal:

- absorb the rest of the registry into a disciplined intake wave
- make edge and optional systems visible without pretending they are all near-term implementation commitments

Wave 6 includes:

- remaining early consoles and first-wave machines:
  - `fairchild_channel_f`
  - `bally_astrocade`
  - `odyssey2_videopac_family`
  - `atari_5200_family`
  - `creativision_family`
  - `arcadia_2001_family`
  - `cassette_vision_family`
  - `super_cassette_vision_family`
  - `microvision_family`
  - `epoch_game_pocket_computer`
- remaining long-tail computer families:
  - `commodore_pet_cbm_family`
  - `trs80_model_i_family`
  - `commodore_vic20_family`
  - `commodore_plus4_c16_family`
  - `ti99_4a_family`
  - `oric_family`
  - `thomson_family`
  - `enterprise_family`
  - `sam_coupe_family`
  - `tatung_einstein_family`
  - `camputers_lynx_family`
  - `mattel_aquarius_family`
  - `sord_m5_family`
  - `tomy_tutor_family`
  - `coleco_adam_family`
  - `nabu_family`
  - `galaksija_family`
  - `kc85_family`
  - `elektronika_bk_family`
  - `sharp_mz_family`
  - `sharp_x1_family`
  - `nec_pc6001_family`
  - `fujitsu_fm7_family`
  - `electron_family`
  - `amstrad_pcw_family`
- remaining handheld, mobile, and educational systems:
  - `supervision_family`
  - `mega_duck_family`
  - `gamate_family`
  - `game_master_family`
  - `pokemon_mini_family`
  - `gameking_family`
  - `gp32_family`
  - `ngage_family`
  - `tapwave_zodiac_family`
  - `gizmondo_family`
  - `gp2x_family`
  - `dingoo_family`
  - `hyperscan_family`
  - `vsmile_family`
  - `leapster_family`
  - `zeebo_family`
- multimedia and edge consoles:
  - `cdi_family`
  - `laseractive_family`
  - `pcfx_family`
  - `playdia_family`
  - `casio_loopy_family`
  - `super_acan_family`
  - `apple_pippin_family`
  - `nuon_family`
- edge and boundary platforms:
  - `saturn_family`
  - `xbox_family`
  - `ps2_family`
  - `ps3_family`
  - `xbox360_family`

Why this wave exists:

- the registry should not hide optional and edge systems just because the main roadmap is selective
- some of these systems may later be promoted into earlier implementation waves
- some should remain intentionally archived behind explicit scope guards

## Sequencing Logic

- `Wave 2` deepens the systems most likely to force good abstractions.
- `Wave 3` fills out the suite's flagship retro catalog.
- `Wave 4` handles the advanced but still non-`D` machines before the roadmap crosses into true dynarec-era systems.
- `Wave 5` is the first place where a separate `D` execution substrate is worth its engineering cost.
- `Wave 6` keeps the long tail visible and managed instead of leaving it as vague "someday" scope.

## Practical Commitment Level

- If the suite remains a long-term but finite project, `Wave 2` and `Wave 3` are the most realistic next commitments.
- `Wave 4` is the upper end of a classic retro-focused roadmap.
- `Wave 5` is ambitious but still coherent if the `D`-substrate work succeeds.
- `Wave 6` should be treated as an intake and research track unless specific systems are promoted.

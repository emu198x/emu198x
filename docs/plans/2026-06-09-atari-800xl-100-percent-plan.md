---
title: "plan: Atari 800XL to 100% — disk/SIO media, cartridge banking, chip-accuracy depth, model breadth"
type: plan
date: 2026-06-09
system: docs/systems/atari/atari-800xl.md
basis: code-grounded survey of machine-/runtime-/emu198x-atari-800xl + atari-antic/gtia/pokey + mos-6502 + mos-pia-6520, live test runs, cross-checked against reference/by-system/atari-8bit, 2026-06-09. Shared-chip (ANTIC/GTIA/POKEY/6502) findings supplied by prior chip assessments and referenced, not re-derived.
---

# Atari 800XL — road to 100%

What it would take to bring the Atari 800XL to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests. The 800XL is the
furthest-advanced donor extraction outside the launch six — it **boots a real OS
ROM through to the BASIC `READY` prompt** and types into BASIC — but it is a
fourth distinct shape from the launch platforms.

## Executive summary

**The 800XL's hard part — bring-up — is done; the long pole is media and chip
depth, in that order.** The machine wiring is real and correct enough to cold-boot
the Atari OS, survive the SIO disk-boot timeout, fall through to the built-in
BASIC cartridge, render a GR.0 text screen, and accept keyboard input
(`machine-atari-800xl/tests/basic_boot_probe.rs` asserts LOMEM/VNTP set, the
`READY` screen codes present, and `PRINT 6*7` → `42`). The memory map, PORTB ROM
banking, the PIA A0/A1 cross-wire, ANTIC/GTIA/POKEY I/O decode, and the DMA-steal
CPU stall are all wired (`machine-atari-800xl/src/lib.rs`).

That inversion is the headline. **The boot works but you cannot load anything
into it.** There is no disk, no SIO peripheral, no XEX loader, and the cartridge
layer is flat 8K/16K with no bank-switching (`machine-atari-800xl/src/cartridge.rs`;
`$D500` CCTL writes are silently dropped at `lib.rs:400`). So the only software
the 800XL runs today is a single flat cartridge or the built-in BASIC — none of
the disk-based library, none of the banked carts, none of the type-in-from-disk
curriculum flow.

Underneath that, the shared custom chips carry **real accuracy debt that this
machine is the consumer of**: ANTIC fine scrolling (HSCROL/VSCROL) is completely
non-functional, GTIA PRIOR priority schemes are ignored, and POKEY has two
confirmed defects (distortion table, 16-bit linked-channel period). Those are
filed as chip-level work elsewhere; this plan **references** them as the depth
tier the 800XL inherits, and adds only the 800XL-specific wiring/verification
they need.

The **CPU is at the ceiling** — the `mos-6502` NMOS core is Tom-Harte- and
Dormann-verified — so, as with the C64 and NES, there is **no CPU work** on the
road to 100%. The one 6502-level caveat that touches this machine
(IRQ/NMI ROM-level cross-check for a non-NES system that drives interrupts hard)
is a verification item, not a defect.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | SIO + ATR disk loading, XEX executable loader, banked cartridges (OSS/XEGS/MegaCart), paddle pots + 2nd joystick port, deep snapshot, doc/test-name fixes | **~5–8 weeks** |
| B — Chip-accuracy depth (the 800XL's share) | wire + verify the ANTIC fine-scroll fix, GTIA PRIOR schemes, POKEY distortion/16-bit fixes at the machine level; mid-line ANTIC register-write timing; GTIA-mode 9/A/B playfield feed | **~4–6 weeks** |
| C — Cycle-exact finish | real SIO baud/serial timing (replace the magic-constant handshake), ANTIC per-character DMA-steal positions, IRQ/NMI ROM-level cross-check | **~3–5 weeks** |
| D — Preservation / model breadth | 130XE extended RAM, 400/800 variants, PBI/cassette, CAR-header cart detection, more banking schemes, PAL palette + $D014 | **~4–6 weeks** |

**True 100% of everything ≈ 16–25 weeks.** Like the C64 (and unlike the
Spectrum) it is **not** front-loaded onto cheap wins — Tier A's disk stack is
genuinely the long pole because nothing in-tree parses Atari disk media yet. The
launch-irrelevant reality (800XL is engineering-bar, not October-public scope)
means this is a depth roadmap, not a deadline plan.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (media + I/O breadth; the long pole)

| Item | Effort | Notes |
|------|--------|-------|
| **SIO + ATR disk loading** | **XL** | There is **no Atari disk format crate at all** (no `format-atari-*` under `crates/`). The 800XL boots, runs the OS SIO send-loop, times out, and falls to BASIC — but with a drive attached it has nothing to talk to. POKEY's SIO is self-admittedly faked (`atari-pokey/src/lib.rs:108–116`, magic tick constants tuned only so the boot send-loop sees the right IRQST order; `cpu_freq` stored-but-unused). Real disk loading needs an ATR parser + a D1: drive state machine + the SIO command/data framing. This is the single biggest gap — it unlocks the entire disk-based library and the type-in-from-disk curriculum flow. |
| **XEX executable loader** | **M** | The simplest path to running a binary without a full DOS/disk stack: parse the segmented `$FFFF`-header XEX format and inject segments + run the init/run vectors. No format crate exists. High leverage — most homebrew and curriculum binaries ship as XEX. |
| **Banked cartridges (OSS / XEGS / MegaCart)** | **L** | `cartridge.rs` handles only flat 8K (`$A000`) and 16K (`$8000`); `$D500–$D7FF` CCTL writes are dropped (`lib.rs:400 0xD500..=0xD7FF => {}`). Reference (`atari-8bit-reference.md:1086–1094`) documents OSS Super Cart, XEGS 64K, and MegaCart up to 256K, all banked via `$D500`. Without this, 32K+ carts don't run. |
| **Paddle pots + 2nd joystick port** | **M** | POKEY has `set_pot` (the 5200 uses it) but the 800XL machine never exposes paddles; the drivability assessment flags atari-800xl as a paddle gap (`drivability-assessment.md:116,241`). The 2nd joystick port (PIA PORTB lower nibble on 400/800, or via the same PORTA path) is also unexposed — `input.rs:67` self-admits "second joystick port and the POKEY paddle pots are not yet exposed." |
| **Deep snapshot (live state, not re-derive)** | **M** | `runtime-atari-800xl/src/snapshot.rs` serialises only `{time, model_id, os_bytes, basic_bytes, cart_bytes, basic_enabled}` and `restore` rebuilds a *fresh* machine from ROM bytes — it does **not** capture live RAM, CPU, ANTIC/GTIA/POKEY/PIA state. A "restore" therefore resets the running machine to cold boot rather than restoring it. Rewind/save-state is non-functional for this system. |
| **Doc + test-name corrections** | **S** | (1) `cartridge.rs`/`lib.rs` test `rejects_invalid_rom_size` asserts a 4097-byte ROM `is_ok()` — the name contradicts the assertion (only oversize is rejected; sub-8K is silently accepted as an 8K cart). (2) `outstanding-work.md:465` says "POKEY audio synthesis unwired in the binary" — imprecise: the headless `--audio-capture` path **does** synthesise + write WAV (`emu198x-atari-800xl/src/script.rs:242`); what is absent is a live/native audio device, gated on the native window. Correct the doc. |

## Tier B — Chip-accuracy depth (the 800XL's share of the custom chips)

These are the machine-level wiring + verification the shared-chip fixes need.
The chip-internal fixes are filed as chip-level issues elsewhere; the 800XL is
one of exactly two consumers (with the 5200) of ANTIC/GTIA/POKEY.

| Item | Effort | Notes |
|------|--------|-------|
| **Verify ANTIC fine-scroll fix end-to-end on 800XL** | **M** | HSCROL/VSCROL are stored but never read in render (`atari-antic` — the single biggest ANTIC gap). Most smooth-scrolling Atari games depend on it. After the chip fix, the 800XL needs a machine-level scroll regression (a cart that programs HSCROL and proves horizontal pixel shift). |
| **Verify GTIA PRIOR priority schemes on 800XL** | **M** | GTIA ignores PRIOR bits 0–3 (hardcoded "PM over PF over background"); games using PRIOR=$04 (playfield-over-players, status-bar overlays) render players wrongly on top. After the chip fix, add an 800XL machine test that drives PRIOR and checks PM-vs-PF layering. |
| **Verify POKEY distortion + 16-bit period fixes audibly on 800XL** | **M** | POKEY's distortion table is wrong for 3 of 8 settings and 16-bit linked-channel mode produces the wrong period (both confirmed chip defects). The 800XL is a primary consumer; after the chip fix, capture-and-compare audio via `--audio-capture` against a reference (Altirra/atari800). |
| **Mid-line ANTIC register-write timing** | **M** | `start_scan_line` builds the whole line up-front (`lib.rs:286–305`), so a mid-line write to DMACTL/CHBASE/HSCROL only takes effect next line. GTIA *colour* writes are beam-correct (the `mid_line_colbk_write_splits_the_scanline` test proves it), but ANTIC-side mid-line effects (DLI-driven CHBASE swaps, mid-screen DMACTL) won't land. Needs-runtime-verification on whether real software depends on it; if so, this is an ANTIC-line-streaming change. |
| **GTIA-mode 9/A/B playfield feed from ANTIC** | **S–M** | Unverified that `atari-gtia` consumes ANTIC's 1bpp/2bpp playfield Vec correctly for `AnticMode::Mode9/A/B` (the GTIA special-colour modes). Needs an on-machine render check; flagged as verification, not asserted defect. |

## Tier C — Cycle-exact finish

| Item | Effort | Notes |
|------|--------|-------|
| **Real SIO serial/baud timing** | **L** | Replace POKEY's magic-constant SIO handshake (`SEROUT_HOLD_TICKS=90`, `SEROUT_SHIFT_TICKS=930`, hand-tuned so only the boot send-loop's IRQST bit order is right) with a real baud-rate shift model. Required for disk-timing-sensitive code and fastloaders; rides on the Tier-A disk stack. SERIN is currently a passive latch with no receive clock. |
| **ANTIC per-character DMA-steal positions** | **M** | `cpu_dma_stalled` steals the exact DMA budget but spreads it Bresenham-evenly across the fetch window rather than at true per-character fetch positions (self-admitted in `atari-antic/src/lib.rs:42–53`, the relaxation MAME also takes). Tightening this is the last mile of cycle-exact CPU/ANTIC contention. |
| **IRQ/NMI ROM-level cross-check** | **S–M** | The `mos-6502` interrupt edge cases (branch-IRQ-suppress, NMI-hijack-during-BRK) pass the in-crate hermetic tests but get their ROM-level (blargg) validation only at the *NES* machine layer. The 800XL drives IRQ (POKEY/PIA) and NMI (ANTIC VBI/DLI) heavily; a machine-level interrupt-timing harness would close the verification gap. Verification, not a known defect. |

## Tier D — Preservation / model breadth

| Item | Effort | Notes |
|------|--------|-------|
| **130XE 128 KB extended RAM** | **M** | PORTB bits 2–5 drive 4×16 KB extended banks via the FREDDIE controller (`reference:85,1116`); not modelled (`lib.rs` reads only PORTB bits 0/1/7). Deferred in the original slice (`outstanding-work.md:471`). |
| **400 / 800 variants** | **M** | Same chip family, different RAM, no XL PORTB banking, two cartridge slots (left $A000 + right $8000). Model-selector flag + the 10 KB OS path. The profile catalogue currently only declares NTSC/PAL 800XL (`runtime-atari-800xl/src/profiles.rs`). |
| **PAL palette + $D014 region flag** | **S–M** | GTIA returns `$D014 = 0x00` (which *tells software PAL*) while the only palette compiled in is NTSC (`atari-gtia/src/palette.rs`, NTSC-only). A real PAL 800XL profile exists (`Atari800xlRegion::Pal`, 312 lines) but has no PAL palette and the wrong $D014. Software branching on $D014 for tempo/timing picks the wrong path. |
| **CAR-header cart detection** | **S** | Carts are detected purely by length (`cartridge.rs:18`). The standard `.car` header (16-byte CARTRIDGE magic + type ID + checksum) would disambiguate banking schemes instead of guessing — feeds the Tier-A banked-cart work. |
| **PBI / cassette / 850 interface** | **L** | The Parallel Bus Interface (600XL/800XL, `reference:1112`) and cassette (the C: device over SIO) are preservation-tier peripherals. Cassette rides the SIO stack; PBI is a separate bus. |

## Done as part of this plan (free, ~half a day)

Doc + test-name drift corrected: (1) the `outstanding-work.md` "POKEY audio
unwired in the binary" line is imprecise — the `--audio-capture` headless path
synthesises and writes WAV today; only a live audio device is missing. (2) The
`rejects_invalid_rom_size` test name is corrected to reflect that it only rejects
oversize ROMs and silently accepts sub-8K ones. (3) The plan records that all
six boot/keyboard regression tests are `#[ignore]`-gated on a local ROM bundle
(`~/.emu198x/roms/atari-800xl/`), so the "boots to READY" claim is
verified-when-ROMs-present, not in CI — captured as a runtime-verification note
rather than asserted.

## Recommended sequence (highest leverage first)

1. **XEX executable loader** (M) — the cheapest path to running real binaries;
   no disk stack required. Highest leverage per week.
2. **Banked cartridges** (L) — unlocks the 32K+ cartridge library; the CCTL
   write strobe is already routed to a no-op, so the wiring point exists.
3. **SIO + ATR disk loading** (XL) — the long pole; unlocks the disk-based
   library and the curriculum's disk flow. Build the ATR parser + D1: drive +
   real SIO framing together.
4. **Paddle pots + 2nd joystick + deep snapshot** (M×3) — round out I/O and make
   save-state actually work.
5. **Chip-depth verification** (Tier B) — once the ANTIC/GTIA/POKEY chip fixes
   land, wire + prove them at the 800XL machine level (scroll, PRIOR, audio A/B).
6. **Real SIO timing + per-character DMA-steal** (L + M) — cycle-exact finish,
   riding the disk stack.
7. **130XE / 400 / 800 / PAL palette** (M×3 + S–M) — model + preservation breadth.

## Key files

- CPU (at ceiling, no work): `crates/mos-6502/src/{lib,cycle,tick}.rs`.
- Machine wiring: `crates/machine-atari-800xl/src/lib.rs` (memory map `mem_read`/`mem_write` :329–404, PORTB banking :307–333, PIA cross-wire `bus_to_pia_addr` :324, DMA stall + tick `tick_colour_clock` :234–279, CCTL no-op `:400`, snapshot-relevant `ram()`/`peek`/`poke`).
- Cartridge (flat-only, banking gap): `crates/machine-atari-800xl/src/cartridge.rs`.
- Runtime + snapshot (shallow): `crates/runtime-atari-800xl/src/{runtime.rs,snapshot.rs,profiles.rs,input.rs}` (paddle/2nd-port gap `input.rs:67`).
- Binary + capture: `crates/emu198x-atari-800xl/src/{main.rs,script.rs}` (`--audio-capture` `script.rs:242`).
- Chips (depth tier, shared with 5200): `crates/atari-antic/src/lib.rs` (HSCROL/VSCROL, mode 3, NMIST), `crates/atari-gtia/src/{lib.rs,palette.rs}` (PRIOR, $D014, GR.10), `crates/atari-pokey/src/lib.rs` (distortion `:976–994`, 16-bit `:821–866`, SIO `:108–116`).
- Tests: `crates/machine-atari-800xl/tests/{basic_boot_probe.rs,os_boot.rs}` (all `#[ignore]`, ROM-gated), in-crate unit tests `lib.rs:591–760` (16 pass).
- Reference: `reference/by-system/atari-8bit/{atari-8bit-reference.md,mappingtheatari.md}`; emulators `emulators/atari/` (Altirra/atari800 as audio + SIO oracle); solution record `docs/solutions/atari-800xl-sio-disk-boot-to-basic.md`.


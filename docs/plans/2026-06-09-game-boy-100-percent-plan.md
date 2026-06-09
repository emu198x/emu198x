---
title: "plan: Nintendo Game Boy to 100% — the PPU mid-scanline long pole, CGB breadth, and preservation tail"
type: plan
date: 2026-06-09
system: docs/systems/nintendo/game-boy.md
basis: code-grounded survey of all Game Boy crates with live test-suite runs (Adam Tennant SM83 49,600; mooneye DMG acceptance 35/35; blargg cpu_instrs/instr_timing/mem_timing v1+v2; blargg dmg_sound 12/12; dmg-acid2 pixel-reference; Mealybug-tearoom pixel-diff diagnostic), 2026-06-09
---

# Nintendo Game Boy — road to 100%

What it would take to bring the DMG Game Boy to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates **with live test-suite runs**.
Unlike the brief's open question, the system-level timing claim is no longer a
"trust the doc" item — it was re-run and **passes** (see Executive summary).

## Executive summary

**The Game Boy is the most-finished handheld/console core in the fleet for the
DMG launch surface, and it carries a single, sharply-localised core-accuracy long
pole plus a large breadth tail.** This is a fourth distinct shape:

- The **Spectrum** had a finished core and cheap, front-loaded breadth.
- The **C64** plays its library but hides a hard core-accuracy long pole (the VIC-II rewrite).
- The **NES** has a finished core plus cheap breadth, with two real bugs.
- The **Game Boy** has a finished core *and* the breadth basics in hand (5 MBCs,
  battery `.sav`, 5 boot profiles, native + headless verifier) — but its one hard
  core-accuracy item is **the PPU pixel-FIFO at mid-scanline resolution**, and its
  breadth tail is dominated by **a whole second machine (CGB)**.

The three chips that the brief called out are genuinely done — and I verified it
live rather than trusting the knowledge doc:

- **CPU (SM83):** at the ceiling, independently established (49,600 Adam Tennant
  single-step + 99 lib units). `sharp-lr35902`. **No work** beyond the one STOP
  fidelity enhancement (CGB-only; see Tier C).
- **PPU (`nintendo-game-boy-ppu`):** **dmg-acid2 pixel-perfect** (re-ran
  `dmg_acid2_renders_reference`: hash `0xf272a8ffe3db4c16`, byte-stable against the
  reference). Per-scanline rendering and the STAT/LY/window machinery are at the
  ceiling. The remaining gap is **mid-scanline FIFO timing** (Mealybug) — see Tier A.
- **APU (`nintendo-game-boy-apu`):** **at the ceiling for DMG.** I re-ran the full
  blargg `dmg_sound` suite — **all 12 sub-tests pass** (01-registers … 12-wave
  write while on, including the wave-read-while-on and sweep-details cases). The
  outstanding-work doc's "APU not yet ledgered" line is **stale**; it is ledgered now.

The system-level timing suites the brief flagged as needing verification were
**run and pass**: `phase2_verification` returns **6 passed, 0 failed** — blargg
`cpu_instrs` (all 11), `instr_timing`, `mem_timing` v1+v2, the mooneye acceptance
gate, and dmg-acid2. The mooneye DMG acceptance suite at machine level
(`mooneye_dmg_acceptance_suite_passes`) also passes its full local set. **The
"needs runtime verification" item on system-level timing is closed.**

So "100% Game Boy" is **one hard PPU item + CGB (a second machine) + a
preservation/peripheral tail**, not a CPU or APU rewrite.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum/DMG 100%" | Mealybug mid-scanline FIFO rewrite (+oracle), full OAM-DMA bus blocking, real-game screenshot smokes, blargg oam_bug + interrupt_time harness | **~4–7 weeks** |
| B — Boot fidelity + DMG completeness | optional real boot-ROM execution slot, MBC3 RTC wall-clock advance, ROM+RAM (`$09`) battery persistence, MBC1M edge coverage | **~1.5–2.5 weeks** |
| C — CGB (a second machine) | CGB core: double-speed, VRAM/WRAM/palette banking, HDMA/GDMA, CGB-acid2 + cgb_sound + CGB mooneye, STOP-speed-switch fidelity | **~8–12 weeks** |
| D — Preservation breadth | SGB super functions, link-cable/serial peer, MBC6/MBC7+sensor, HuC1/HuC3, MMM01, Pocket Camera, TAMA5, rumble surface | **~6–10 weeks** |

**True 100% of everything ≈ 20–32 weeks.** It is **back-loaded onto CGB + the
preservation tail** (Tiers C+D), not the DMG core. The launch-relevant + "feels
finished DMG" slice (Tier A + the cheap parts of B) is **~5–8 weeks**, and the
single highest-value item in it — the Mealybug FIFO rewrite — is genuinely hard.

For the October Spectrum-only launch none of this is on the critical path; the
Game Boy is an engineering-bar system. This plan is the forward view for when it
becomes a deliverable.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — DMG core finish (the long pole)

| Item | Effort | Notes |
|------|--------|-------|
| **Mealybug-tearoom mid-scanline FIFO rewrite** | **L–XL** | The one hard core item. I ran the `diagnostic_mealybug` pixel-diff harness: of 24 DMG ROMs only **`m2_win_en_toggle` is exact (diff=0)**; the rest fail, several badly — `m3_wx_5_change` diff=11303, `m3_wx_4_change` 11203, `m3_scy_change` 10425, `m3_lcdc_win_en_change_multiple` 8316, `m3_lcdc_obj_size`/`tile_sel`/`bgp_change` in the hundreds-to-thousands. These exercise LCDC/SCX/SCY/WX/BGP/OBP changes **applied mid-Mode-3**, which the current background/window fetcher in `nintendo-game-boy-ppu/src/{fetcher,fifo}.rs` does not honour per-dot. dmg-acid2 (end-of-Mode-3 state) already passes, so this is specifically the *intra-scanline mutation* path. |
| **Mealybug pixel oracle in CI tier** | M | Promote the existing `diagnostic_mealybug` diff harness (machine `tests.rs:1326`) into a gating env-test like `dmg_acid2_renders_reference`, with per-ROM expected-diff thresholds, so the FIFO rewrite is provable ROM-by-ROM, not vibes. References live at `assets/test-suites/gameboy/mealybug-tearoom/ppu/`. |
| **Full OAM-DMA bus blocking** | M | The machine self-flags this gap (`machine-nintendo-game-boy/src/lib.rs:14-17`): OAM access is blocked and the transfer is paced (`tick_oam_dma`, `:492`), but CPU access to **non-HRAM** memory during the 160-m-cycle DMA is not gated. Real hardware returns the in-flight DMA byte / blocks the bus; some titles depend on the HRAM-only execution discipline. |
| **blargg `oam_bug` + `interrupt_time` harness** | S–M | Both corpora are on disk (`assets/test-suites/gameboy/blargg/{oam_bug,interrupt_time}`) but **no test references them**. Wire them into the phase2 harness to ledger the DMG OAM-corruption bug + interrupt-timing behaviour (pass/fail currently unknown — see needs-verification). |
| **Real-game screenshot smokes** | S | Today the only end-to-end evidence is synthetic test ROMs + dmg-acid2. Boot a handful of known-good commercial titles and lock framebuffer hashes so real-software regressions get caught (mirrors the Dragon/Oric smoke pattern). |

## Tier B — Boot fidelity + DMG completeness

| Item | Effort | Notes |
|------|--------|-------|
| **Optional real boot-ROM execution** | M | The CPU resets at `$0100` with documented post-boot register state per `BootProfile` (`machine-nintendo-game-boy/src/lib.rs:46-113`, 5 profiles incl. SGB/SGB2 DIV phases) rather than running the 256-byte boot ROM. This is a deliberate skipped-boot model and is *correct* for game-running; a real boot-ROM slot (logo scroll, header-checksum gate, the boot DIV/wave-RAM side effects) is a fidelity/preservation enhancement, not a defect. |
| **MBC3 RTC wall-clock advance** | M | `nintendo-game-boy-mbc/src/mbc3.rs` models the latch correctly (`$0C` 0→1 latch, halt bit, day-carry — unit-tested) but **nothing advances the live registers over real time** — the doc comment at `mbc3.rs:14` explicitly defers it to "the runtime layer … once it exists", and `grep` finds no RTC tick in runtime or machine. Pokemon Gold/Silver/Crystal time events, Harvest Moon depend on it. Add a tick source + serde-persisted last-wall-time. |
| **ROM+RAM (`$09`) battery persistence** | S | `format-nintendo-game-boy-cartridge/src/lib.rs:205-211` decodes cart-type `$08`/`$09` (ROM+RAM, ROM+RAM+BATTERY) to `CartType::RomOnly`, dropping the battery flag. The RAM is allocated and round-trips in memory, but `has_battery()` returns false so a `$09` cart's RAM is **never written to a `.sav`**. Niche (few carts) but a real persistence gap. |
| **MBC1M multicart edge coverage** | S | `mbc1.rs` auto-detects MBC1M (`looks_like_mbc1m`) and wires the one-bit-lower bank select. Verify against the mooneye `emulator-only/mbc1` multicart ROMs explicitly in the gating tier (currently swept, not gated). |

## Tier C — CGB (effectively a second machine)

The runtime is **DMG-family only** — `runtime-nintendo-game-boy/src/profiles.rs:4-5`
states "CGB will land alongside the family-driver lift". The five `Model` variants
are all DMG-class (Dmg0/Dmg/Mgb/Sgb/Sgb2). CGB is not a feature, it is a second
core sharing the SM83.

| Item | Effort | Notes |
|------|--------|-------|
| **CGB core scaffold** | XL | Double-speed (the real use of SM83 STOP — see below), 32 KiB banked WRAM + 16 KiB banked VRAM, BG/OBJ palette RAM, the priority/attribute model, KEY1/VBK/SVBK/BCPS/OCPS registers. The machine's WRAM/VRAM are fixed 8 KiB arrays (`lib.rs:34-35`) — banking is a structural change. |
| **HDMA / GDMA** | M | The CGB general + HBlank DMA controllers (`$FF51`–`$FF55`). No DMA path beyond OAM-DMA exists today. |
| **SM83 STOP fidelity (CGB speed switch)** | M | The established shared-chip finding: STOP is a sticky flag in `sharp-lr35902/src/opcodes/misc.rs:49-58`, not a faithful model. The **primary** real use of STOP is the CGB double-speed switch — so this enhancement is a CGB-tier dependency, not a standalone DMG item. (Irrelevant to DMG/launch; do not file against DMG.) |
| **CGB verification corpora** | M | cgb-acid2, cgb-acid-hell, blargg `cgb_sound` (present at `assets/test-suites/gameboy/blargg/cgb_sound`), CGB-specific mooneye. None can be scored until the core exists. |

## Tier D — Preservation + peripheral breadth

| Item | Effort | Notes |
|------|--------|-------|
| **SGB super functions** | L | Today SGB/SGB2 are *skipped-boot register profiles* only (`profiles.rs`, `BootProfile::Sgb/Sgb2`). The actual Super Game Boy features — packet command protocol, border, multi-palette, SNES-side audio — are unimplemented. |
| **Link cable / serial peer** | M–L | Serial is internal-clock self-clocking only (`lib.rs:442-456`, `tick_serial_irq`) — it pushes the byte to a reporting channel and raises `IF_SERIAL`. No external clock, no peer connection. Tetris/Pokemon trade/link play need a real serial peer (loopback, or two-instance bridge). |
| **MBC6, MBC7+sensor+rumble** | M each | Rejected at header parse (`format/src/lib.rs:272-276`). MBC7 needs the accelerometer (Kirby Tilt 'n' Tumble, Command Master). MBC6 is single-title (Net de Get). |
| **HuC1 / HuC3** | M | Rejected at parse (`:285-289`). HuC3 adds its own RTC + IR. Hudson library. |
| **MMM01** | M | Rejected at parse (`:268-271`). Multi-game compilation mapper. |
| **Pocket Camera, Bandai TAMA5** | M / M | Rejected at parse (`:277-284`). Camera needs the sensor model; TAMA5 is bespoke. Pure preservation. |
| **Rumble output surface** | S | MBC5 rumble flag is parsed (`CartType::Mbc5 { rumble }`) but there is no host rumble sink. Surface it as a host event. |

## Done as part of this plan (free, ~half a day)

The outstanding-work doc (`docs/status/outstanding-work.md:113-130`) is stale in
one direction and should be corrected:

- **"APU not yet ledgered" is wrong** — blargg `dmg_sound` all 12 sub-tests pass
  live (`blargg_dmg_sound_suite_passes`). The APU is ledgered and at the DMG ceiling.
- The knowledge overview (`knowledge/systems/nintendo-game-boy/overview.md`) and
  usability doc correctly describe the DMG core as through the Phase-2 gate; no
  correction needed there, but the lib-unit-test count drifted (doc says 92; live
  run is **99 CPU units + 14 common + 14 format + 22 machine + 88 APU + 26 MBC +
  24 PPU + 19 timer**). Refresh the figure.
- Record the **Mealybug baseline** (1/24 exact) as the explicit PPU frontier so it
  is a tracked, closeable number rather than a prose "next frontier".

## Recommended sequence (highest leverage first)

1. **Mealybug pixel oracle** (M) — build the gating comparator *before* the rewrite,
   ROM-by-ROM thresholds from the diagnostic baseline.
2. **Mealybug mid-scanline FIFO rewrite** (L–XL) — the one hard DMG core item; the
   single biggest accuracy lift. Demoscene-grade and mid-frame-effect titles.
3. **Full OAM-DMA bus blocking** (M) + **blargg oam_bug + interrupt_time harness**
   (S–M) — close the remaining DMG timing/bus corners and ledger two unrun corpora.
4. **Real-game screenshot smokes** (S) — cheap regression net on actual software.
5. **MBC3 RTC wall-clock advance** (M) + **ROM+RAM `$09` battery** (S) — the cheap
   DMG-completeness/persistence wins.
6. **Optional boot-ROM execution** (M) — boot fidelity / preservation.
7. **CGB core scaffold → HDMA → STOP speed-switch → CGB corpora** (XL+M+M+M) — the
   second machine; the bulk of remaining accuracy work, all back-loaded.
8. **SGB super functions (L), link cable (M–L), then the mapper/peripheral tail**
   (MBC7/MBC6/HuC/MMM01/Camera/TAMA5) — the completionist long tail.

## Key files

- CPU (at ceiling, independently verified): `crates/sharp-lr35902/src/{lib,alu,cb,reg,flags}.rs`, `crates/sharp-lr35902/src/opcodes/`, `crates/sharp-lr35902/tests/single_step_tests.rs`; STOP-fidelity enhancement at `src/opcodes/misc.rs:49-58`.
- PPU (dmg-acid2 at ceiling; Mealybug is the long pole): `crates/nintendo-game-boy-ppu/src/{lib,fetcher,fifo,sprite}.rs`; Mealybug diagnostic + dmg-acid2 reference test at `crates/machine-nintendo-game-boy/src/tests.rs:1204` and `:1326`/`:1404`.
- APU (at DMG ceiling): `crates/nintendo-game-boy-apu/src/{lib,square,wave,noise}.rs`; suite gate `crates/machine-nintendo-game-boy/src/tests.rs:1261`.
- Timer/interrupts (mooneye TIMA-reload passes): `crates/nintendo-game-boy-timer/src/lib.rs`.
- Machine wiring / memory map / OAM-DMA / serial: `crates/machine-nintendo-game-boy/src/lib.rs` (bus `:564-598`, IO `:411-477`, OAM-DMA `:479-530`, serial `:442-546`).
- Cartridge format + cart-type gaps: `crates/format-nintendo-game-boy-cartridge/src/lib.rs` (ROM+RAM `:205-211`, rejected mappers `:268-289`).
- MBCs (RTC advance gap; MBC1M): `crates/nintendo-game-boy-mbc/src/{mbc1,mbc2,mbc3,mbc5}.rs` (RTC `mbc3.rs:14`).
- Runtime / models / `.sav` / CGB-deferred: `crates/runtime-nintendo-game-boy/src/{profiles,runtime}.rs`; system harness `crates/runtime-nintendo-game-boy/tests/phase2_verification.rs`.
- Status docs to correct: `docs/status/outstanding-work.md:113-130`, `knowledge/systems/nintendo-game-boy/overview.md`.
- Test corpora (all present): `/Users/stevehill/Projects/198x/assets/test-suites/gameboy/{blargg,mooneye-test-suite,dmg-acid2,mealybug-tearoom,v1,v2}`.
- Reference: SameBoy (`/Users/stevehill/Projects/198x/emulators/gameboy/SameBoy`), Pan Docs; `/Users/stevehill/Projects/198x/reference/by-system/nintendo-gameboy`.


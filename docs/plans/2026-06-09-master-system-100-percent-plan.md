---
title: "plan: Sega Master System to 100% — cart RAM/saves, control ports, BIOS boot, mapper breadth, VDP/PSG depth"
type: plan
date: 2026-06-09
system: docs/systems/sega/master-system.md
basis: code-grounded survey of machine-/runtime-/emu198x-sega-master-system + sega-vdp + ti-sn76489 + zilog-z80, four shared-chip assessments, live test runs, and cross-check against reference/by-system/sega-master-system/sms-reference.md and reference/by-topic/vdp-sms/vdp-sms-reference.md, 2026-06-09
---

# Sega Master System — road to 100%

What it would take to bring the Master System to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests, cross-checked
against the chip-level VDP/PSG references and the system reference. Where the
in-tree docs drift from the code I say so below.

## Executive summary

**The Master System is a fourth distinct shape: a console whose *one library it
can render* boots cleanly, but whose system layer is the thinnest of the
finished cores.** The Spectrum was finished-core-plus-cheap-breadth; the C64 hides
a VIC-II core long pole; the NES is finished-everywhere-plus-two-bugs. The SMS is
**a working Mode-4 renderer wired to an at-ceiling Z80, sitting on a system layer
that is missing the parts that make a *console* a console** — battery saves,
control-port logic, BIOS boot, and mapper breadth.

The CPU is not the problem. `zilog-z80` is at the ceiling — Tom Harte
1,604,000/1,604,000, FUSE 1351/1356 exact (5 accepted), isa_conformance green
(re-verified 2026-06-09). The only Z80 gap, real IM0 acknowledge
(`crates/zilog-z80/src/z80.rs:977-981`), is latent for every Z80 system and the
SMS uses IM1 — so **no SMS-specific CPU work**.

The long pole is **not** a single chip rewrite. It is a **breadth-plus-depth
spread across the system layer and the two Sega chips**: the machine
(`crates/machine-sega-master-system/src/lib.rs`) explicitly stubs cart RAM to
`$FF` (lib.rs:290-296) and implements none of the control ports `$3E`/`$3F`; the
runtime carries no `.sav`, no BIOS, no header/SMD parsing, and a snapshot that
discards all live state; the `sega-vdp` chip is a strong Mode-4 pipeline with
~8 documented behaviours unimplemented or wrong; and `ti-sn76489` carries a
period-0 bug. None of these individually is a VIC-II-scale rewrite, but together
they are the bulk of the work.

What a learner ships today: a single SMS cart that runs in Mode 4 with a
joypad, mono PSG, and Pause→NMI. "100%" means save-game RPGs persist, the Light
Phaser works, Codemasters and Japanese-FM carts run, BIOS-boot titles see their
splash, and the VDP passes the raster-effect cases (Sonic parallax, Out Run road
split).

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | cart RAM + `.sav` persistence, port `$3E`/`$3F` control, header/SMD parse + checksum, live-state snapshot, mapper-from-header dispatch, doc fixes | **~3–5 weeks** |
| B — VDP/PSG core accuracy | the SMS-specific VDP defects (latch reset, R9 latch, VSI, H-counter), live H-counter wiring, line-IRQ raster verification, SN76489 period-0 clamp | **~3–4 weeks** |
| C — Peripherals + region | Light Phaser (TH-latch + H-counter), region/TV-system port bits, Sports Pad / Paddle, SegaScope 3-D | **~3–4 weeks** |
| D — Preservation breadth | YM2413 FM, Codemasters + Korean mappers, SMS BIOS images + Snail Maze/built-ins, Game Gear `.gg` smoke + stereo, SMS2 224/240-line modes, sprite MAG | **~6–9 weeks** |

**True 100% of everything ≈ 15–22 weeks.** It is **front-loaded onto Tier A** —
the cheapest, highest-leverage work is the system-layer gaps that stop whole
genres (save RPGs) and peripherals (light gun) from working at all. Tier D (FM,
exotic mappers, BIOS built-ins) is the JP/preservation long tail. The
launch-relevant "feels complete" slice (Tier A + the audible/visible parts of B)
is ~5–8 weeks.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (system layer; front-loaded)

| Item | Effort | Notes |
|------|--------|-------|
| **Cart RAM (SRAM) read/write path** | **M** | `mem_read` returns `$FF` for `$8000-$BFFF` whenever `$FFFC` bit 3 is set (`machine-sega-master-system/src/lib.rs:290-296`) and `mem_write` never routes there. Add the 16 KB (32 KB on Sonic 2 BR) battery-RAM window with the bank-select bit. Reference §Cartridge Save RAM: Phantasy Star, Wonder Boy III, Ys, Golden Axe Warrior, Miracle Warriors. Without it these RPGs cannot save. The machine doc-comment (lib.rs:289-290) admits the stub. |
| **`.sav` battery persistence** | **S–M** | Profile declares `WritebackPolicy::InMemoryOnly` (`runtime-sega-master-system/src/profiles.rs:88`) and nothing loads/flushes a `.sav`. Mirror the Game Boy `.sav` sidecar pattern (cited working in current-system-usability.md:50). Load-on-insert + flush-on-eject/exit, sized to the window. Rides on the cart-RAM item. |
| **Port `$3E` / `$3F` control registers** | **M** | `io_write` (lib.rs:333-345) handles only GG `$06`, PSG `$40-$7F`, VDP `$80-$BF`. Reference §I/O Port Map: `$3E` = memory control (BIOS/cart/IO enable bits), `$3F` = I/O control (TH-pin direction/level — the Light Phaser latch gate and a common region-detect side-channel). Unblocks both BIOS boot and the light gun. |
| **Header + SMD parse + checksum, in the runtime** | **S–M** | SMD-header stripping lives only in the *ignored* `tests/cart_boot.rs:62-67`; the production `insert_cartridge` path (`runtime-sega-master-system/src/runtime.rs:51`) takes raw bytes with no `$7FF0` "TMR SEGA" header read, no region/size decode, no checksum. Parse the header (reference §Cart-detect logic, `$7FF0-$7FFF`) to drive mapper choice, region default, and the BIOS checksum gate. |
| **Live-state snapshot** | **M** | `runtime-sega-master-system/src/snapshot.rs` serialises only `cart_bytes` + `time`; restore *rebuilds the machine from scratch* (`rebuild_after_restore` → `Sms::new`), discarding CPU/VDP/PSG/RAM/mapper state. `sega-vdp` (lib.rs:657) and `ti-sn76489` (lib.rs:329) already expose `save_state`/`load_state`; the Z80 + RAM + mapper_regs + ports need a serialiser. Today a snapshot is a soft-reset, not a save-state. Outstanding-work.md:1070 calls snapshot "deferred (shared family pattern)" — that understates it: the envelope exists but is hollow. |
| **Mapper-from-header dispatch scaffold** | **S–M** | Only the Sega standard mapper exists, hardwired in the machine (`mem_write` $FFFC-$FFFF, lib.rs:303-313). Add a mapper-selection seam so Codemasters/Korean variants (Tier D) can slot in without a machine rewrite. Reference §Codemasters / §Korean Mapper Variants. |
| **Doc-drift fixes** | **S** | (1) machine lib.rs:72 claims "sega-vdp exposes only `tick_scanline()` (not per-dot tick)" — false; `tick()` exists (sega-vdp lib.rs:359) and the machine calls it (lib.rs:209). Outstanding-work.md:1050-1051 already closed this but the source comment is stale. (2) sega-vdp lib.rs:18-19 claims legacy TMS9918 modes are "retained for SG-1000 backward compatibility" — false twice over (SG-1000 uses `ti-tms9918`, not this crate; and the legacy modes are stubbed here — see Tier D). |

## Tier B — VDP / PSG core accuracy

Shared-chip defects (established in the chip assessments, not re-derived). Only
the SMS consumes `sega-vdp`, so every VDP gap below lands on the Master System
alone.

| Item | Effort | Notes |
|------|--------|-------|
| **Control-port latch reset bug** | **S** | `read_data`/`write_data` both set `latch_first = true` (sega-vdp lib.rs:236, 245); reference (vdp-sms-reference.md:81-84) says only a status read or RES clears the first-write latch, never a data-port access. Desyncs code that interleaves a data access between the two control bytes. -> accuracy + bug. |
| **R9 vertical-scroll latch** | **S–M** | `mode4_bg_lookup` reads `regs[9]` live per pixel (lib.rs:503); reference (vdp-sms-reference.md:487-490) says R9 is sampled once at active-display start — a mid-frame write must wait for next frame. -> accuracy + bug. |
| **Vertical Scroll Inhibit (R0 bit 7)** | **S** | Columns 24-31 must lock to scroll-Y=0 when set (vdp-sms-reference.md:130, 391); the code decodes R0 bits 5 and 6 but never bit 7 (lib.rs:497, 504). -> accuracy + bug. |
| **H-counter live wiring + read** | **M** | `h_counter` is a permanent stub — never written (declared lib.rs:95, only read at lib.rs:321-322), so `read_h_counter()` always returns 0. Wire it through the per-dot `tick()` and the `$3F` TH-latch (Tier C ties in). Breaks the Light Phaser raster read and any `$7F` H-position read. -> accuracy + bug. |
| **SN76489 period-0 clamp** | **S** | `tick()` reloads `tone_counter` with period even when 0 (ti-sn76489 lib.rs:189-194); reference (psg-sn76489-reference.md:144-146, 471-473) says N=0 behaves as N=1. Affects every consumer including SMS; also propagates into noise mode 3. -> accuracy + bug. |
| **Line-IRQ raster-timing verification** | **M** | The line/frame-interrupt counter is processed at line end in `advance_line()`; the machine ticks the VDP at the 3:2 dot phase (machine lib.rs:204-211). Whether line-IRQ timing is cycle-correct for tight raster splits (Sonic parallax, Out Run road split) is **unverified by code** — needs a real ROM or reference-emulator trace. Build a raster-split harness before claiming it. |

## Tier C — Peripherals + region detection

| Item | Effort | Notes |
|------|--------|-------|
| **Light Phaser** | **M–L** | The marquee SMS peripheral. Needs three pieces that currently don't exist: port `$3F` TH-pin handling (Tier A), the VDP H-counter latch on TH falling edge into `$7F` (Tier B H-counter), and pixel-brightness sampling so the photodiode "sees" the reticle (reference §Light Phaser, vdp-sms-reference.md:451-455). Safari Hunt, Gangster Town, Rambo III, Operation Wolf. |
| **Region + TV-system port bits** | **S–M** | `io_read` returns `port_dd` raw (always `0xFF`, machine lib.rs:328) and `$00` returns `0xFF` on non-GG (lib.rs:329); no region bit. Reference §Region Locking: port `$00`/`$DD` bit 7 = Japan(0)/Export(1); some VDP revisions expose an NTSC/PAL bit. Late arcade ports refuse to run on the wrong region. -> accuracy. |
| **Sports Pad / Paddle / Handle** | **M** | TH-driven quadrature/nibble-clock controllers (reference §Other Peripherals). Reuse the `$3F` TH machinery. Great Ice Hockey, Woody Pop (JP). -> enhancement. |
| **SegaScope 3-D Glasses** | **M** | Card-slot shutter signal toggled per vblank (reference §SegaScope). Eight games (Space Harrier 3-D, Out Run 3-D, Zaxxon 3-D…). -> enhancement. |

## Tier D — Preservation breadth (back-loaded; JP / exotic)

| Item | Effort | Notes |
|------|--------|-------|
| **YM2413 FM (OPLL)** | **L–XL** | New chip crate — 9-channel 2-op FM at ports `$F0/$F1/$F2`, Japanese SMS / Mark III FM Sound Unit (reference §YM2413). ~70 JP games (Phantasy Star FM, Aleste, Power Strike II). Outstanding-work.md:1067 marks it out-of-scope; it is the heaviest single SMS item. -> enhancement. |
| **Codemasters mapper** | **M** | Bank registers at `$0000/$4000/$8000` (writes to ROM space, reads return ROM), header at `$7FE0-$7FEF` (reference §Codemasters). Micro Machines, Fantastic Dizzy, Cosmic Spacehead. Rides the Tier-A dispatch seam. -> enhancement. |
| **Korean mapper variants** | **M** | MSX-style `$A000`, 8 KB-page, Janggun, Nemesis, 4-Pak (reference §Korean Mapper Variants). Preservation footnotes. -> enhancement. |
| **SMS BIOS boot path + built-ins** | **L** | No BIOS support at all — the machine jumps cart `$0000` directly. Add BIOS-ROM load, the `$7FF0` header/checksum validation gate, cart/card detect, the `$3E` BIOS-disable handoff, and the Snail Maze / Hang-On+Safari Hunt built-ins (reference §BIOS Variants). `firmware: vec![]` in profiles.rs:82 today. -> enhancement. |
| **Game Gear `.gg` smoke + true stereo** | **M** | GG variant is wired (160×144 crop, `$06` stereo routed at machine lib.rs:335) but `take_buffer_stereo` duplicates mono (ti-sn76489 lib.rs:273-283 — panning stored, never applied), and there is no `.gg` cart smoke test (outstanding-work.md:1071-1074). Per-channel buffers exist to do real panning. GG is not in the launch list. -> enhancement. |
| **SMS2 224/240-line modes + sprite MAG** | **M** | `active_lines` hardcoded 192 (sega-vdp lib.rs:393), NT vertical wrap hardcoded 224 (lib.rs:506), `$D0` terminator fires unconditionally (lib.rs:578); R1 M1/M3 not decoded and the `variant` field is dead (lib.rs:104). Sprite MAG (R1 bit 0) and the SMS1 sprite-zoom bug unimplemented (lib.rs:559-644). Few games (224-line: some late EU titles). -> enhancement. |
| **OVR 9th-sprite index in status** | **S** | Overflow sets only status bit 6 (sega-vdp lib.rs:589); SMS2 Mode 4 should report the 9th sprite index in status bits 4:0 (vdp-sms-reference.md:174). Niche. -> enhancement. |
| **Legacy TMS9918 modes (Graphics I/II, Text, Multicolor)** | **M** | `bg_pixel` returns backdrop for any non-Mode-4 path (sega-vdp lib.rs:487-489); only Mode 4 renders. Real exposure is tiny (commercial SMS software is Mode 4; SG-1000 uses `ti-tms9918` not this crate) but the doc claims they are "retained" — implement or correct the claim (paired with the Tier-A doc fix). -> enhancement. |

## Done as part of this plan (free, ~half a day)

- **Source doc-drift corrected** in two places the survey caught: the stale
  "sega-vdp exposes only tick_scanline()" comment (machine lib.rs:72) and the
  false "legacy modes retained for SG-1000 compatibility" claim (sega-vdp
  lib.rs:18-19) — neither matches the code.
- **Snapshot reality recorded**: outstanding-work.md:1070 frames snapshot as
  "deferred (shared family pattern)"; the envelope actually exists but stores
  only cart+time, so a "restore" is a reset. Re-scoped as a Tier-A item, not a
  deferral.
- **Test inventory captured** (re-run 2026-06-09, all green): machine 10 unit +
  1 `#[ignore]` cart-boot smoke; runtime 2; sega-vdp 13; ti-sn76489 11. No
  failing or flaky SMS tests; the only ignored test needs a cart ROM not shipped
  in-tree.

## Recommended sequence (highest leverage first)

1. **Cart RAM + `.sav` persistence** (M + S–M) — the one Tier-A gap that stops a
   whole genre (save RPGs) dead. Highest leverage per week.
2. **Port `$3E`/`$3F` control + header/SMD parse + checksum** (M + S–M) — the
   shared substrate under BIOS boot, region detect, and the light gun; and the
   production load path needs the header it currently ignores.
3. **Live-state snapshot** (M) — turn the hollow envelope into a real save-state
   now the chips already expose `save_state`/`load_state`.
4. **SN76489 period-0 clamp + the four VDP accuracy bugs** (S×5) — cheap
   correctness wins (latch reset, R9 latch, VSI, period-0) with real game impact.
5. **H-counter wiring → Light Phaser** (M then M–L) — the H-counter unblocks both
   the `$7F` read and the gun; do them as one thread.
6. **Region/TV-system port bits** (S–M) — cheap, stops wrong-region refusals.
7. **Line-IRQ raster verification** (M) — build the split-screen harness, then
   confirm or fix against Sonic / Out Run.
8. **Mapper dispatch → Codemasters → Korean** (S–M + M + M) — the affordable
   breadth tail once the seam exists.
9. **SMS2 224/240 modes, sprite MAG, OVR index, legacy TMS modes** (M/M/S/M) —
   VDP completeness.
10. **YM2413 FM** (L–XL), **SMS BIOS + built-ins** (L), **GG stereo + `.gg`
    smoke** (M), **Sports Pad / Paddle / SegaScope** (M×3) — the JP / preservation
    long tail.

## Key files

- CPU (at ceiling, no SMS work): `crates/zilog-z80/src/z80.rs`, tests
  `crates/zilog-z80/tests/{z80_fuse,isa_conformance}.rs`.
- Machine wiring: `crates/machine-sega-master-system/src/lib.rs` — cart-RAM stub
  (`mem_read` :290-296), mapper writes (`mem_write` :303-313), I/O (`io_read`
  :316-331 / `io_write` :333-345, no `$3E`/`$3F`), VDP tick (:204-211), stale
  tick_scanline comment (:72). Tests: `tests/cart_boot.rs` (ignored smoke, SMD
  strip at :62-67).
- Runtime: `crates/runtime-sega-master-system/src/{runtime.rs,snapshot.rs,profiles.rs,input.rs}`
  — `insert_cartridge` (runtime.rs:51, no header parse), hollow snapshot
  (snapshot.rs), `WritebackPolicy::InMemoryOnly` + `firmware: vec![]`
  (profiles.rs:82-88).
- VDP: `crates/sega-vdp/src/lib.rs` — latch reset (:236,245), R9 live read
  (:503), VSI gap (:497,504), H-counter stub (:95,321-322), `save_state`
  (:657), `active_lines`/wrap hardcodes (:393,506), sprite eval (:559-644),
  legacy-mode stub (:487-489), stale doc (:18-19).
- PSG: `crates/ti-sn76489/src/lib.rs` — period-0 (:189-194), stereo stub
  (:273-283), `save_state` (:329).
- Reference: `reference/by-system/sega-master-system/sms-reference.md`,
  `reference/by-topic/vdp-sms/vdp-sms-reference.md`,
  `reference/by-topic/.../psg-sn76489-reference.md`; reference emulators MEKA /
  Emulicious for VDP/mapper edge cases.


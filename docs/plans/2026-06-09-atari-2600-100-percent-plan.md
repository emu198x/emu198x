> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Atari 2600 (VCS) to 100% — audio from silence, TIA cycle-exactness, peripherals and the cartridge long tail"
type: plan
date: 2026-06-09
system: docs/systems/atari/atari-2600.md
basis: code-grounded survey of machine-atari-2600, atari-tia, mos-riot-6532, runtime-atari-2600 with live test runs + shared 6502/TIA chip findings, 2026-06-09
---

# Atari 2600 (VCS) — road to 100%

What it would take to bring the Atari 2600 to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and a live test run (45 unit
tests across the four crates pass; the one cart-boot smoke is `--ignored`,
needs a ROM). The shared 6502 and TIA chip assessments are taken as established
and not re-derived here — this plan is the **system** view: machine wiring,
cartridge formats, peripherals, audio plumbing, and the 2600-specific tests.

## Executive summary

**The 2600 is a fourth distinct shape: the core *boots and plays* but is silent,
and its accuracy ceiling lives almost entirely in one chip.** Unlike the C64
(hard core long pole) or the NES (finished core + cheap breadth), the 2600's
hard part and its breadth are the *same* component — the TIA — because the 2600
has no framebuffer, no sound chip, and barely any RAM. The machine layer is
small and correct; the RIOT is solid (11/11 tests, including the documented
INTIM-prescaler silicon fix at `mos-riot-6532/src/lib.rs:149-163`); the CPU is
at ceiling (shared finding: `mos-6502` is externally verified, NMOS variant).

What is **done**: the 6507/TIA/RIOT wiring with the distinctive A12/A7/A9 decode
(`machine-atari-2600/src/lib.rs:142-162`), the master-colour-clock tick with
CPU+RIOT at 1/3 rate and WSYNC halt (`lib.rs:122-140`), F8/F6/F4 bank switching
by hotspot (`cartridge.rs:82-108`), joystick + console-switch input through the
RIOT (`runtime-atari-2600/src/input.rs`), paddle capacitor timing on INPT0-3
(`atari-tia` + `input.rs:73-77`), NTSC+PAL framebuffers, collision latches, and
save/load state. Combat boots to the canonical two-tank playfield.

What is **the long pole**: the TIA. **Audio is fully stubbed** — the AUDx
register writes are empty arms (`atari-tia/src/lib.rs:808-813`) and the runtime
emits empty audio packets by design (`runtime-atari-2600/src/runtime.rs:204-211`),
so *every* game is silent. On top of that sit the established TIA accuracy gaps:
RESP/RESM/RESBL pipeline delay missing, HMOVE-bar artefact absent, the VDELP
double-buffer over-latching, suspect player-NUSIZ stretch logic, PAL hue-1 grey,
and the INPT4/5 fire-button **latch mode** unimplemented.

What is **system-specific debt the chip findings do not cover**: fire buttons
are **dead end-to-end** — the TIA exposes `set_inpt4`/`set_inpt5`
(`atari-tia/src/lib.rs:965-973`) but the machine layer never wires them and the
runtime explicitly defers it (`input.rs:59-60`: "Defer wiring until
machine-atari-2600 exposes set_inpt4/5"). And the cartridge layer infers banking
**purely from ROM length** (`cartridge.rs:33-41`), so it covers only F8/F6/F4 —
the large E0/FE/3F/FA/FE/3E and Superchip (cart-RAM) tail is absent, and
length-inference is wrong for several real carts.

So "100% 2600" is **audio from scratch + a TIA cycle-exactness pass + fire-button
and peripheral wiring + the cartridge-mapper long tail** — front-loaded onto two
high-value items (audio, fire buttons) that unblock playability, then a genuinely
hard TIA accuracy tier.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | TIA audio synthesis + runtime drain, fire-button wiring (machine + runtime), Superchip cart-RAM + the common mappers (E0/FE/3F/3E), system-doc fixes | **~3–5 weeks** |
| B — TIA cycle-exact core | RESP/RESM pipeline delay, HMOVE-bar + 8-clock injection, VDELP latch fix, NUSIZ player/missile fix, WSYNC-in-HBLANK no-op, a TIA oracle harness (Stella/test ROMs) | **~5–8 weeks** |
| C — Audio + palette fidelity | the two polynomial/pure-tone generators to Stella grade, 6-bit volume mixing + host resample, PAL hue-1 (and full PAL chroma) correction | **~2–3 weeks** |
| D — Preservation breadth | the cartridge long tail (3F/FA/FE/E7/EF/DPC/ARM-less bankswitch zoo), .a26 header + ROM database disambiguation, driving/keypad/booster-grip controllers, region auto-detect, SaraRAM/Pitfall-II DPC | **~5–8 weeks** |

**True 100% of everything ≈ 15–24 weeks.** It is **front-loaded onto Tier A**:
audio and fire buttons are the difference between "renders" and "playable", and
both are cheap relative to the TIA accuracy tier. Tier B (the cycle-exact TIA) is
the hard long pole — the same shape as the C64's VIC-II rewrite, and like that
one it should be built behind an oracle harness, not by vibes.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (playability; do first)

| Item | Effort | Notes |
|------|--------|-------|
| **TIA audio synthesis** | **L** | The headline gap. `atari-tia/src/lib.rs:808-813` — all six AUDx writes are empty arms; the chip produces no sound. Implement the two channels: 4-bit AUDC waveform/polynomial selector (the 16-mode table at `tia-reference.md:320-335`), 5-bit AUDF divider, 4-bit AUDV volume, clocked at colour-clock ÷ 114 (~30 kHz, one audio clock per 2 scanlines — `tia-reference.md:339-340`, note 9 at `:460`). Output an internal sample buffer. **Every game is silent without this.** |
| **Runtime audio drain** | **S–M** | `runtime-atari-2600/src/runtime.rs:204-211` pushes an *empty* `AudioPacket` every frame by design. Once the TIA exposes a sample buffer, drain it, resample to the 48 kHz sink, and push real samples. Pairs with the chip item above. |
| **Fire-button wiring (machine + runtime)** | **S** | Confirmed dead end-to-end. The TIA has `set_inpt4`/`set_inpt5` (`atari-tia/src/lib.rs:965-973`) but `machine-atari-2600/src/lib.rs` exposes **no** method to reach them, and `runtime-atari-2600/src/input.rs:59-60` explicitly defers: "Defer wiring until machine-atari-2600 exposes set_inpt4/5." Add `Atari2600::set_fire(port, pressed)` → `tia.set_inpt4/5`, then route the `fire`/`fire2` host button names in `input.rs`. The query surface already reads INPT4/5 (`queries.rs:57-58`) but nothing can set them — so fire reads are permanently "unpressed" today. |
| **Superchip / cart-RAM (FxSC)** | **M** | The Superchip adds 128 bytes of cart RAM (write `$1000-$107F`, read `$1080-$10FF`) on top of F6/F8/F4. `cartridge.rs` has no RAM at all. Many bigger commercial carts (and the *SC variants) need it. |
| **Common extra mappers — E0, FE, 3F** | **M** | `cartridge.rs:33-41` covers only None/F8/F6/F4. E0 (Parker Bros, 8K, 1K segments), FE (Activision, 8K via stack-sense), 3F (Tigervision, 8K+ via `$3F` write) cover a meaningful slice of the commercial library that simply won't bank correctly today. |
| **System-doc fixes** | **S** | `docs/status/outstanding-work.md:194-195` lists stale LoC/test counts (says "1204 LoC", "13/13 tests" for the TIA; the crate is now 1359 LoC and **15/15**; RIOT "477 LoC, 11/11" — file is 525 LoC, 11/11 confirmed). The status doc's "audio mixing refinements are in the accuracy backlog" understates: audio is *fully stubbed*, not refinement-pending. |

## Tier B — TIA cycle-exact core (the long pole; build behind an oracle)

These are the established chip-level TIA findings, scoped and sequenced. Build a
**TIA oracle harness first** (per-frame/per-cycle comparison against Stella or
the TIA test ROMs) so the accuracy work is provable, exactly as the C64 plan
front-loads the VIC-II comparator.

| Item | Effort | Notes |
|------|--------|-------|
| **RESP/RESM/RESBL pipeline delay** | **M** | `lib.rs:803-807` set object position to `hpos − HBLANK_CLOCKS` with **no** pipeline offset; real TIA lands the object ~4–5 colour clocks later (`tia-reference.md:241-245`, note 3 at `:432`). Every strobed sprite is mis-positioned by ~5 px. Exact offset needs ROM verification but the omission is unambiguous. |
| **HMOVE 8-clock injection + HMOVE-bar artefact** | **M–L** | `apply_hmove` (`lib.rs:918-924`) mutates positions instantly and `lib.rs:473` blanks the first 8 visible pixels unconditionally on `hmove_pending`, regardless of *when* in the line HMOVE was strobed. Real TIA injects 8 extra colour clocks and only shows the comb when HMOVE is written outside HBLANK (`tia-reference.md:304-308`, note 4 at `:437`: Pitfall exploits this — do not filter it out). Also reconcile the sign convention (`decode_hmove` `lib.rs:1145-1155` vs reference table `:297-301`). |
| **VDELP/VDELBL double-buffer latch fix** | **S–M** | `lib.rs:814-827`: writing GRP0 sets `grp0_old` from the new GRP0 *and* `grp1_old` in the same write — over-latching. Reference note 7 (`:450-454`) prescribes "latch the *other* player on write", not "delay by one write". Verify against a 2-frame-kernel ROM. |
| **NUSIZ player/missile stretch correction** | **S** | `lib.rs:580-585` derives a player width from NUSIZ bits 4:5 then discards it via `width.min(1)` (`:600-604`); bits 4:5 are the **missile** size, copy/size lives in bits 2:0 (`tia-reference.md:209-220`). Dead, misleading logic — remove it and drive player stretch from the right bits. |
| **WSYNC-during-HBLANK no-op + open-bus read bits** | **S** | `lib.rs:783-785` sets `wsync_halt` unconditionally; writing WSYNC during HBLANK is a no-op (`tia-reference.md:358`). Separately, collision/INPT reads (`read`, `lib.rs:877-895`) return the latch in the low bits too; on real TIA only bits 6-7 are driven and the rest float (data-bus retention). Both are edge-case kernel correctness. |
| **TIA oracle harness** | **M** | Per-cycle/per-frame comparator against Stella (in `emulators/atari/`) or the TIA test ROMs. Built *alongside* the rewrite, not after. |

## Tier C — Audio + palette fidelity

| Item | Effort | Notes |
|------|--------|-------|
| **Polynomial/pure-tone generators to Stella grade** | **M** | Beyond "make sound" (Tier A): the exact 4-bit/5-bit/9-bit polynomial LFSR taps and the divide-by-N modes (`tia-reference.md:320-335`) so noise and tone match hardware, not an approximation. |
| **Volume mixing + faithful host resample** | **S–M** | Two-channel 4-bit linear volume sum (`:343-345`) with a correct ÷114 → 48 kHz resampler in the runtime drain. |
| **PAL palette — hue 1 (and full chroma audit)** | **S** | `palette.rs:77-79`: PAL hue 1 is a verbatim copy of hue 0's grey ramp instead of a real PAL chroma. Any PAL game using colour register `$1x` renders grey. Audit the whole PAL table against a reference while here. |
| **Region-accurate vertical timing** | **S–M** | Machine uses fixed `lines_per_frame` (`lib.rs:68-73`) and the TIA detects VSYNC-deassert for frame end (established finding note 10). Acceptable now; cycle-exact vertical timing is a completeness item. |

## Tier D — Preservation breadth (back-loaded)

| Item | Effort | Notes |
|------|--------|-------|
| **The cartridge-mapper long tail** | **L–XL** | Beyond Tier A's E0/FE/3F: 3E (RAM+ROM banking), FA/CBS RAM+, E7 (M-Network), EF/EFSC, UA, 0840, plus DPC (Pitfall II's on-cart co-processor) and the SARA/Superchip variants of each base scheme. Each is a small self-contained mapper but there are many. |
| **.a26 header + ROM database disambiguation** | **M** | Banking is inferred purely from ROM *length* (`cartridge.rs:33-41`) — wrong for several real carts where length is ambiguous (e.g. 8K that is FE vs F8 vs E0). Add `.a26`/header parsing and/or a cart-database (CRC → scheme) so detection is correct, not size-guessed. No `MediaKind`-level format crate exists for the 2600 today (runtime takes raw cart bytes, `runtime.rs:152-168`). |
| **Driving / keypad / booster-grip / Genesis controllers** | **M** | Only digital joystick + paddles are wired (`input.rs`). Driving controllers (gray-code quadrature on the same lines), keypad/keyboard controllers, and the booster grip are the 2600 peripheral tail. |
| **INPT4/5 latched fire-button mode (VBLANK bit 6)** | **S** | Established chip finding #8: VBLANK write (`lib.rs:775-782`) handles bit 7 (paddle dump) but ignores bit 6, the fire-button latch enable. Lands naturally once Tier A wires the fire buttons. |
| **Region auto-detection** | **S** | Region is a `Model` choice (`profiles.rs`); no TV-standard heuristic from frame-line-count. A "feels complete" nicety, not load-bearing. |

## Done as part of this plan (free, ~half a day)

System-doc touch-up in `docs/status/outstanding-work.md:191-241`: correct the
stale LoC/test counts (TIA is 1359 LoC / **15/15**, not "1204 / 13/13"; RIOT is
525 LoC / 11/11), and re-grade the audio line — it currently reads "audio mixing
refinements are in the accuracy backlog" and "AUDx registers latch", which
**overstates** the state: the AUDx writes are *empty arms* (no latch, no
generator), so audio is fully stubbed, not refinement-pending. Also note the
fire-button wiring gap, which the status doc does not mention at all.

## Recommended sequence (highest leverage first)

1. **Fire-button wiring** (S) — cheapest playability win; the chip is ready, only
   the machine method + runtime route are missing. Fixes a confirmed dead path.
2. **TIA audio synthesis + runtime drain** (L + S–M) — the single biggest "feels
   like a real 2600" item; every game is silent today.
3. **Superchip cart-RAM + E0/FE/3F mappers** (M + M) — the breadth that stops
   real commercial carts banking wrong.
4. **System-doc fixes** (S) — eradicate the stale counts and the audio overstatement.
5. **TIA oracle harness** (M) — build the comparator *before* the accuracy rewrite.
6. **RESP/RESM pipeline delay → HMOVE bar/injection → VDELP latch → NUSIZ fix →
   WSYNC/open-bus** (M/M-L/S-M/S/S) — the cycle-exact TIA tier, oracle-gated.
7. **Polynomial generators + volume mixing + PAL hue-1** (M/S-M/S) — audio and
   palette fidelity to Stella grade.
8. **Cartridge long tail + .a26/database + extra controllers + latched fire +
   region auto-detect** (L-XL/M/M/S/S) — the preservation completionist tail.

## Key files

- CPU (at ceiling, shared finding): `crates/mos-6502/src/{lib,cycle,tick}.rs` (NMOS variant `M6502::new()`); no 2600 CPU work.
- Machine wiring: `crates/machine-atari-2600/src/lib.rs` (`tick_colour_clock` `:122-140`, A12/A7/A9 decode `:142-162`, no fire-button method — add `set_fire`).
- Cartridge: `crates/machine-atari-2600/src/cartridge.rs` (size-inferred banking `:33-41`, hotspots `:82-108`; add cart-RAM + the mapper zoo).
- TIA (the long pole): `crates/atari-tia/src/lib.rs` (audio stub `:808-813`, RESP `:803-807`, HMOVE `:918-924` + blank `:473`, VDELP `:814-827`, NUSIZ `:580-604`, WSYNC `:783-785`, INPT4/5 setters `:965-973`, `decode_hmove` `:1145-1155`) and `crates/atari-tia/src/palette.rs:77-79` (PAL hue 1).
- RIOT (solid): `crates/mos-riot-6532/src/lib.rs` (INTIM silicon fix `:149-163`); no work expected.
- Runtime: `crates/runtime-atari-2600/src/runtime.rs:204-211` (empty audio packet — drain here), `src/input.rs:59-60` (deferred fire wiring), `src/queries.rs:57-58` (INPT reads with no write path), `src/profiles.rs` (region as model choice).
- Tests: `crates/machine-atari-2600/tests/cart_boot.rs` (`--ignored`, needs a cart); 45 in-crate unit tests pass.
- Reference: `reference/by-topic/tia/tia-reference.md` (audio `:316-345`, HMOVE `:294-308`, NUSIZ `:209-220`, positioning `:241-245`); Stella in `emulators/atari/`.
</plan_markdown>
<parameter name="needs_runtime_verification">["TIA RESP/RESM/RESBL exact pipeline-delay offset (~4-5 colour clocks) — the omission is unambiguous from code, but the precise value must be confirmed against a TIA positioning test ROM (e.g. Stella's player/missile alignment suites) before asserting the magnitude.", "HMOVE sign convention: decode_hmove (atari-tia/src/lib.rs:1145-1155) negates the nybble ($70 -> -7, $80 -> +8) while tia-reference.md:297-301 labels $70 -> +7 / $80 -> -8. Net screen direction may still be correct; needs an HMOVE positioning test ROM to confirm bug vs. mere mislabel.", "VDELP over-latch (atari-tia/src/lib.rs:814-827): the logic contradicts reference note 7's prescribed 'latch the other player' behaviour, but the observable divergence needs a 2-frame-kernel test ROM to confirm it actually mis-renders under real write patterns.", "TIA audio output: no audio test ROM was run (chip is fully stubbed). Once generators are implemented, correctness of the polynomial taps and the ÷114 audio clock must be verified against Stella or a known-good audio reference recording.", "Cartridge banking correctness for real commercial carts: the size-inference in cartridge.rs:33-41 was read but not exercised against an actual cart library; which titles mis-detect (8K FE vs F8 vs E0 ambiguity) needs a ROM-set sweep.", "cart_boot.rs smoke is #[ignore] (needs an Atari 2600 cart under ~/.emu198x/media/atari-2600/ or EMU198X_ATARI_2600_CART). Live boot of Combat was reported in outstanding-work.md:210 but could not be re-run here without the ROM.", "PAL palette beyond hue 1: palette.rs:77-79 is a confirmed grey-for-hue-1 copy, but a full PAL chroma audit against a colour reference was not performed — other hues may also drift."]

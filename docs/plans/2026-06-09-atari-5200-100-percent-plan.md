> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Atari 5200 SuperSystem to 100% — shared-chip accuracy, cart breadth, snapshot, peripherals"
type: plan
date: 2026-06-09
system: docs/systems/atari/atari-5200.md
basis: code-grounded survey of machine-atari-5200, runtime-atari-5200, the shared ANTIC/GTIA/POKEY/6502 crates, live test runs, and reference cross-check (mappingtheatari.md, atari-8bit-reference.md), 2026-06-09
---

# Atari 5200 SuperSystem — road to 100%

What it would take to bring the Atari 5200 to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and live test runs — not doc
prose. The machine layer is small and clean; almost all of the remaining accuracy
work lives in the **three shared chips it borrows from the 8-bit Atari line**, and
those are tracked at the chip level, not here. This plan covers the 5200's own
shape and the system-specific gaps.

## Executive summary

**The 5200 is a thin, correct shell over three shared chips that are each only
partially complete.** The machine wiring itself is in good shape: it boots Pac-Man
end-to-end to its menu with a real BIOS (per `docs/status/outstanding-work.md:269`),
the two-chip 16 KB cart decode is correct and tested
(`crates/machine-atari-5200/src/cartridge.rs:45-53`,
`sixteen_kb_two_chip_decode`), ANTIC DMAs from a full 64 KB bus image so display
lists and character sets in cart ROM/BIOS render (`lib.rs:103-140, 235`), the
analogue stick + keypad + fire are wired through POKEY pots / keyboard scan / GTIA
TRIG0 (`runtime-atari-5200/src/input.rs`), and audio is drained into the runtime's
audio sink (`runtime.rs:210-220`). Centipede is reported **fully drivable**
(`docs/status/drivability-assessment.md:276`). All 15 machine tests, 5 runtime
tests, and the shared-chip suites (ANTIC 19, GTIA 13, POKEY 14) pass; the only
machine-level integration test (`cart_boot.rs`) is `#[ignore]` because it needs a
real cart + BIOS on disk.

**The long pole is not 5200-specific — it is the three shared chips.** The single
largest visual gap is **ANTIC HSCROL/VSCROL fine scrolling**, which is decoded but
never consumed in the render path (registers stored, no reader); the largest other
visual gaps are **GTIA PRIOR priority schemes** (hardcoded default order) and the
**GTIA `$D014` PAL-flag inversion**; the two confirmed audio defects are **POKEY's
distortion table** (3 of 8 settings wrong) and **16-bit linked-channel period**.
All five are filed at the chip level and affect the 800XL identically — they are
**referenced** here, not re-filed.

What is genuinely 5200-*specific* and unfiled elsewhere: **bank-switched
cartridges** (Bounty Bob, AtariMax — the only carts that don't run at all today),
**`.a52`/`.car` header stripping** (so headered dumps load), a **real live-state
snapshot** (today's snapshot rebuilds from cart+BIOS and throws away CPU/chip/RAM
state), **the second fire button + multi-controller ports**, and **PAL palette +
`$D014` correctness as it surfaces on the 5200**.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | bank-switched carts (Bounty Bob / AtariMax), `.a52`/`.car` header stripping, doc-drift fix | **~1–1.5 weeks** |
| B — Cycle-exact core (mostly shared-chip; tracked at chip level) | ANTIC HSCROL/VSCROL + mode-3 + NMIST + per-line write timing; the machine-level DMA-interleave is a thin consumer of the ANTIC fix | **~chip-tracked + ~M of 5200 wiring** |
| C — Audio + analogue fidelity (shared-chip + 5200 wiring) | POKEY distortion table + 16-bit period (chip), then validate the 5200's POKEY-pot scan timing against real analogue-stick reads | **~chip-tracked + ~S–M of 5200 verification** |
| D — Preservation breadth + peripherals | live-state snapshot, second fire button + up to 4 controllers, PAL palette + `$D014`, bundled-BIOS / firmware story | **~2–3 weeks** |

**True 100% of everything ≈ chip-tracked depth + ~5–7 weeks of 5200-specific
work.** Unlike the C64, the 5200 has **no CPU work** — the `mos-6502` NMOS core it
uses as the "Sally" 6502C is at the externally-verified ceiling (Tom Harte
2.56 M, Klaus Dormann functional) and needs none. And unlike the C64 the
5200's own long pole is *shared* with the 800XL, so the depth investment is
amortised across two systems. The launch-relevant slice (Tier A) is small and
cheap; everything else is either chip-tracked or preservation tail.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — "Curriculum 100%" (cart breadth + loader)

| Item | Effort | Notes |
|------|--------|-------|
| **Bank-switched cartridges** | **M** | `cartridge.rs:3` declares "no bank switching" and `from_rom` rejects any size outside 4/8/16/32 KB (`cartridge.rs:27-33`). The 5200 has two real bank-switched families: **Bounty Bob Strikes Back** (40 KB; two switchable 4 KB windows at `$4000`/`$5000` selected by reads in `$4FF6-$4FF9` / `$5FF6-$5FF9`) and **AtariMax** 128 KB/512 KB flash carts. Without this, those titles don't load at all. The window-switch needs a write/read hook into `mem_read`/`mem_write` (currently `cart.read` is side-effect-free, `lib.rs:264,397`). |
| **`.a52` / `.car` header stripping** | **S** | The runtime takes raw cart bytes (`runtime.rs:50,68`) and `Cartridge::from_rom` validates on exact ROM size. Standard 5200 dumps carry a 16-byte `.car`/`.a52` header (`CART` magic + type + checksum); a headered file is the wrong size and is rejected. Strip a recognised header before sizing, and use its **cart-type byte** to disambiguate 16 KB two-chip vs linear and to drive the bank-switch mapper above. The `cart_boot.rs:19-25` size filter would also need the header length added. |
| **Doc-drift fix** | **S** | `docs/status/outstanding-work.md:254` says "machine-atari-5200, 14/14 tests" and `:247-248` says ANTIC 14/14, GTIA 9/9; live runs show **machine 15**, ANTIC 19, GTIA 13, POKEY 14. Correct the counts. |

## Tier B — Cycle-exact core (shared-chip long pole + thin 5200 wiring)

The substance here lives in `atari-antic` and affects the 800XL identically; it is
**filed at the chip level, not re-filed in this plan**. Listed so the 5200's
dependency on it is explicit.

| Item | Effort | Notes |
|------|--------|-------|
| **ANTIC HSCROL/VSCROL fine scrolling** (chip) | **chip-tracked** | The single biggest visual gap on the 5200. Registers are stored and the per-line enable flags decoded, but neither is read during rendering. A large fraction of 5200 scrollers (e.g. shooters, platformers) get no smooth scroll. Shared with 800XL. |
| **ANTIC mode 3, NMIST-latch, mid-line write timing** (chip) | **chip-tracked** | Mode-3 descenders render wrong; NMIST status is gated behind NMIEN (polling-with-NMI-off reads zero); whole-line up-front generation defers mid-line DMACTL/CHBASE/HSCROL writes to the next line. All shared with 800XL. |
| **Machine-level DMA interleave** | **M** | `lib.rs:214` gates the CPU with `cpu_dma_stalled(line_cycle, dma_budget)` — the budget is stolen spread evenly across the fetch window (the Bresenham approximation ANTIC documents at `atari-antic` `lib.rs:42-53`), not at true per-character fetch positions. This is the 5200's **own** consumer of the ANTIC timing model; once the chip exposes true fetch positions, wire them here. Self-admitted accuracy debt, not a defect (`docs/status/outstanding-work.md:319-322`). |
| **6502C "Sally" IRQ/NMI ROM cross-check** | **S (verify)** | The `mos-6502` core is at ceiling on Tom Harte + Dormann-functional, but its branch/interrupt edge cases are blargg-validated only at the *NES machine* layer. The 5200 drives NMI heavily (VBI + DLI). Confirm IRQ/NMI behaviour holds on this machine. Verification, not new code. |

## Tier C — Audio + analogue fidelity (shared-chip + 5200 verification)

POKEY is shared with the 800XL; the two confirmed defects are **filed at the chip
level**. The 5200-specific piece is verifying the pot-scan path drives the
analogue stick correctly.

| Item | Effort | Notes |
|------|--------|-------|
| **POKEY distortion table + 16-bit period** (chip) | **chip-tracked** | Three of eight AUDC distortion settings play the wrong noise (explosions/engine/percussion); 16-bit linked-channel mode computes `(L+1)(H+1)` instead of `(L + 256·H + 1)`, mistuning bass/precise tones. Both shared with 800XL. |
| **5200 pot-scan / analogue-stick verification** | **S–M** | The machine sets pots 0/1 from `set_joystick` (`lib.rs:319-322`) and POKEY scans one count per scan line (frame-granular, `atari-pokey` pot scanner). 5200 sticks are *non-self-centring* analogue — verify a game reading POTX/POTY tracks the host axis correctly across a frame and that centre (114) reads as centre. Likely fine; needs an on-machine check. |
| **5200 audio sample-rate / mono mix confirmation** | **S (verify)** | Runtime pushes mono 48 kHz from `take_audio_buffer` (`runtime.rs:211-220`). Confirm against a reference emulator that the single-POKEY mono mix matches; the 5200 is single-POKEY (no second chip wired, correct for the base console). |

## Tier D — Preservation breadth + peripherals

| Item | Effort | Notes |
|------|--------|-------|
| **Live-state snapshot** | **M** | The snapshot envelope (`runtime-atari-5200/src/snapshot.rs`) stores only `time`, `model_id`, `cart_bytes`, `bios_bytes`; `restore` calls `rebuild_machine`, which builds a **fresh** `Atari5200` and discards all CPU/ANTIC/GTIA/POKEY/RAM state. Save/load therefore resets the machine to power-on, not to the saved instant. The chips already have save/load (`atari-antic`/`atari-gtia`/`atari-pokey` all carry state serialisation). Wire a real machine-state snapshot. Flagged "deferred" at `docs/status/outstanding-work.md:325`. |
| **Second fire button + up to 4 controllers** | **M** | Only one fire button on one controller is wired: `set_fire` → GTIA TRIG0 (`lib.rs:324-326`). GTIA exposes TRIG0-3 (`atari-gtia` `lib.rs:123,287`). The 5200 controller has **two** fire buttons (top + bottom) and the console supports **up to four** controllers. Wire the second button and ports 2-4 (each controller = one pot pair + triggers + a keypad). RUNTIME-VERIFY which line the second button reads (TRIG vs a POKEY-scanned path) against the reference before wiring. |
| **PAL palette + `$D014` correctness** | **S–M** | The 5200 region enum, profiles, and frame timing are PAL-aware (`lib.rs:66-93`, `profiles.rs`), but GTIA carries an **NTSC-only palette** and `$D014` returns `0x00` — which per the reference signals *PAL* — regardless of region (shared GTIA defect). On a PAL 5200 there is no PAL palette to use and `$D014` is wrong on NTSC too. The fix is mostly chip-level, but the 5200 PAL profile needs the region threaded to the GTIA palette once it exists. |
| **Bundled-BIOS / firmware story** | **S** | The profile declares the 2 KB BIOS **optional** (`profiles.rs:63-67`) and cart-only boot falls through to the cart's reset-vector mirror (`lib.rs:267-276`). No BIOS is bundled. Decide whether to ship/locate a BIOS so titles needing the BIOS handoff (the `JMP ($BFFE)` path) work out of the box, mirroring the firmware-locate story used elsewhere. |

## Done as part of this plan (free, ~half a day)

Doc-drift correction in `docs/status/outstanding-work.md`: the machine test count
is **15**, not 14 (`:254`); ANTIC is **19/19** and GTIA **13/13**, not 14/14 and
9/9 (`:247-248`). No code claim in the survey contradicted the system's own source
comments — the machine layer's doc-comments (memory map, clock model, two-chip
decode) all match the code. The only drift found was stale test counts.

## Recommended sequence (highest leverage first)

1. **`.a52`/`.car` header stripping** (S) — cheapest unblock; lets headered dumps
   load and feeds the cart-type byte into the mapper.
2. **Bank-switched cartridges** (M) — the one Tier-A gap that stops real titles
   (Bounty Bob, AtariMax) running at all. Highest game-impact per week.
3. **Doc-drift fix** (S) — correct the test counts while the survey is fresh.
4. **ANTIC HSCROL/VSCROL** (chip-tracked) — the biggest *visual* win; lands on the
   800XL at the same time. Pull from the chip plan.
5. **POKEY distortion table + 16-bit period** (chip-tracked) — the audible audio
   wins; also shared with 800XL.
6. **Live-state snapshot** (M) — rewind/save-state actually works; the chips
   already serialise, so this is wiring.
7. **Second fire button + multi-controller** (M) — completes the peripheral story
   (verify the second-button line first).
8. **PAL palette + `$D014`** (S–M, chip-led) and **machine-level DMA interleave**
   (M) — completionist depth once the chip fixes land.
9. **Bundled-BIOS story** (S) — out-of-box boot for BIOS-dependent titles.

## Key files

- Machine wiring: `crates/machine-atari-5200/src/lib.rs` (memory map `:260-293`,
  scan-line/DMA loop `:182-258`, NMI from VBI/DLI `:256-257`, single-fire wiring
  `:324-326`, single-POKEY).
- Cartridge: `crates/machine-atari-5200/src/cartridge.rs` (sizing `:27-33`,
  two-chip 16 KB decode `:45-53`; **no bank switching** `:3`).
- Runtime: `crates/runtime-atari-5200/src/{runtime.rs,input.rs,profiles.rs,snapshot.rs}`
  (cart bytes raw, no header strip `runtime.rs:50,68`; analogue/keypad/fire input
  `input.rs`; PAL/NTSC profiles `profiles.rs:9-42`; **snapshot loses live state**
  `snapshot.rs`).
- Integration test (ignored, needs cart+BIOS): `crates/machine-atari-5200/tests/cart_boot.rs`.
- CPU (at ceiling, no work): `crates/mos-6502/src/{lib,cycle,tick}.rs`.
- Shared chips (defects tracked at chip level, affect 800XL too):
  `crates/atari-antic/src/lib.rs` (HSCROL/VSCROL, mode-3, NMIST, DMA model),
  `crates/atari-gtia/src/{lib.rs,palette.rs}` (PRIOR schemes, `$D014`, GR.10
  backdrop, NTSC-only palette), `crates/atari-pokey/src/lib.rs` (distortion table
  `:976-994`, 16-bit period `:821-866`, pot scanner `:381-388`).
- Status docs: `docs/status/outstanding-work.md:243-329` (stale test counts),
  `docs/status/current-system-usability.md:70`, `docs/status/drivability-assessment.md:276`.
- Reference: `reference/by-system/atari-8bit/{atari-8bit-reference.md,mappingtheatari.md}`.

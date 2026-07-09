> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Jupiter Ace to 100% — media loading, snapshot fidelity, beeper/tape, display polish"
type: plan
date: 2026-06-09
system: docs/systems/jupiter/jupiter-ace.md
basis: code-grounded survey of machine-jupiter-ace / runtime-jupiter-ace / emu198x-jupiter-ace with a live test run (22 unit tests green, ROM-boot test ignored pending ROM), cross-checked against the shared zilog-z80 finding, 2026-06-09
---

# Jupiter Ace — road to 100%

What it would take to bring the Jupiter Ace to feature- and accuracy-complete,
grounded in a code-level read of the three crates that make it
(`machine-jupiter-ace`, `runtime-jupiter-ace`, `emu198x-jupiter-ace`) and a live
`cargo test` run, not doc prose. Where the status doc and the code disagree, the
code wins and the disagreement is called out.

## Executive summary

**The Jupiter Ace is a finished *core* with no *content path*.** It is the
opposite shape from a C64: there is no hard accuracy long-pole here — the Ace has
no custom video IC (the Z80 paints a 32×24 character display straight out of RAM),
its CPU is the fleet's at-ceiling `zilog-z80`, and the memory map, mirror decode,
keyboard matrix and 50 Hz interrupt all match the hardware and pass directed tests
(`machine-jupiter-ace`: 19 unit tests + display/keyboard sub-tests, all green;
`runtime-jupiter-ace`: 3 green). It **boots its Forth ROM to the cursor and is
interactive** (`current-system-usability.md:81` — types and executes Forth lines).

The long pole is **breadth and persistence, not depth**: there is **no way to load
software** (no `.tap` / `.ace` parser, and `load_media` rejects every image —
`runtime.rs:164-170`), and the snapshot saves only the ROM and a clock value, not
RAM/CPU/display state (`snapshot.rs:10-43`), so save/restore silently drops the
machine's entire working state. Those two — get programs in, and make state
round-trip — are the whole game for "100%".

The CPU needs no work: the Ace depends on `zilog-z80`, which the shared finding
re-verified today at the accuracy ceiling (Tom Harte 1,604,000/1,604,000; FUSE
1351/1356 exact). The Ace conventionally runs IM 1
(`lib.rs:174-179` hard-acks IM1-style and floats 0xFF), so the one latent Z80 gap
(real IM0 acknowledge) does not touch this system.

**Two doc-vs-code contradictions found and recorded** (see "Done as part of this
plan"): the status doc claims audio is "unwired in the binary" — it is wired
(`script.rs:78-79`, `:192-196` write a WAV via `--audio-capture`); and it
describes the frame interrupt as "pulsed for the first 32 T-states" — the code
actually holds INT until the CPU acknowledges it (`display.rs:84-92`, `:188-197`),
a deliberate fix so the 50 Hz tick can't be missed.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | `.ace` snapshot load + `.tap` tape load via `load_media`; full runtime snapshot (RAM/CPU/display); doc-drift fixes | **~1.5–2.5 weeks** |
| B — System accuracy finish | tape SAVE (MIC) + load timing on port $FE; beeper/tape bit-3 modelling; 16/48 KB expansion memory-map verification; scanline-aware display option | **~1.5–2 weeks** |
| C — Audio + display fidelity | beeper pulse-width / band-limited downsampler review; warm-CRT colour + border provenance; cursor/flash semantics confirmation | **~3–5 days** |
| D — Preservation breadth | `.z80`-style / alternate Ace snapshot variants, expansion ROM / forth-toolkit carts, Deep Thought / Boldfield RAM-pack variants, real ROM-boot CI gate | **~1–2 weeks** |

**True 100% of everything ≈ 4.5–7 weeks**, front-loaded: Tier A (the content path
+ snapshot fidelity) is most of the user-visible value and the cheapest. Unlike
the C64 there is no multi-week core rewrite anywhere in this plan.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (the content path; do first)

| Item | Effort | Notes |
|------|--------|-------|
| **`.ace` snapshot load** | **M** | The single highest-leverage gap: with no tape and no `.ace` loader there is *no way to run any software but the ROM*. The `.ace` format is a flat RAM dump from `$2000` (status doc `outstanding-work.md:525-526` — "donor handled this; not yet ported (RAM dump at `$2000`)"). Parse it, restore video/char/user RAM and the few system variables, and resume. New `format-jupiter-ace-ace` crate + a `MediaKind` wiring in `load_media` (currently `runtime.rs:164-170` returns `UnknownMediaSlot` for *every* image, and `profiles.rs:85` declares `media_slots: Vec::new()`). |
| **`.tap` tape load** | **M–L** | The Ace's native tape format (distinct from the Spectrum `.tap`; no `format-jupiter-ace-*` crate exists — only the Spectrum/C64 tap crates are in the workspace). Model the byte-level load through the ROM's tape routines, or a fast-load shortcut that injects the block. Requires the tape-input read path on port $FE (see Tier B) if done at signal level. The bulk of the surviving Ace library is on tape. |
| **Full runtime snapshot (RAM + CPU + display)** | **M** | `snapshot.rs` serialises only `version`, `time`, `model_id`, `bios_bytes` — **not** RAM, char RAM, video RAM, CPU registers, keyboard state, or display phase. Restore therefore rebuilds a *cold* machine and discards all running state (`runtime.rs:113-136` `rebuild_after_restore` just re-news the machine). This matches the ZX81 sibling's minimal pattern (`runtime-sinclair-zx81/src/snapshot.rs` is equally thin), so it is a **shared family shortcut**, not an Ace-only oversight — but it means "save state" does nothing useful. Add accessors on `JupiterAce` for the RAM banks + a `Z80` register snapshot and serialise them. |
| **Media-slot declaration + CLI mount** | **S** | `profiles.rs` declares no media slots and `script.rs` has no `--tape`/`--snapshot` flag; once the parsers exist, declare the slot(s) and add the load flag so the content path is reachable from the headless binary and MCP. |

## Tier B — System accuracy finish

| Item | Effort | Notes |
|------|--------|-------|
| **Tape I/O on port $FE (MIC bit 3 + EAR bit 5)** | **M** | `io_write` (`lib.rs:226-230`) models *only* the beeper (bit 4); `io_read` (`lib.rs:218-224`) returns keyboard bits with bits 5-7 forced high (`keyboard.rs:65`), so there is **no tape-input (EAR) path at all**. Real Ace tape SAVE toggles MIC (bit 3) and LOAD samples EAR (bit 5) on the same port. Needed for signal-level `.tap` load and any tape SAVE. |
| **Tape SAVE (write-back)** | **M** | Once MIC is modelled, capture the saved bitstream to a `.tap` writer + writable mount/flush — the Ace counterpart to the C64/Spectrum tape-save decision. Lower priority than load. |
| **16/48 KB expansion memory-map verification** | **S–M** | The expansion path (`lib.rs:195-199`, `:209-214`) maps `$4000+` linearly into the RAM `Vec` after the 1 KB base. The stock Ace's real expansion (16 KB pack at `$2000`-ish region behaviour, and the 32 KB top pack) has specific decode quirks the linear model may not capture; **needs-runtime-verification** against MAME `cantab/jupace.cpp` (the cited map source) for the Ace16k/Ace48k models. The 3 KB stock map is verified by tests; the expanded maps are not exercised by any test. |
| **Scanline-aware display option** | **S–M** | `render_frame` paints the whole screen once per frame from a RAM snapshot (`display.rs:135-163`) — the doc itself calls this "simpler than scanline-accurate" and notes the Ace "has no mid-frame effects." True for stock software; a from-RAM mid-frame rewrite (a Forth program racing the beam) would not render. Low risk, listed for completeness; keep the cheap path unless a real title needs it. |

## Tier C — Audio + display fidelity

| Item | Effort | Notes |
|------|--------|-------|
| **Beeper downsampler review** | **S–M** | Audio is a 1-bit speaker state sampled into a 48 kHz buffer by a fixed-rate accumulator (`lib.rs:128-139`), reading `display.speaker_state` (a plain bool toggled by bit 4). This is point-sampled, not band-limited — high-frequency beeper tones will alias. Compare against the Spectrum beeper path; add band-limiting if the Spectrum has it and the Ace lacks it. |
| **CRT colour + border provenance** | **S** | The display hard-codes a "slightly warm white" `0xFFCFCFCF` (`display.rs:70`) and borrows the ZX81's 32/24 px border envelope (`display.rs:32-39`) explicitly "to match the period look" — both are *aesthetic choices*, not measured. Cite a reference photo / service-manual source, or accept them as deliberate stylisation and record that. |
| **Cursor / mode-indicator semantics** | **S** | `current-system-usability.md:81` records the steady inverse-block cursor as correct (a mode indicator like the ZX81 family, *not* a flashing cursor). Confirm against the ROM behaviour and lock it down so a future "make the cursor flash" change is recognised as a regression. **needs-runtime-verification** against real hardware / MAME. |

## Tier D — Preservation breadth (the long tail)

| Item | Effort | Notes |
|------|--------|-------|
| **Alternate Ace snapshot / tape variants** | **M** | Beyond the canonical `.ace` + `.tap`: any emulator-specific snapshot variants and TZX-style timed tape images for protected/fast-load tapes. Preservation, not "runs the common library." |
| **Expansion ROM / Forth-toolkit carts** | **M** | The Ace's expansion-port ROM cartridges (e.g. Forth extension ROMs). No cartridge `MediaKind` or expansion-ROM mapping exists today. Niche. |
| **RAM-pack / third-party expansion variants** | **S–M** | Deep Thought / Boldfield and other RAM-pack expansions beyond the clean 16/48 KB models — decode edge cases for completeness. |
| **Real ROM-boot CI gate** | **S** | `tests/rom_boot.rs:22` is `#[ignore]` pending an 8 KB ROM at `~/.emu198x/roms/jupiter-ace/ace.rom`. The test is well-built (asserts font copy reached `$2800` + a single cursor block) but never runs in CI. Wire the ROM into the CI asset path and un-ignore, so boot is a tracked gate, not a manual check. **needs-runtime-verification** that it passes today (the doc claims a live boot 2026-06-04, but the gate is not automated). |

## Done as part of this plan (free, ~half a day)

Two doc-drift fixes in the system / status docs, both code-confirmed today:

- **Audio is wired, not "unwired in the binary."** `outstanding-work.md:521-523`
  lists "Audio output unwired in the binary (mono beeper buffer is taken … but no
  WAV is written)." The binary *does* write a WAV: `script.rs:78-79` parses
  `--audio-capture` and `:192-196` calls `save_audio_capture`. The buffer is also
  pushed to the runtime audio sink every frame (`runtime.rs:206-211`). Correct the
  doc.
- **The frame interrupt is held until ack, not "pulsed for 32 T-states."**
  `outstanding-work.md:506-507` says "INT pulsed at the top of each frame for the
  first 32 T-states." The code asserts INT at frame top and **holds** it until the
  CPU acknowledges (`display.rs:84-92` `int_pending`, `:188-197`
  `interrupt_active`/`ack_interrupt`, released in `lib.rs:174-179` on `IntAck`) —
  a deliberate fix (the in-code comment at `display.rs:84-87` explains the 32-T
  window was missable and left the Forth ROM spinning). Correct the doc.

Test count is accurate to state today: 22 Jupiter-Ace tests green (19 machine +
3 runtime), 1 ignored (ROM boot, pending ROM asset).

## Recommended sequence (highest leverage first)

1. **`.ace` snapshot load** (M) — the one gap that turns "boots the ROM" into
   "runs software." Highest leverage per day; the donor already proved the format.
2. **Full runtime snapshot** (M) — make save-state actually preserve the machine;
   also the substrate the `.ace` path can reuse for RAM restore.
3. **Media-slot declaration + CLI mount** (S) — make the new loaders reachable
   from the headless binary and MCP.
4. **Doc-drift fixes** (S) — audio-wired + held-INT corrections, while the code is
   fresh in mind.
5. **`.tap` tape load** (M–L) + **tape I/O on port $FE** (M) — the bulk of the
   surviving Ace library; load first, the EAR/MIC path underneath it.
6. **16/48 KB expansion map verification** (S–M) — confirm the expanded models
   against MAME before declaring them done.
7. **Beeper downsampler + CRT/border provenance** (S–M + S) — audible/visible
   fidelity polish.
8. **Tape SAVE** (M), then **the Tier-D preservation tail** (carts, RAM-pack
   variants, ROM-boot CI gate) — completionist.

## Key files

- Machine core: `crates/machine-jupiter-ace/src/lib.rs` (memory map `:184-216`,
  I/O / beeper `:218-230`, bus + IntAck `:141-182`, half-cycle 2×/T-state drive
  `:111-126`).
- Display: `crates/machine-jupiter-ace/src/display.rs` (held-INT `:84-92`,
  `:188-197`; whole-frame render `:135-163`; colour/border `:32-71`).
- Keyboard / input: `crates/machine-jupiter-ace/src/keyboard.rs`,
  `crates/machine-jupiter-ace/src/input.rs`,
  `crates/runtime-jupiter-ace/src/input.rs`.
- Runtime: `crates/runtime-jupiter-ace/src/runtime.rs` (`load_media` rejects all
  media `:164-170`; rebuild-on-restore `:113-136`),
  `crates/runtime-jupiter-ace/src/snapshot.rs` (thin envelope `:10-43`),
  `crates/runtime-jupiter-ace/src/profiles.rs` (no media slots `:85`; models
  `:8-55`).
- Binary: `crates/emu198x-jupiter-ace/src/script.rs` (audio capture wired
  `:78-79`, `:192-196`; no tape/snapshot flag).
- Tests: `crates/machine-jupiter-ace/tests/rom_boot.rs` (`#[ignore]`, needs ROM).
- CPU (at ceiling, no work): `crates/zilog-z80` (shared finding — Tom Harte 100%,
  FUSE 1351/1356; Ace uses IM1 so the IM0 gap is latent).
- Reference: MAME `cantab/jupace.cpp` (the cited memory-map source);
  `emulators/zx-spectrum/.../jupiter.rom` (the in-tree 8 KB Forth ROM, md5
  `db6efdfd82cebdfbb493d85b1a5efc3c`); `reference/by-system/jupiter-ace/`
  (magazines only — no datasheet extract present).

# No ROM trap-load

**Status:** Rejected 2026-05-12. Tape and disk loading stays cycle-accurate at all times. No `LD-BYTES` trap, no `LOAD` shortcut, no synthetic register injection. The cost of a slow load is the cost we pay.

## Context

The Spectrum catalogue runs ~50 entries through the headless harness on every test pass. Most entries are tape-based and spend several thousand frames reproducing the ROM's `LD-BYTES` byte-loop: pilot detect, sync detect, edge sampling, parity check, repeat. At cycle accuracy this is millions of T-states of work per block, and the catalogue's wall-clock cost is dominated by tape loads.

The temptation is a ROM trap: when the Z80 fetches an instruction at `$0556` (48 BASIC `LD-BYTES` entry) with the right surrounding bytes and a playing tape, skip the routine entirely. Read the caller's `A`/`IX`/`DE`, write the next block's payload straight to memory, synthesise the documented exit register state (`A = parity`, `IX += DE`, `DE = 0`, `F.C = 1`), pop the return address, jump to it. One trap call replaces hundreds of thousands of T-states with a few hundred.

## Why this was tempting

- 5–10× wall-clock speedup on tape-heavy catalogue passes.
- The post-trap CPU state is bit-identical to what real `LD-BYTES` produces *at the return point*.
- ROM-trap is a well-trodden technique — FUSE, Spectaculator, ZXSpin and others all ship one.
- Standard ROM-speed loads are by definition deterministic, so the trap can never disagree with cycle-accurate execution on whether the load *succeeds* — only on the T-state count.

## Why we rejected it

The post-trap *register* state matches. The post-trap *system* state does not, and that's what matters.

A real `LD-BYTES` call takes ~6 000 frames (~2 minutes of simulated wall time) for a typical 48K game body block. During those frames:

- The ULA renders 6 000 border-stripe frames; the loader's `OUT (#FE)` pattern paints the iconic loading stripes into the framebuffer.
- The AY/beeper produces ~2 minutes of audio.
- Interrupt counters (`FRAMES` system variable, etc.) advance proportionally.
- Any peripheral driven by master-clock time (Kempston, AY tone counters, the FDC's seek timer on +3) ticks forward.

The trap collapses this to ~0 frames. Skip-ahead state isn't bit-identical to play-through state — by `wait_frames = 60` post-load the game has been running for ~6 000 frames longer than the catalogue's stored `frame_hash` was captured against. Every hash drifts. The catalogue's accuracy assertion stops asserting accuracy.

This isn't a hash-update problem. It's a category error: the trap *is* a stub for the ROM's `LD-BYTES` routine. It does in software what RULES.md rule 20 forbids ("No stub implementations. Every chip does what the silicon does") and rule 21 forbids ("Accuracy is foundational, not retrofitted").

The Z80 is a chip. The 48 BASIC ROM is a sequence of bytes the chip executes. A trap that intercepts execution and produces output the chip would have produced *eventually* is exactly the kind of stub the rule names. The fact that the trap is correct in steady state is the same kind of correctness a `mock_database_call()` is — fine for a unit test, lethal for the thing the project is built to verify.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Adding a `tape_trap_enabled` toggle, a `ROM_LD_BYTES_ENTRY` constant, a `try_tape_trap()` method, or anything else that compares `z80.regs.pc` against a known ROM address to short-circuit execution.
- Adding parallel views of tape media (e.g. a `blocks: Vec<TapeBlock>` alongside the `spans: Vec<TapeSpan>` pulse stream) — the only reason to keep blocks around after `load_blocks` is so a trap can synthesise them later.
- Writing "but it's accuracy-preserving because the post-state matches" — the post-*register* state matching is not the same as post-*system* state matching, and we judge accuracy on the latter.
- Doing the same for `+3DOS` disk loading, `LD A,(DE) ; CP "M" ; JR …` Speedlock detection, NES Famicom BASIC `LOAD"`, C64 `JSR $F539` (KERNAL LOAD), Amiga `trackdisk.device` request interception, or any other "skip the ROM/firmware/library routine and synthesise its output" pattern. Same rule, every system.
- Justifying a trap by "this is what FUSE / Spectaculator / WinUAE does." Other emulators ship traps because they target playability, not preservation. Our bar is different.

## What we do instead when loads feel slow

Two legitimate paths:

1. **Unbounded headless frame rate.** Real cycle-accurate emulation, but with no wall-clock pacing between frames. The host CPU runs the simulation flat-out; a 2-minute simulated tape load completes in whatever wall time the host can sustain (typically 5–15 seconds on a modern Mac, release build). This is the original ask in the conversation that produced this decision; it preserves accuracy completely and is the right answer.
2. **Live with it.** For the catalogue's CI value, slow loads are fine — they happen once per entry per pass, on machines that aren't waiting on a human. If a particular entry is so slow it bottlenecks development iteration, prefer raising frame_budget or skipping the entry over violating rule 20.

## What we removed

The aborted attempt added, then this decision removed:

- `TapePlayer::blocks`, `block_span_starts`, `next_trap_block_idx` fields and the `trap_consume_block` / `current_trap_block` / `trap_blocks_count` accessors on `crates/common-sinclair-zx-spectrum/src/tape.rs`.
- `tape_trap_enabled`, `ROM_LD_BYTES_ENTRY`, and `try_tape_trap()` on `crates/common-sinclair-zx-spectrum-48k-class/src/core.rs`, plus the call site in `tick_cpu_and_bus`.
- `crates/emu198x-catalogue/tests/tape_trap_bench.rs`.

The RULES.md addition of rule 30 (brainstorm before implementation) was kept — the lesson from this aborted attempt is that we skipped the brainstorm and went straight to code, and the cost of unwinding the wrong design was higher than the cost of pausing to think would have been.

## Related rules

- RULES.md rule 20 — "No stub implementations. Every chip does what the silicon does."
- RULES.md rule 21 — "Accuracy is foundational, not retrofitted."
- RULES.md rule 30 — "Brainstorm before implementation."
- `feedback_no_simplifications.md` — always implement accurate hardware-level emulation, never stub.
- `feedback_cycle_accurate_from_start.md` — cycle accuracy must be foundational, not retrofitted.

# Decision: NES CPU cycle multi-phase model

**Date:** 2026-05-30

## The decision

**The NES `Nes::tick()` will split each CPU cycle into two sub-phases — start-phase and end-phase — with the PPU advancing inside each phase. The CPU's bus op happens between the two phases. The CPU's NMI/IRQ pin sample reads PPU state as captured at the end-phase advance, which runs to one master clock *behind* the CPU's own master-clock counter.**

This refines [nes-clock-topology.md](nes-clock-topology.md). The "PPU ticks every master clock division, CPU ticks every 3rd" topology still holds at the level of *what advances when*. What changes is *how the CPU cycle itself decomposes* so the PPU's externally-observable signals (NMI line; `$2000`–`$2007` register state) line up with where the CPU samples them, instead of running concurrent with the CPU's bus op.

## Why this is needed

blargg `ppu_vbl_nmi/05-nmi_timing`, `06-suppression`, `07-nmi_on_timing`, `08-nmi_off_timing` and `cpu_interrupts_v2/2-nmi_and_brk`, `/3-nmi_and_irq` all report exactly one PPU clock of skew between our NMI line transitions and the reference timing. Every other source of timing error has been ruled out (CPU edge detection, branch quirks, NMI-hijack mechanism — all correct against Tom Harte / Mesen). Three independent single-axis attempts each failed in a self-cancelling pattern:

| Attempt | Helps | Breaks |
|---|---|---|
| Move PPU NMI assertion from dot 3 to dot 1 | 05 (partial) | 06, 07, 08 |
| Make `$2000` write immediate (drop `pending_nmi_output`) | structurally aligned with Mesen | no test delta |
| Sample `cpu.nmi` after bus op (let `$2002` read clear NMI first) | 06 V- suppression rows | left 1-clock drift |

The combination of all three still leaves a 1-PPU-clock drift in the same direction across all four blargg tests, which is the signature of a master-tick alignment problem rather than a logic bug. Mesen models this with `_ppuOffset = 1` — its `Ppu::Run(masterClock - 1)` calls mean the PPU's externally-observable state at any given master clock represents the world one tick ago, giving the CPU's `$2002` read a chance to land in the PPU's logical "before NMI assertion" window even when the assertion master tick has technically passed.

Recreating that lag inside our `body-then-increment` PPU shape isn't a one-line change — every `dot==N` check in the PPU (sprite eval, BG fetch, copy_horizontal/vertical, dot-339 odd-frame skip, etc.) is written against the current convention, and shifting the dot increment to before the body would shift all of them by one tick. The clean fix is to introduce the phase split at the CPU cycle level instead, which is the abstraction Mesen uses.

## Why not extract a shared multi-machine abstraction now

Three working data points, three different shapes:

| Machine | CPU:peripheral ratio | Bus model |
|---|---|---|
| C64 (`machine-commodore-c64`) | 1:1 (PHI1/PHI2) | shared, arbitrated via `RDY`/`BA` |
| NES (target) | 1:3 | separate (PRG + CHR), no arbitration; phase-lag for register I/O |
| Atari 800XL (`Emu198x-Oldest` donor) | 1:2 colour clocks | shared, ANTIC steals via `dma_budget` + `wsync_halt`; ANTIC currently processes whole scan lines per line boundary, not per-dot |

A wider survey of 6502-family machines turns up at least seven distinct shape families — three is the floor, not the ceiling.

| Shape family | Mechanism | Examples |
|---|---|---|
| Concurrent tick with stall signal | Chip raises a stall flag, CPU honours it for a cycle or more | C64 (`RDY`), Atari 800XL (`dma_budget`), Atari 2600 (TIA `wsync_halt`) |
| Block halt | Chip halts CPU for a contiguous run of cycles | Atari 7800 (MARIA display kernel), Acorn Electron (ULA halts MODE 0-3 scanlines) |
| Phase-lagged register I/O | PPU state is sampled one master tick behind so register reads land in the right phase | NES (Mesen `_ppuOffset`) — this entry's target |
| Dynamic CPU frequency | CPU clock divider changes based on address or mode | BBC Micro (1 MHz peripheral bus at `$FCxx-$FExx`), C128 (1/2 MHz mode), HuC6280 (overclock instruction) |
| Cycle-stretching on access | Per-access penalty added to CPU cycles | Oric (DRAM refresh), BBC Micro (alternate framing of 1 MHz bus) |
| Half-cycle bus sharing | Sub-CPU-cycle clock granularity required for cycle-accurate emulation | Apple II — same precedent as [half-cycle-signals.md](half-cycle-signals.md) on Spectrum |
| Beam-racing only | No video-side interrupt — CPU manually writes registers during each scanline | Atari 2600 (TIA) — overlaps Concurrent stall, but the *programming model* is fundamentally different |

Each chip family forces a different dance. Extracting a single `tick_cpu_cycle` trait at N=2 — or even N=3 — would either lock in a NES-shaped abstraction the other machines can't fit, or a meta-abstraction that's so general it stops carrying meaning. With seven shape families visible, the question shifts from *"what's our shared abstraction"* to *"which pairs of machines actually share a shape"* — and the answer right now is essentially none. C64 and Atari 800XL both have "concurrent tick with stall signal" but use different mechanisms (RDY/BA vs dma_budget) and different ratios; refactoring one to be expressible in the other's terms costs more than it buys. The right move is to do the NES refactor with C64-style structural alignment as a guidepost (peripherals advance, then CPU bus op, then sample pins, then CPU tick — but split into two passes per CPU cycle), and revisit shared abstraction only when two different machines genuinely need the same shape, not when their shapes can be made to *look* alike through a thick enough trait.

## Implications

- `Nes::tick()` is no longer "advance one master clock division." It becomes "advance one CPU sub-phase." The master clock counter advances at finer resolution (4× current — Mesen scales the NES master clock to give 12 ticks per CPU cycle, split 6+6).
- The PPU exposes a `run(target_master_clock)` method that advances to (or past) a given master-clock target, rather than a `tick()` that advances exactly one dot. The internal dot loop stays — only the entry shape changes.
- The PPU's `_ppuOffset = 1` equivalent (Mesen-style "PPU runs to master-1") is the mechanism by which the CPU's `$2002` register read lands in the PPU's pre-NMI window even when wall-clock has passed dot 1.
- `master_clock()` reported by the machine to the runtime / MCP / script layer counts at the new finer resolution. Wherever a script step expects "N master ticks = N PPU dots," that contract changes. The `run_ticks` MCP step (added earlier this session) is the most visible affected surface.
- The `ppu_onscreen` and `nes_sweep` harnesses count `nes.master_clock()` — the absolute values will shift but the relative completion behaviour holds.
- The C64 machine's `tick()` shape does NOT change. It's already "one PHI2 cycle = one peripheral advance + one CPU cycle." The deeper alignment between C64 and NES is that *both will be conceptually one-CPU-cycle-per-tick*; the C64 just happens to need only one sub-phase because of its 1:1 ratio, while the NES needs two because of its 1:3 ratio and the phase-lagged register I/O.
- The Atari 800XL port (when it lands) gets a third opportunity to compare; expect ANTIC to need finer-grained ticking than the donor's per-scan-line shape if cycle-accurate timing matters there.

## Drift triggers

Phrases that should make you re-read this entry.

- *"Just move NMI from dot 3 to dot 1 — that's how Mesen does it"* — single-axis change. Will break 06/07/08. The fix is structural, not a check-line move.
- *"Shift the PPU's dot increment before the body to match Mesen's `_cycle++` first"* — touches every `dot==N` check in the PPU. The blast radius is sprite eval, BG fetch, the copy_v / copy_h timing windows, the dot-339 odd-frame skip — all of them. Phase split at the CPU cycle level avoids the blast.
- *"Let's extract a `Tickable` trait so C64 / NES / Atari all share the loop"* — three machines, three shapes (see table above). Wait for N≥4 with measurable shared structure.
- *"Sub-CPU-cycle master-clock granularity is overkill for an emulator"* — six failing blargg tests across two test ROMs disagree. The granularity is exactly what's needed for register-I/O phase alignment.
- *"`run_ticks` semantics shouldn't change just because of a CPU-side refactor"* — they have to, because the master clock IS what `run_ticks` advances. Recalibrate the harnesses that depend on absolute master-clock values; the relative behaviour is what matters for grading.
- *"The C64 already does multi-phase via `RDY`/`BA`, so just port that pattern"* — the C64's pattern solves bus arbitration. The NES needs phase-lag for register I/O sampling. Same shape conceptually (split clock), different physical problem.

## Implementation order (when this work lands)

1. Bump the master clock counter to Mesen-scaled granularity (12 per CPU cycle for NES NTSC). All existing dot-driven code keeps working if dot advancement still happens on the 4-tick boundary.
2. Add `Ppu::run(target_master_clock)` as an alternative entry point; the existing `tick()` becomes a thin wrapper that calls `run(self.master_clock + 4)`.
3. Refactor `Nes::tick()` to use start-phase / end-phase explicitly. Within each CPU cycle: start-phase advances PPU to `cpu_master - 1`, bus op, end-phase advances PPU to `cpu_master + endCount - 1`, sample pins, CPU tick.
4. Verify Tom Harte single-step (~2.56M) + Dormann + nestest stay green at every step. blargg PPU 05 + 06 + 07 + 08 should flip in the same pass (since they share the root cause).
5. Recalibrate `ppu_onscreen` and `nes_sweep` harnesses — their `MAX_TICKS` constants need scaling.
6. cpu_interrupts_v2 tests 2 and 3 should flip alongside, since their off-by-one was the same root cause.

## Related

- [nes-clock-topology.md](nes-clock-topology.md) — original "master clock drives the loop" decision. This entry refines, doesn't replace.
- [half-cycle-signals.md](half-cycle-signals.md) — Spectrum precedent for sub-CPU-cycle granularity (half-cycle for Z80 + ULA). Same shape of decision; the NES refactor is the 6502 equivalent.
- `emulators/nes/Mesen2/Core/NES/NesCpu.cpp` — the reference implementation. `StartCpuCycle` / `EndCpuCycle` plus `_ppuOffset` is the canonical pattern.
- Task #35 (Emu198x project tasks) — the failing tests this refactor unblocks.

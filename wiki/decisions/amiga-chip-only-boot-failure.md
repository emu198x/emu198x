# Issue: Kickstart 1.3 won't boot on chip-RAM-only A500

**Date:** 2026-04-19
**Status:** Open — investigate

## Symptom

`runtime-commodore-amiga` constructs every Amiga with **512 KiB of trapdoor slow RAM at $C00000** (`runtime.rs:535` — `Amiga::new_with_slow_ram(kickstart, 512 * 1024)`).

The comment justifies it as "KS 1.2+ A500/A2000 boot paths depend on 512 KiB trapdoor slow RAM at $C00000 so ExecBase can live there instead of consuming scarce chip RAM." A bare `Amiga::new(kickstart)` call (chip RAM only, no slow RAM) **does not reach the insert-disk screen** — DMACON / BPLCON0 fail to land in the configuration the Kickstart 1.3 boot path normally produces, and the screen does not compose correctly.

## Why this matters

A real Commodore A500 shipped with **512 KiB of chip RAM and no expansion**. Kickstart 1.2 and 1.3 are designed to boot on that hardware. Both WinUAE and FS-UAE boot Kickstart 1.3 to insert-disk on a 512K-chip-only configuration without complaint.

Our hardcoded slow RAM workaround is **masking a real bug** in the boot path. As long as the workaround is in place we can't tell whether subsequent fixes (e.g. the 2026-04-19 chip-bus-arbitration fix) are correct on the canonical 512K-chip configuration, only on the expanded one.

The runtime workaround was likely added when a previous version of the chip-bus arbitration was producing different symptoms. After the arbitration fix, the configuration may now be incorrect for entirely different reasons — that's exactly the kind of layered confusion the workaround creates.

## What to investigate

1. **Reproduce on bare `Amiga::new(kickstart)`** — confirm DMACON / BPLCON0 / display state at frame 250 with chip RAM only, vs the expanded configuration.
2. **Compare `MemHeader` allocation under the two configurations** — Exec scans the memory list during init and decides where to place ExecBase. With slow RAM present, it goes to slow RAM; without, it must go to chip RAM. Check whether the chip-RAM-only path is failing because:
   - Exec doesn't see chip RAM at all (memory map decode wrong?)
   - Exec rejects chip RAM (size check, signature check?)
   - ExecBase is placed but the boot routine then fails for a different reason (memory pointer overlap with screen RAM?)
3. **vAmiga or WinUAE comparison** — boot KS 1.3 on a 512K-chip-only config in vAmiga with cycle logging on; sample the same state at the same waypoint as our test; diff. The first divergence is the bug.
4. **Likely culprits** to check first:
   - `Amiga::new()` without slow RAM may be leaving the slow-RAM region marked as something other than "unmapped" (autoconfig probe could see it as present-but-zero-sized?)
   - Memory size detected by Exec may be wrong if the Gary decode misclassifies an absent slow RAM region
   - The reset overlay / boot ROM mapping may differ between the two configs in a subtle way

## Acceptance

- `Amiga::new(kickstart)` (no slow RAM) boots Kickstart 1.3 to the insert-disk screen within 250 frames.
- All 6 invariant tests in `kickstart_boot_invariants.rs` pass when `boot_kickstart13()` uses `Amiga::new()` instead of `new_with_slow_ram(kickstart, 512 * 1024)`.
- The hardcoded slow RAM in `runtime.rs:535` is reduced to 0 by default (with the option to add it back per-model when emulating a slow-RAM-expanded A500).

## Findings (2026-04-19, fifth pass — full causal chain mapped)

### The complete picture

GfxBase state at frame 250 (`tests/gfxbase_state.rs`):

| Field | Chip-only | Slow-RAM |
|---|---|---|
| GfxBase->ActiView | `$000049A6` | `$00005A10` |
| ActiView->ViewPort | **`$00000000` NULL** | `$000059E8` |
| ActiView->LOFCprList | **`$00000000` NULL** | `$00C01808` |
| ActiView->DyOffset | `$002C` | `$002C` (same) |
| ActiView->DxOffset | `$0081` | `$0081` (same) |
| GfxBase->LOFlist | `$00000676` (= ExecBase!) | `$0000B888` (real list) |
| GfxBase->copinit | `$00002368` (boot copper) | `$00000420` (boot copper) |

**Chip-only's View struct exists with valid offsets, but its ViewPort
and LOFCprList pointers are NULL.** That makes MrgCop's early exit
trigger at `$FCD4E6` (`BEQ.W $FCD55E if View->ViewPort == NULL`).
MrgCop never allocates the merged copper list, so `LOFCprList->start`
is never updated, so `GfxBase->LOFlist` stays at the placeholder
value (= ExecBase).

### The toxicity of ExecBase-as-COP2LC

The VBL handler at PC `$FC6D6C` writes `GfxBase->LOFlist` to COP2LC
every frame. The active copper list at `$2368` (chip-only) does
`MOVE $08A=$0` (COPJMP2) at the end of its sequence, jumping the
copper to COP2LC.

When COP2LC = chip-RAM ExecBase (`$0676`), the copper executes
ExecBase library-struct bytes as copper instructions. Decoding the
bytes at offset `$067E` from chip-only's ExecBase:

```
$067E: 09 00 00 FC   →  copper MOVE BPLCON0=$00FC
```

And many subsequent bytes decode as `MOVE INTENA=...`, `MOVE
BPL1PT=...`, etc. The copper writes garbage to chip registers
including INTENA — clearing SOFTINT.

### Why slow-RAM survives the same toxicity

In slow-RAM, COP2LC = `$C00276` (slow-RAM ExecBase) for a window. The
bytes at slow-RAM ExecBase are different from chip-RAM ExecBase
because the structures sit at different addresses with different
neighboring data. By chance, they don't clear SOFTINT during the
window. And slow-RAM advances past the placeholder state quickly
(by frame 230, COP2LC = `$0000B888` = real list).

### The deadlock

1. Chip-only copper writes garbage → INTENA loses SOFTINT bit
2. Without SOFTINT, the scheduler can't dispatch tasks
3. The task that populates `View->ViewPort` never runs
4. MrgCop keeps early-exiting (View->ViewPort still NULL)
5. `cprlist->start` never updated → `GfxBase->LOFlist` stays at
   ExecBase placeholder
6. Every frame, VBL handler re-writes COP2LC = ExecBase
7. Every frame, copper re-corrupts chip registers
8. Permanent deadlock

### Why real V34 hardware boots chip-only

Real A500s with no expansion DO boot KS 1.3. So real hardware avoids
the toxicity. Plausible mechanisms:

- **Timing**: real V34 might set up COP2LC AFTER the real list is
  allocated, never triggering the placeholder-as-copper-code window.
- **COPEN gating**: V34 might disable bitplane DMA / copper DMA
  during the placeholder window.
- **Placeholder choice**: V34 might use a different "safe" pointer
  for the placeholder (e.g., a known harmless ROM address) rather
  than ExecBase.
- **Our timing differs**: our emulator advances the boot in a way
  that exposes the placeholder window when real V34 doesn't.

### The fix question

We don't yet know which of these mechanisms real V34 uses. Until
we do, the chip-only path remains broken. Options:

1. **Trace v34 boot in WinUAE/FS-UAE chip-only mode** with copper-
   instruction-level logging to see what COP2LC actually is during
   the equivalent frames. If real V34 never sets COP2LC = ExecBase,
   we have an emulator timing bug. If it does but doesn't crash,
   we have a different chipset modeling bug.
2. **Disable copper temporarily during placeholder window** as a
   workaround. Check DMACON.COPEN gating in the boot path. May not
   match real hardware behaviour but unblock the chip-only path.
3. **Document as known-broken**, keep the runtime workaround
   (`Amiga::new_with_slow_ram(_, 512 * 1024)`), revisit once the
   ECS / AGA variants are implemented (since those crates may make
   different choices that surface the issue differently).

The bug is now characterized to the **exact instruction, exact data
field, exact causal step**. The remaining work is identifying the
single mechanism that prevents real-hardware V34 from hitting this
trap.

## Findings (2026-04-19, fourth pass — root mechanism identified; SUPERSEDED by fifth pass)

### The mechanism, definitively

The chip-only path is killed by the copper jumping into ExecBase (chip-RAM
copy) and executing library struct fields as copper instructions. Direct
trace evidence (`tests/cop1lc_write_log.rs`):

**A routine at PC `$00FC6D6C` runs every frame in BOTH configs**, writing
`d0` to COP2LC. The d0 progression diverges:

| Frame | Chip-only d0 → COP2LC | Slow-RAM d0 → COP2LC |
|---|---|---|
| 85/108 | $00000000 (initial) | $00000000 |
| 87/109 | $00002408 | $000004C0 |
| 92/115 | **$00000676** (= chip-RAM ExecBase) | $00C00276 (= slow-RAM ExecBase) |
| 102+ | **$00000676** ← stuck, forever | (continues) |
| 230+ | (still stuck) | **$0000B888** (real per-frame copper-list buffer) |

Both configs first set COP2LC = ExecBase pointer as a placeholder.
Slow-RAM then advances to set COP2LC = a freshly-allocated real
copper-list buffer (`$B888`). Chip-only never reaches that step.

### Why the placeholder kills chip-only but not slow-RAM

The active copper list at `$2368` (chip-only) / `$0420` (slow-RAM) does
`MOVE $08A = $0000` — `COPJMP2`, which jumps the copper to COP2LC. So
COP2LC is reached every frame.

When COP2LC = ExecBase, the copper executes ExecBase struct fields as
copper instructions:

- **Slow-RAM (`$C00276`):** the bytes there happen to be benign as
  copper instructions, OR the system advances quickly past this state.
  Damage is invisible.
- **Chip-only (`$0676`):** the bytes are the bootstrap ExecBase
  positive-data area. At offset `$067E` the bytes `09 00 00 FC` decode
  as copper `MOVE BPLCON0 = $00FC` — corrupting the display register.
  Subsequent copper instructions from ExecBase data write more bogus
  values to BPL pointers, sprite pointers, and INTENA. By frame 87
  the visible display state is destroyed.

### What the system fails to do in chip-only

The "create per-frame copper-list buffer" step. In slow-RAM this happens
between frame 115 and frame 230 — d0 transitions from ExecBase to
`$B888`. In chip-only it never happens.

Why? Hypotheses (need next-pass to confirm):

- **A1 = $00C01E1E (slow-RAM) vs $0000221E (chip-only)** at PC `$FC6D6C`.
  These look like pointers into the SAME structure but in different
  memory regions. A1 is the structure base, +offset = the d0 source. So
  the structure exists in BOTH configs. Whatever WRITES the buffer
  address into that structure differs.

- The buffer-allocation routine probably calls `AllocMem(size,
  MEMF_CHIP|MEMF_PUBLIC|MEMF_CLEAR)`. Both configs have plenty of free
  chip RAM. So allocation should succeed.

- The CALLER of the allocation may run only when some preceding init
  step succeeds. That preceding step probably succeeds in slow-RAM and
  not in chip-only. Candidates: graphics.library Init, opening a View,
  setting up a screen.

### Concrete next steps (FOR THE NEXT INVESTIGATOR)

1. **Disassemble `$FC6D6C`** and follow back to find where d0 is loaded
   from. Walk BACK through the call chain to find the caller(s) and
   the structure being read.
2. **Watch writes to the d0-source address** (whatever offset of A1
   `$0000221E`/`$00C01E1E` corresponds to the field). The slow-RAM
   write of `$B888` to that field is the missing operation. Find what
   PC writes it; find why that PC doesn't run in chip-only.
3. **Compare the call stack at PC `$FC6D6C`** between configs. If the
   stack is identical but the structure field differs, the divergence
   is upstream of the routine. If the stack differs, the divergence
   is in how this routine is reached.

### Net architectural conclusion

The bug is NOT in our emulator's chipset, memory model, or arbitration.
KS 1.3 itself uses ExecBase as a placeholder for COP2LC during boot,
which only works if the bytes-at-ExecBase happen to be benign as copper
instructions OR if the system progresses past the placeholder state
quickly enough. In chip-only, neither holds: bytes at chip-RAM
ExecBase corrupt the display, and the system never progresses past
the placeholder.

The actual fix is to figure out what step in slow-RAM advances COP2LC
from `$C00276` to `$B888` and why it doesn't happen in chip-only.
That's a KS-internal logic question, not an emulator-architecture
question.

## Findings (2026-04-19, third pass — write-log traces, narrative corrected; SUPERSEDED by fourth pass)

### Memory layout is NOT the bug (correcting the second-pass theory)

The second-pass theory said chip-only's MemHeader at `$8C2` overlapped the
bootstrap ExecBase positive part `$676-$8DA`. Direct write-log capture
(`tests/execbase_write_log.rs`) refutes this:

- The chip-RAM-MemHeader writes appear at `$8C2` only in chip-only — not
  in slow-RAM. In slow-RAM, those bytes are zero-filled once during the
  initial low-memory clear and never touched again. The slow-RAM chip-RAM
  MemHeader actually lives at `$400`.
- Every AllocMem call in chip-only updates `mh_First` (`$8D2-$8D5`) and
  `mh_Free` (`$8DE-$8E1`) — recorded at PCs `$FC171A`, `$FC178A`,
  `$FC1722`, `$FC17B6`. mh_Free decreases monotonically from `$00079180`
  to `$00074378` (~5 KiB allocated by frame 250). Pattern is normal
  allocator activity.
- V34 ExecBase positive size is `$24C` (588 bytes), not the V37 `$264`.
  ExecBase covers `$676` → `$8C2`. MemHeader begins **exactly** at `$8C2`.
  No overlap.

### There is no "bootstrap-to-proper" ExecBase swap in V34

The second-pass also assumed a swap (V37 phase 9 at `$F80438-$F80498`)
that never completes in chip-only. Direct trace shows ONE ExecBase write
in both configs, both at the same PC `$00FC027A`:

```
MOVE.L A6, $4.W
```

The value of A6 differs because the V34 bootstrap allocator picks a
different region depending on memory availability:

| Config | PC | Value written to `$4` | Region |
|---|---|---|---|
| chip-only | `$FC027A` | `$00000676` | chip RAM |
| chip+slow | `$FC027A` | `$00C00276` | slow RAM |

So V34's allocator IS slow-RAM-aware: prefer slow RAM, fall back to chip
RAM. This is by design. There's no missing swap; both configs use a
single ExecBase placement decided at bootstrap.

### So where IS the bug?

We've now ruled out:
- Memory layout / overlap (no overlap, MemHeader sits cleanly at the
  end of ExecBase positive part)
- ExecBase swap failure (no swap; bootstrap value is the final value)
- AllocMem flag failure (no MEMF_FAST in any KS 1.3 AllocMem call)
- Memory exhaustion (475K free at frame 250)
- Reported RAM size mismatch (MaxLocMem=`$80000`, MaxExtMem=0 — both correct)
- ExecBase integrity (ChkBase = `~ExecBase`, valid)

What remains as plausible:

1. **CPU/chip-bus arbitration timing under sustained chip-RAM load.**
   With slow-RAM, most data structures (ExecBase, library bases, copper
   lists in some cases) live off the chip bus. CPU accesses to them
   never stall on Agnus DMA. With chip-only, every data-structure
   access goes through the chip bus and may stall. If our arbitration
   model has any subtle bug (missed RDY transition, off-by-one CCK,
   etc.) under heavy contention, chip-only exposes it.

2. **Floating-bus return value on unmapped reads.** Real A500 returns
   `$FFFF` (or last bus value) for unmapped addresses. Our emulator
   returns `$0` (Memory::read_byte fall-through). If V34 anywhere
   distinguishes "device present, returns 0" from "no device, floating
   bus", the chip-only path could trip a wrong branch. Worth a targeted
   audit: change `Memory::read_byte` fall-through to return `$FF`
   (`$FFFF` on word reads) and re-run the chip-only golden test.

3. **Copper executes garbage.** COP1LC briefly hits `$2368` at frame 86
   then collapses. `$2368` in chip RAM is whatever was there; the
   copper interprets the bytes as MOVE/WAIT/SKIP and writes random
   chip-register values. This is a SYMPTOM of (1) or (2), not a root
   cause — but it explains the visible teardown (BPLCON0 `$00FE`,
   etc.) at frame 87.

### Concrete next steps

1. ~~**Try the floating-bus fix.**~~ DONE — `Memory::read_byte` now
   returns `$FF` for unmapped reads (matching real A500 floating bus).
   All slow-RAM invariant tests still pass; chip-only golden fails
   IDENTICALLY (30072 pixels diff, same first-diff location). So
   not floating-bus-sensitive. The fix is left in place as it is more
   hardware-accurate either way.
2. **Trace COP1LCH/COP1LCL writes during boot.** Both halves of the
   copper list pointer are 16-bit writes to `$DFF080`/`$DFF082`. Track
   every write with PC. The pattern of writes will show whether the
   copper list ADDRESS is set correctly but the list itself is
   corrupt, or whether the address itself is wrong. Compare both
   configs.
3. **Audit chip-bus arbitration under heavy contention.** Specifically,
   what happens to `cpu.bus_status` transitions across multiple CCKs
   when DMA repeatedly preempts the CPU. The recent fix (decode
   address before gating) only addressed the gross "CPU on ROM stalls
   on DMA" case; subtler timing issues may remain.

## Findings (2026-04-19, second pass — runtime probes executed; SUPERSEDED by third pass)

### What we now know directly

Boot makes substantial progress, then **tears itself down**. INTENA progression
(every-frame trace, see `tests/intena_progression.rs`):

| frame | chip-only INTENA | DMACON | BPLCON0 | COP1LC | reading |
|---|---|---|---|---|---|
| 33 | `$4004` MASTER+SOFTINT | `$0200` | `$0200` | 0 | early init done |
| 85 | `$202C` (no master) | `$0250` | `$0200` | `$2368` | preparing display |
| **86** | **`$602C`** all bits | **`$03F0`** | **`$1302`** | **`$2368`** | **DISPLAY UP for one frame** |
| 87 | `$2020` no SOFTINT/DSKBLK | `$03F0` | `$00FE` | `$2368` | being torn down |
| 89 | `$6020` no SOFTINT/DSKBLK | `$03F0` | `$0000` | `$0000` | display gone |
| 158 | `$6020` | `$02D0` | `$0000` | `$0000` | stable wrong state |
| 288 | `$2000` master OFF | `$0210` | `$0000` | `$08000000` (garbage) | terminal |

Compare slow-RAM:

| frame | slow-RAM INTENA | DMACON | BPLCON0 | COP1LC |
|---|---|---|---|---|
| 55 | `$4004` MASTER+SOFTINT | `$0200` | `$0200` | 0 |
| 107 | `$600C` MASTER+EXTER+DSKBLK+SOFTINT | `$0210` | `$0200` | 0 |
| **108** | **`$602C`** | **`$03F0`** | **`$1302`** | **`$00000420`** |
| 217+ | `$602C` (stable) | `$03D0` | `$0302` | `$00000420` |

PC trace (`tests/chip_only_pc_trace.rs`) corroborates: chip-only ends up
permanently parked at `$FC0F94`, which disassembles as the post-`STOP`
return point in the scheduler idle loop:

```
$FC0F90: 4E72 2000   STOP   #$2000          ; supervisor wait
$FC0F94: 60E6        BRA.B  $FC0F7C         ; dispatch loop
```

The dispatch loop checks the task-ready queue (via `MOVEA.L (A0),A3` then
`MOVE.L (A3),D0; BNE.B`). With SOFTINT and DSKBLK never re-enabled, the
queue stays empty and the loop sleeps forever.

### What this rules out

- **Hypothesis H1 (MEMF_FAST allocation failure): WRONG.** The KS 1.3 ROM
  contains 33 AllocMem call sites. Disassembly shows ZERO use
  `MEMF_FAST`. All allocations are `MEMF_PUBLIC` ± `MEMF_CHIP` /
  `MEMF_CLEAR` / no flags. Slow RAM's higher priority cannot be the
  cause because no allocator ever asks for it.
- **MemList corruption:** The "MemHeader#2 garbage" in the original probe
  was a probe artefact — walking past a single-entry list into the LH
  sentinel. MemList is fine in both configs.
- **Memory exhaustion:** chip-only MemHeader has 475K free at frame 250.
  Plenty of room.

### What it points to: bootstrap-vs-proper ExecBase overlap

`ExecBase=$00000676` in chip-only — but the chip-RAM MemHeader covers
`$8E8-$7E800`, so `$676` cannot have come from AllocMem. This **must** be
the bootstrap ExecBase from the pre-Exec phase (V37 trace `$F801FE`),
allocated raw from the `$400+` pool.

In slow-RAM, `ExecBase=$00C00276` — the post-AllocMem proper ExecBase
sitting in slow RAM. The swap (V37 `$F80438-$F80498`) succeeded.

In chip-only the swap is the only plausible source of the frame-86 → 87
teardown:
1. AllocMem succeeds for the new $57C ExecBase, allocating somewhere
   in chip RAM
2. Copy old ExecBase fields to new
3. Update `[$00000004]` to point to new ExecBase
4. Free old bootstrap area

If step 1 returns an address that **overlaps** the bootstrap ExecBase
(both in chip RAM, both small structures, both around `$300`-`$700`),
step 2 corrupts the source mid-copy. The system runs for a few
instructions in a half-corrupt state, then control returns to the now-
broken bootstrap, which still sits at `$00000004`. Subsequent dispatch
into Exec library functions (via the jump table that was just
overwritten with garbage) lands on bogus PCs, registers get clobbered,
and the system limps into the scheduler idle loop with INTENA
half-configured.

That ExecBase pointer never updated past `$676` is the strongest signal
the swap aborted partway.

### Concrete next steps

1. **Dump the bootstrap ExecBase pool layout in the running emulator.**
   The bootstrap raw allocator at `$F8021A` allocates from `$400+` with
   a $57C-byte ExecBase placed via `move.l a6, $4(a7)` and adjusted by
   `suba.w #$fce8`. If our raw allocator returns an address that
   conflicts with later AllocMem placement, the swap collides. Trace
   both addresses — they should be in non-overlapping regions.
2. **Hook `Memory::write_byte` to log writes to `$00000004` (ExecBase
   pointer).** A successful swap writes once. A failing swap may
   never write or write to a bogus value. If the write happens but
   ExecBase still reads as `$676`, the write itself or the address
   decode is broken.
3. **Compare AllocMem return values across the two configs.** Run
   slow-RAM through the same instrumentation; the new ExecBase
   allocation should land at `$C00276` (matches the probe). In
   chip-only, what does AllocMem return? If it returns an address
   inside the bootstrap area (~$400-$700), confirmed cause.
4. **Look at our raw bootstrap allocator** (whatever V34's equivalent
   of V37's `$F81C02` is). Either we run it correctly and the bug is
   downstream in AllocMem fragment management, or we run it
   incorrectly and the bootstrap pool starts in the wrong place.

### What this means for the runtime workaround

`runtime.rs:517` (`Amiga::new_with_slow_ram(_, 512 * 1024)`) is now
known to mask a real bug, not a hardware necessity. Real A500s with no
expansion boot KS 1.3 to insert-disk; FS-UAE with chip-only matches
pixel-exact (per the golden capture). Removing the workaround stays
dependent on this fix.

### Visual evidence

`tests/golden/a500-ks13-512k-chip-frame250.actual.png` shows what the
torn-down state looks like — white background (default COLOR00) with a
small red/black artefact in the top-right. The artefact is leftover
shift-register data leaking through Denise's BPU=0 fallback (which
itself is a separate quirk — see
`amiga-denise-bpu-zero-rendering.md` — but only visible because the
boot stalls before clearing the registers).

## Findings (2026-04-19, original first pass — static-analysis only)

### Static-analysis pass (probe written, runtime exec deferred)

Walked the Kickstart 1.3 boot path against our memory / Gary / CPU-bus
implementation and the V37 boot trace at
`~/Projects/Emu198x-Reference/_organised/by-system/commodore-amiga/amiga-rom-boot-traces.md`
(V33 and V34 are documented to use the same shape, only the chip-RAM
probe ceiling and expansion-RAM probe range differ — V33 stops at 512K
chip and probes $C00000-$C80000).

A diagnostic probe was added at
`crates/machine-commodore-amiga/tests/chip_only_boot_probe.rs`. It runs
the real Kickstart 1.3 ROM in both configs for 250 frames and dumps:
DMACON / BPLCON0 / INTENA / INTREQ, copper PC and COP1LC, COLOR00/01,
the vector-table SSP/PC, the ExecBase pointer, ChkBase, ColdCapture,
MaxLocMem, last-Alert, the MemList head, and the first one or two
MemHeader entries (attrs / lower / upper / free). Run with:

```
cargo test -p machine-commodore-amiga --test chip_only_boot_probe \
  -- --ignored --nocapture
```

The probe was authored but **not executed** in this session — sandbox
denied `cargo build` / `cargo test`, so the runtime divergence has not
yet been observed directly. Findings below are static-only.

### What is the same between the two configs

Walking the configs end-to-end (`Amiga::new` vs
`Amiga::new_with_slow_ram(_, 512 * 1024)`):

- Chip RAM size, layout, and aliasing mask are identical (512K with
  `mask = 0x7FFFF`, so $80000 wraps to $0 — V33 chip-RAM probe relies
  on this for top-of-RAM detection).
- 68000 reset vector handling, OVL handshake, custom-register decode,
  CIA register handling — all identical.
- `service_cpu_bus` treats `ChipSelect::SlowRam` and `ChipSelect::Unmapped`
  via the **same arm**: read goes through `Memory::read_word` (returns
  zero when slow RAM is absent), write through `Memory::write_byte`
  (silently drops when slow RAM is absent). No code-path divergence
  inside the bus servicer.

### What differs

Two state changes between the two configs, and only two:

1. `Gary::slow_ram_present` flips from `false` to `true`, so
   `gary.decode($C00000-$D7FFFF)` returns `SlowRam` instead of
   `Unmapped`.
2. `Memory::slow_ram` becomes a 512 KiB live buffer instead of an empty
   `Vec`.

Both have one downstream effect each:

- Effect of (1): the chip-bus-arbitration gate in `lib.rs:410-423`
  treats $C00000 accesses as `needs_chip_bus = true` (with slow RAM)
  vs `false` (without). With slow RAM, the CPU can stall on Agnus
  arbitration when reading/writing slow RAM. Without, it proceeds at
  full speed. **Functionally either is correct for the current
  arbitration model.**
- Effect of (2): writes to $C00000 actually land somewhere (slow RAM)
  with config-2; with config-1 they are dropped. Reads return slow-RAM
  contents vs zero.

### Why the chip-only path is *expected* to work

Kickstart 1.3 (V33/V34) probes for expansion RAM at $C00000-$C80000
(ref: V33 column in §6 of `amiga-rom-boot-traces.md`). When the probe
finds nothing — which is exactly what our chip-only emulator should
report (writes drop, reads return zero, no wrap) — the V37 trace at
$F802D2 shows the boot continues with `beq.b $f802e2` ("no expansion
RAM found, skip"). V33 follows the same pattern.

Real A500 hardware ships with 512K chip RAM and no slow RAM. KS 1.3
boots that config to insert-disk on real hardware, WinUAE, FS-UAE, and
vAmiga without complaint. The only static reason our chip-only path
should diverge is if (a) one of the two state differences above
causes a subtle behavioural change downstream, or (b) we have a bug
that is currently masked by the slow-RAM workaround.

### Hypotheses for the actual divergence

In rough order of likelihood — all need the probe to run to confirm:

#### H1 — AllocMem fallback into chip RAM exposes a tighter constraint

With slow RAM: `AllocMem(MEMF_PUBLIC, 0x57C)` for the second-stage
ExecBase (V37 trace $F80444 — V33 follows the same shape with a
smaller PosSize) lands in the higher-priority slow-RAM MemHeader at
$C00000+. Plus all 25-ish exec-library bases, copper lists, sprite
data, and graphics buffers also tend to land in slow RAM by priority.

Without slow RAM: every one of those allocations falls back to the
chip-RAM MemHeader. Total demand from Phase 3-9 is well under 512K, so
this should fit — **but** if any allocation specifies `MEMF_FAST`
explicitly, it would fail outright. KS 1.3's strap, layers, and
intuition init paths are the most likely candidates. AllocMem failure
returns 0 → caller dereferences NULL → typical symptom is a stuck
pre-display screen with no DMACON / BPLCON0 progress. **Matches the
described symptom.** Probe should look for low-memory writes that
overlap, or for an Alert raised before the insert-disk handoff.

#### H2 — Gary chip-bus-gate misclassifying *which* accesses are chip-bus

The arbitration gate (lib.rs:410-423) treats $C00000 as
`needs_chip_bus` only when `slow_ram_present`. If a downstream chip
relies on the gate's classification rather than its own decode (e.g.
copper or blitter setup-state expectations), the chip-only path could
be skipping a stall that the slow-RAM path correctly applies. Less
likely than H1 because both paths have run through extensive Tom Harte
and Spectrum-style timing tests, but worth a probe.

#### H3 — Silent-drop writes to $C00000 land on a routine that expects
*something* readable

Less likely on KS 1.3 specifically (the slow-RAM probe is documented
to be tolerant of nothing-being-there), but if any later boot stage
does a `MOVEA.L $C00000, A0` and uses A0 as a base for a structure
walk, getting `$0000_0000` back vs a buffer-of-zeros would diverge.
The probe's MemList walk should surface this — if any MemHeader points
into $C00000 region without slow RAM, that's the smoking gun.

### Concrete next steps

1. **Run the probe** (`cargo test -p machine-commodore-amiga --test
   chip_only_boot_probe -- --ignored --nocapture`). Diff the two dumps.
   First field that differs is the divergence anchor.
2. If ExecBase pointer is `0x00000000` or out-of-range in the chip-only
   dump → AllocMem failed → instrument `Memory::write_byte` for writes
   to ExecBase+0x202 (last-Alert field) and follow back to the failing
   call. (H1.)
3. If ExecBase looks valid but DMACON/BPLCON0 stay zero → instrument
   `write_custom_reg` to log every write and find where Phase 8
   stalls. (H2 or H3.)
4. Once the divergence is localised, decide whether the fix lives in
   the chip-bus gate, in `Memory`, or in a chip module. Then verify
   the kickstart_boot_invariants tests still pass with
   `Amiga::new()` substituted for `new_with_slow_ram(_, 512 * 1024)`.

### Code references checked

- `crates/machine-commodore-amiga/src/lib.rs:132-229` — `Amiga::new`
  / `new_with_slow_ram` constructors. Only difference is
  `gary.set_slow_ram_present(...)` and `Memory::new`'s `slow_ram_size`.
- `crates/machine-commodore-amiga/src/lib.rs:408-436` — chip-bus
  arbitration gate.
- `crates/machine-commodore-amiga/src/lib.rs:691-760` — `service_cpu_bus`
  arms for ChipRam, SlowRam, Rom, Unmapped (SlowRam/Rom/Unmapped share
  the same arm).
- `crates/machine-commodore-amiga/src/memory.rs:51-108` — read/write
  byte/word for chip RAM, slow RAM, ROM, overlay.
- `crates/commodore-gary/src/lib.rs:163-249` — Gary decode; line 224
  is the slow-RAM gate.
- `crates/runtime-commodore-amiga/src/runtime.rs:513-518` — the
  workaround call (note: actual line is 517, not 535 as in §"Symptom"
  above — fix that line number when this entry is next touched).
- `crates/machine-commodore-amiga/tests/kickstart_boot_invariants.rs:42-49`
  — the comment claiming "KS 1.2/1.3 need somewhere to put ExecBase
  that isn't chip RAM" is at odds with real A500 hardware behaviour.
  Either this comment is wrong, or it documents a real symptom whose
  underlying cause is the bug we are looking for.

## Related

- `wiki/decisions/amiga-architecture-review.md`
- `wiki/decisions/amiga-port-plan.md`
- `crates/machine-commodore-amiga/tests/kickstart_boot_invariants.rs` — current tests, gated on slow-RAM config
- `crates/machine-commodore-amiga/tests/chip_only_boot_probe.rs` — diagnostic probe added 2026-04-19
- `crates/runtime-commodore-amiga/src/runtime.rs:517` — the workaround

# Decision: Amiga machine rollout plan

**Date:** 2026-05-22
**Status:** A1200 Stages A–K landed 2026-05-22. Stage J fixed a
68010+ RTE bug (RTE now pops the Format/Vector word). Stage K
added the 68020+ 28-byte Format-$A "short bus fault" frame for
group-0 (bus/address error) exceptions, replacing the 14-byte
68000-style frame, and taught RTE to pop the 20-byte tail when
the popped F/V word's Format nibble is `$A`. KS 3.1 now alerts
with the **specific** code `$80000003` (`AN_ExcptVect`,
"exception vector check failed") instead of the previous chaotic
codes — a sign that the frame format is now agreeing with KS
and the remaining mismatch is at a higher level (the stack
leak from KS's SHORTER pseudo-frame routine, which our 8-byte
RTE pop turns into a 2-byte leak per call). The chain is now:
priv-viol probes succeed → ExitIntr's RTE at `$F81398` pops a
slightly-misaligned PC → vec-3 address error → `$F80B0E`
handler → alert with `$80000003`. Cycle 1 reaches 5538 unique
PCs (2.4× pre-Stage-J), IPL drops at frame 115, video chips
init reached.

## What this is

The order in which the remaining Amiga variants get built, and the
chip-extraction queue that minimises rework across them.

This document is **sequencing**, not architecture. The architectural
seams (chip substrate, 68k family, display surface, storage zoo,
boot CI) are already captured in
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md).
This plan picks an order for working through them.

## Variant zoo with deltas vs current OCS / ECS baseline

| Machine | Chipset | CPU | New I/O chips | Firmware |
|---|---|---|---|---|
| **A1200** | AGA (Alice + Lisa) | 68EC020 | Gayle | KS 3.0 / 3.1 |
| **A600** | ECS (already done) | 68000 (done) | Gayle (shared w/ A1200) | KS 2.05 |
| **A2000B** | ECS or OCS (done) | 68000 (done) | Zorro-II extras only | KS 1.3 / 2.04 |
| **CDTV** | OCS (done) | 68000 (done) | DMAC, CD-ROM | KS 1.3 + CDTV ROM |
| **A4000-030** | AGA (shared w/ A1200) | 68030 | Fat Gary, Ramsey | KS 3.0 / 3.1 |
| **A3000** | ECS (done) | 68030 (shared w/ A4000) | Fat Gary, Ramsey, Buster | KS 1.4 → 2.04 |
| **CD32** | AGA (shared w/ A1200) | 68EC020 (shared w/ A1200) | Gayle (shared), **Akiko**, CD-ROM | KS 3.1 + CD32 ROM |
| **A4000-040** | AGA (shared) | 68040 | (shared w/ A4000-030) | KS 3.1 |
| **Vampire V2 / V4** | AGA + FPGA RTG | AC68080 | RTG framebuffer, SD card | KS 3.x + Apollo OS |

## Chip extraction queue, ordered by cumulative unlock

1. **Gayle** — unlocks A600, A1200, CD32. Three-way win. Most of the donor's
   2334-line crate is NE2000 PCMCIA (drop it); the minimum-viable subset is
   ID register + IDE-empty status + Gayle CS register, ~500 lines.
2. **AGA Alice + Lisa** — unlocks A1200, A4000, CD32. Donor crates are 278 +
   372 lines, both already wrap-don't-clone over their ECS counterparts;
   they should compose with the Seam-1 `DeniseChip` trait without changes.
3. **Fat Gary + Ramsey** — unlocks A3000 + A4000. Not yet ported from any donor.
4. **DMAC** — unlocks CDTV. Not yet ported.
5. **Akiko** — unlocks CD32. Heaviest of the listed chips; chunky-to-planar
   conversion + CD-ROM controller in one die.
6. **Buster** — unlocks A3000 SCSI/DMA.
7. **A2091 / GVP SCSI** — A2000/A3000 with hard disks (post-base-boot only).
8. **AC68080** — Vampire targets. Separate clean-room project; not a donor port.

## Rollout order

1. **A1200** — *current.* High leverage: validates Cpu68020 in a real
   machine, extracts Gayle (unlocks A600, CD32), extracts Alice + Lisa
   (unlocks A4000, CD32) all in one push.
2. **A600** — cheap follow-on. ECS chipset already wired; reuses Gayle from
   A1200 extraction. No CPU change. Mostly a KS 2.05 firmware swap.
3. **CDTV** — orthogonal. OCS + 68000 already done. New work is DMAC +
   CD-ROM peripheral + CDTV firmware. Validates the CD-ROM lane on the
   simpler chipset before CD32 combines everything at once.
4. **A4000-030** — validates Cpu68030 wiring in a real host (the same way
   A1200 validated Cpu68020). Reuses Alice + Lisa from A1200.
5. **CD32** — combines A1200 chipset + CDTV CD-ROM lane + Akiko. The Akiko
   chunky-to-planar implementation is the headline new work.
6. **A3000** — Cpu68030 in an ECS host. Adds Fat Gary + Ramsey + Buster
   (most of the new chip work) on a chipset (ECS) that's already done.
7. **A4000-040** — Cpu68040 swap on the existing A4000-030 chassis.
   Incremental; mostly validates the Cpu68040 wrapper.
8. **Vampire V2 / V4** — separate track. AC68080 + RTG + SD storage.
   Long-horizon per `project_amiga_long_term_scope.md`.

## Sequencing rationale

- **A1200 before A600** even though A600 is cheaper, because A1200's
  Cpu68020 validation is the high-leverage thing — once that holds, A600
  is "swap KS 2.05 firmware and wire Gayle".
- **CDTV before CD32** so CD-ROM peripheral is exercised on a familiar
  (OCS) chipset before CD32 stacks it onto AGA + Akiko.
- **A4000-030 before CD32** so Cpu68030 is validated in a real host before
  CD32 adds Akiko as a separate axis of change.
- **A3000 after A4000-030** so Fat Gary + Ramsey are validated on AGA
  first (where there's more catalogue demand), then transplanted to ECS
  for A3000.

## Per-machine "minimum viable" budget

Each machine targets *"KS reaches the startup screen"* as its Stage C
deliverable. No floppy boot, no Workbench, no software catalogue. The
budget per machine is roughly:

- Extract new chip crates (porting from donor if available)
- Scaffold new `machine-*` crate (parallel to ECS/OCS shape)
- Wire CPU variant if it differs from base (68020/68030/68040)
- Load Kickstart ROM at the right address
- Run N frames and document the first crash/hang

Workbench / catalogue / floppy / IDE boot are post-Stage-C and tracked
per-machine on demand, not in this rollout.

## A1200 Stage C findings — 2026-05-22

Loading `kick31a1200.rom` (KS 3.1 r40.068, 512 KiB) into the A1200
machine and running 50 PAL frames (~1s emulated) produces:

- **Initial PC** $F800D2 (reset vector → KS entry point).
- **Final PC** $F80E60 — ~3.6 KB into the ROM.
- **667 unique PCs visited.** Healthy boot progress; not a 2-byte
  tight loop.
- **1056 PC excursions below $F80000.** KS jumped into chip-RAM
  trampolines or jumps that exited the ROM window — expected during
  exec setup.
- **10 custom-register writes**, **4 INTENA writes** — chipset and
  interrupt-controller surface is being exercised.
- **A4 = $00F3686C** at the stall — pointer into the
  diagnostic-ROM area ($F00000-$F7FFFF), which is unmapped in our
  A1200 build. KS 3.x scans this region during early init looking
  for a third-party diagnostic image.
- **SR = $2701** — supervisor mode, IPL mask 7 (interrupts masked).
  The chipset is writing INTENA but the CPU mask hasn't dropped, so
  even if VBL fires the CPU won't service it. Likely KS hasn't
  reached its IPL-lowering step yet.

**Disassembly at the stall** (PC = $F80E60):

```
$F80E60: 57C9 FFF8   DBEQ  D1, *-6      ; decrement-and-branch
$F80E64: 6610        BNE   *+18
$F80E66: 4BEC FFFE   LEA   -2(A4), A5   ; A4 = $F3686C → A5 = $F3686A
$F80E6A: BBD4        CMP.L (A5), D5     ; D5 = 0
$F80E6C: 66EE        BNE   *-16
$F80E6E: 610C        BSR   *+14
```

This is a memory-scan loop comparing 32-bit words against zero,
walking A4 backward through the $F00000-$F7FFFF region. Without
either a diagnostic ROM image or open-bus reads returning zero,
the inner BNE never falls through and the outer DBEQ counts D1
down — but D1 (low word $0002 at the report time) is depleting
slowly and may eventually fall through. Whether it does within
"reasonable" boot time is Stage D's first question.

## A1200 Stage D findings — 2026-05-22

Extended the boot test from 50 to 5000 PAL frames (~100 seconds
emulated) and added milestone tracking (IPL drops, VBR moves) plus
hot-read / hot-write diagnostics.

### What the "stall" at $F80E60 actually was

The $F80E60 PC sample was inside the **standard KS Resident-module
scanner** (`$F80E42`-`$F80E7B`): the routine walks memory looking
for the `$4AFC` matchword that marks `struct Resident` instances
(`exec/resident.h`). The scan reaches end-of-region in ~7 emulated
frames — what Stage C caught was a slow scan progressing, not a
wedge.

### The real picture: KS is in an early-init do/while loop

Over 5000 frames, KS 3.1 executes 2,315 unique PCs **but no new
ones appear after frame ~50**. The same code path runs ~15 times:

- `$F80446`-`$F80450`: 80ms delay loop ($15000 = 86,016 writes of
  `0` to `$DFF180`/COLOR00 per pass). Ran 15× → 1,287,539 COLOR00
  writes total.
- `$F80452`: `MOVE.W #$4000, $DFF09A` — clear INTENA master.
- `$F8045A`: `BRA.W` into the Resident scanner at `$F80DB0`.
- Some path takes KS back to `$F80446` and the cycle repeats.

Milestones over 5000 frames:
- `min IPL = 7`, never dropped → KS never reaches "interrupts on".
- `VBR = 0`, never moved → no exception-table relocation.
- Only 2 register *read* kinds: SERDATR (`$DFF018`, 90 reads) and
  INTENAR (`$DFF01C`, 30 reads). No VPOSR, no DENISEID, no
  VHPOSR — KS hasn't begun chipset / Agnus / Denise identification.

### The leading hypothesis

KS 3.1 fails an early validation check before chipset-identification,
restarting its init sequence each time. Three candidates by
likelihood:

1. **Memory probe failure.** KS writes test patterns to chip RAM
   and reads them back. The A1200 boot ROM does an explicit memory
   test pass before progressing. If a chip-RAM byte fails the
   pattern check, KS reboots itself. Our `chip_ram` is plain `Vec<u8>`
   — unlikely to fail simple read-back, but the test patterns may
   stress address-decode aliasing that the 19-bit Agnus address mask
   handles differently for the 2 MiB chip-RAM A1200.
2. **CPU type detection fails.** KS 3.1 detects 68020 via specific
   instruction probes (CACR access, MOVEC of 68020-only control
   registers). Our Cpu68020 should handle these via the variant
   hooks, but a single misimplemented opcode would drop KS into the
   "unknown CPU" reset path.
3. **Chipset reset readback.** KS writes to DMACON / INTENA /
   ADKCON / SERPER then re-reads to confirm the chipset accepted
   the clear. If our INTENAR / DMACONR reads return stale or
   unexpected values, KS may retry.

### Stage D follow-ups (Stage E candidates)

1. **Add a "did we trap?" detector** to the test: count exceptions
   taken (group 1 / group 2 / line-A / line-F). If KS is hitting a
   non-existent-instruction trap and falling into a reset handler,
   that would explain the looping. Cheapest to implement.
2. **Trace `$DFF09A` INTENA / `$DFF09C` INTREQ / `$DFF096` DMACON
   writes** and verify the readback path returns matching values.
3. **Compare against WinUAE booting the same ROM** with `debug` mode
   capturing the same first 200 PCs. The divergence point identifies
   the bug.
4. **Check our 2 MiB chip-RAM aliasing.** Our `Memory::chip_ram` has
   a `chip_ram_mask` that should wrap addresses above 512K back into
   the installed pool. For 2 MiB the mask should be `$001FFFFF`;
   verify that's what we configured.

## A1200 Stage E findings — 2026-05-22

Stage E candidate 1 (the exception counter) immediately surfaced
the root cause. The diagnostic showed **30 line-F traps**, two per
init loop iteration — the FPU presence probe at `$F80CA0`
(`FNEG.X FP0; FNEG.S FP0; FSAVE`). Traps fire and are handled,
KS notes "no FPU", proceeds.

The wedge was *one routine after* the FPU probe. Disassembling
`$F83616+`:

```
MOVE.W  #$2700, SR       ; supervisor, IPL=7
MOVEQ   #5, D1            ; try 5 times
MOVE.W  #$0174, $DFF032   ; SERPER = baud divisor
loop:
  MOVEQ #-1, D0
  BSET  #1, $BFE001       ; LED off, delay
  DBF   D0, -10
  BCLR  #1, $BFE001       ; LED on,  delay
  DBF   D0, -10
  MOVE.W $DFF018, D0      ; read SERDATR
  MOVE.W #$0800, $DFF09C  ; clear SOFTINT in INTREQ
  AND.B #$7F, D0
  CMP.B #$7F, D0          ; expect low 7 bits = $7F
  DBEQ  D1, loop          ; retry up to 6 times on mismatch
```

This is the **DiagAlive routine** — Kickstart's power-on chipset
self-test. It expects to read `$FF` in the SERDATR data byte
(the idle / mark state of the RXD line, which samples all-ones
when no serial activity). Low 7 bits = `$7F` ⇒ healthy.

Our Paula was defaulting `serial_rx_byte` to `0`. The fix is one
line:

```rust
// crates/commodore-paula-8364/src/lib.rs Paula8364::new()
serial_rx_byte: 0x00FF,  // idle RXD samples as all-ones (mark)
```

### Impact

| Metric | Before Stage E | After Stage E |
|---|---|---|
| Unique PCs visited (5000 frames) | 2,315 | 2,448 |
| Line-F traps (5000 frames) | 30 | 4 |
| COLOR00 writes (delay-loop passes) | 1,287,539 (×15) | 86,018 (×1) |
| Final PC | `$F80454` (early init) | `$F83190` (mid init) |
| Final SSP | `$1FFFE6` | `$1FFF86` (proper stack) |

DiagAlive now passes on the first try. KS proceeds through
chipset reset, scans residents, and reaches a new wait loop at
`$F83182` — the "wait for serial byte with LED-blink animation"
routine, with `MOVE.L #$00091FFF, D1` setting up a ~74K-iteration
timeout. KS reads SERDATR 10M+ times waiting for RBF to assert.

No regression: `commodore-paula-8364` lib tests pass, `runtime-commodore-amiga`
tests pass (including the Workbench-1.3 snapshot round-trip boot
test that exercises the OCS Paula path end-to-end).

## A1200 Stage F findings — 2026-05-22

Confirmed what `$F83182` is and what's keeping KS in it.

### 50K-frame run rules out natural timeout

Running the boot test for 50,000 PAL frames (~17 minutes emulated
time) instead of 5,000 produces **no new code paths visited**:
unique-PC count stays at 2,448. The Wack-style loop runs forever
on its own.

### The loop *is* a Wack-style serial-debugger dispatcher

Disassembling `$F832E0`-`$F8331C`:

```
$F832E0: TST.L  D7
$F832E2: BEQ.S  RTS                ; if D7=0, return immediately
$F832E4: MOVEQ  #9, D2             ; retry counter (10 tries)
$F832E6: MOVE.L D7, D0
$F832E8: BSR.W  $F8315A            ; print/send D0 over serial
$F832EC: BSR.W  $F83182            ; receive a byte
$F832F0: BPL.S  $F832F8            ; got byte — process it
$F832F2: DBF    D2, $F832E6        ; retry 10x on timeout
$F832F6: RTS
$F832F8: CMP.B  #$1B, D1           ; ESC -> exit
$F832FC: BEQ.S  $F832F6
$F832FE: CMP.B  #$AF, D1           ; magic byte $AF starts command
$F83302: BNE.S  $F832EC            ; not AF -> wait for next byte
$F83304: BSR.W  $F83182            ; got AF; receive command index
$F83308: BMI.S  $F832EC
$F8330A: MOVEQ  #9, D2
$F8330C: SUBQ.B #1, D1
$F8330E: BMI.S  $F832EC
$F83310: LEA    jump_table(PC), A0
$F83314: MOVE.L (A0)+, D0
$F83316: BEQ.S  $F832E0            ; end of table -> loop
$F83318: DBF    D1, $F83316
$F8331C: PEA    $F832DE             ; push return-to-loop
$F83320: MOVEA.L D0, A0
$F83322: JMP    (A0)                ; jump to handler
```

This is the **canonical Wack ("ROMWack")** dispatcher: read a byte,
expect `$1B` (ESC = exit) or `$AF` (command prefix) followed by a
1-byte index, jump through a 16-entry handler table. The handler
table at `$F83324`-`$F83363` has entries pointing into `$F83368`-`$F83540`,
which are command handlers that read further bytes via the same
`$F83182` byte-receive routine.

### Caller tracking shows the entry path

Detecting fresh entries to `$F83182` (PC enters from a different
PC) and keying by the previous PC:

- **68 entries from `$F83180`** — RTS at the end of the serial-send
  helper (`$F83170`-`$F83180`, the routine that writes SERDAT and
  RTSes). Each `BSR $F83158` to print a byte returns to the caller,
  whose next instruction frequently runs into another byte-receive
  call.
- **17 entries from `$F832F2`** — the BSR.W at `$F832EC` returning
  successfully and immediately doing another receive.

That's 85 entries total in 5000 frames (~17 per outer dispatcher
invocation × ~5 outer invocations). KS is calling the Wack
dispatcher repeatedly — something at a higher level keeps
re-entering Wack.

### Injecting ESC ($1B) doesn't break the cycle

Probe: inject `$1B` into Paula's receive buffer on every fresh
entry to `$F83182`. The Wack dispatcher correctly sees ESC and
RTSes. But then **KS oscillates back into DiagAlive 15 times**
(1.29M COLOR00 writes again), each time falling back into Wack.

So the Wack entry isn't gated by "needs an ESC byte". KS is
deliberately calling Wack as part of its boot path, and Wack-exit
returns control to a loop that re-invokes Wack.

### Chip-RAM exception vectors are mostly correct

Dumping the vector table KS installed:

| Vec | Address | Notes |
|---|---|---|
| 2 (bus err) | `$F80B0E` | Standard handler |
| 3 (addr err) | `$F80B0E` | Same as bus err |
| 4 (illegal) | `$F80AD2` | Trampoline → common dispatcher `$F80B3C` |
| 5-7, 9-14 | `$F80AD4`-`$F80AE6` | Trampolines, 2 bytes apart |
| **8 (priv viol)** | **`$F83616`** | **DiagAlive!** (privileged-instruction self-installation) |
| 24 (spurious) | `$F80AFA` | Standard |
| 31 (autovec 7) | `$F8325E` | NMI / Wack entry candidate |

Vector 8 pointing to `$F83616` is intentional — DiagAlive's first
instruction is `MOVE.W #$2700, SR`, a privileged instruction.
DiagAlive installs itself there so user-mode priv violations
trigger a fresh DiagAlive run.

### The OVL has cleared correctly

`m.memory().overlay() == false` at the wedge — KS has cleared the
overlay, so chip-RAM low addresses now serve real chip RAM (not
the ROM mirror). The exception vectors above are reading correctly.

## A1200 Stage G findings — 2026-05-22

Two surprises that change the picture.

### Stage E was wrong — reverted

Tracing PC entries into the Wack prologue (`$F8326E`) showed they
all came from `prev_pc = $F83206` — the `BRA.W` immediately after
the `BEQ.S $F8326E` at `$F83202`. Disassembling further: the BEQ
sits inside a self-referencing trampoline at `$F831EA-$F83204`
that pushes `"SAD!"` magic onto the stack then CMP/BEQ-always-takes
into Wack. Chip RAM at `$000014F2` holds `$F831F6` — that's
`ExecBase->DebugEntry` (offset $42), so any call to
`exec.library/Debug` lands in this trampoline.

Crucially, the routine at `$F83616` (which I'd been calling
"DiagAlive") is NOT just the priv-violation handler — it's part
of the normal boot path. KS executes it as `MOVE.W #$2700, SR;
MOVEQ #5, D1; ...` straight through, AND simultaneously installs
itself as vec 8 via `MOVE.L #$F83616, $0020.W` at `$F8360C` (the
self-install IS the previous instruction, falling through to the
handler body).

The exit logic of this routine:

- If `(SERDATR & $7F) == $7F` → `Z=1` → `BEQ.W $F831EA` taken
  → enters Wack trampoline → Wack.
- Else (timeout after 6 retries) → `BRA.W $F80440` → delay loop
  → INTENA clear → `BRA.W $F80DB8` → `JSR -30(A6)` (exec/Supervisor)
  → continue init.

Stage E's `serial_rx_byte: 0xFF` default made the SERDATR check
match on the first iteration, so KS took the BEQ path into Wack.
**That was the wrong outcome.** Real Amiga boot wants the timeout
path. The Stage E change has been reverted — Paula's
`serial_rx_byte` now defaults to `0` again (matching the
power-on-undefined receive shift register), and the SERDATR check
times out as intended.

### Without Stage E, no Wack — but still stuck

With the revert:
- Wack prologue entries: **0**. KS never reaches Wack.
- Unique PCs visited (5000 frames): 2,315 (Stage D baseline).
- KS oscillates between the COLOR00 delay loop (`$F80450`) and
  the DiagAlive LED-blink area (`$F8363x`). The PCs visited don't
  grow past frame 50 — same wedge shape as Stage D.

After each DiagAlive timeout, `BRA.W $F80DB8` executes
`JSR -30(A6) = exec/Supervisor`. The function passed to Supervisor
(via the calling convention's stack/A5 setup) is presumably a
boot-continuation handler. KS keeps coming back to DiagAlive,
suggesting that Supervisor-called function either fails or returns
to a caller that re-invokes the early-init sequence.

### Two pointers worth knowing

| Chip-RAM | Value | Meaning |
|---|---|---|
| `chip[$00000020]` (vec 8) | `$F83616` | Priv-violation handler (= DiagAlive routine; KS self-installs). |
| `chip[$000014F2]` (ExecBase->DebugEntry) | `$F831F6` | Wack-entry shortcut (any `exec/Debug` call lands here). |
| `chip[$00000004]` (ExecBase) | `$000014B0` | ExecBase struct location. |
| `chip[$0000007C]` (vec 31 / autovec 7) | `$F8325E` | NMI handler (lives in same code region as Wack). |

## A1200 Stage H findings — 2026-05-22

Decoded the `$F80DB8` routine that KS branches to after the
DiagAlive timeout. Stage F's reading ("`JSR -30(A6)` = exec/Supervisor")
was based on bad alignment — the actual disassembly is:

```
$F80DB8: 41F9 0100 0000   LEA.L  $01000000, A0
$F80DBE: 91E8 FFEC        SUBA.L -20(A0), A0     ; A0 -= $00FFFFEC value
$F80DC2: 2068 0004        MOVEA.L 4(A0), A0
$F80DC6: 5588             SUBQ.L #2, A0
$F80DC8: 4E70             RESET
$F80DCA: 4ED0             JMP    (A0)
```

The longword at `$FFFFFFEC` (= ROM offset `$7FFEC` = the last
20 bytes of the ROM) is `$00080000`. So:

```
A0 = $01000000 - $00080000 = $00F80000          (ROM base)
A0 = chip[$F80004]                              (= $F800D2 - reset PC)
A0 -= 2                                          (= $F800D0)
RESET; JMP (A0)
```

This is a **REBOOT TRAMPOLINE**: it computes the ROM base from a
known offset at the ROM tail, derives the reset PC from `$F80004`,
issues `RESET` to clear external chipset state, then jumps to one
byte before the official reset entry point (`$F800D0` — a `4E70`
RESET instruction itself, which then falls through to `$F800D2`).

### KS is in a perpetual reboot loop

Per-tick tracking of `pc == $F800D0 && prev_pc != $F800D0`:

```
prev=$00F80DCE -> $F800D0       (10 entries in 5000 frames)
```

`$F80DCE` is the `JMP (A0)` instruction of the trampoline. So every
DiagAlive timeout cycles: delay loop → reboot trampoline → reset →
KS entry → boot sequence → DiagAlive → timeout → ...

### What triggers the reboot

The reboot path enters at the routine starting at `$F835F2`:

```
$F835F2: LEA   $0400, A7              ; fresh supervisor stack
$F835F6: CLR.L -(A7); CLR.L -(A7)
$F835FA: MOVEM.L D0/D1/A0/A5/A6, -(SP)
$F835FE: ANDI.B #$FE, $00BFE001       ; clear OVL
$F83604: ORI.B  #$03, $00BFE201        ; CIA-A DDRA bits 0+1 output
$F8360C: MOVE.L #$F83616, $0020.W      ; install vec 8 = DiagAlive
$F83614: MOVE.W #$2700, SR             ; set SR
$F83618: MOVEQ  #5, D1                 ; retry counter
$F8361A: MOVE.W #$0174, $DFF032        ; SERPER
... (LED blink + SERDATR check)
```

The validations that route INTO this reboot path are upstream in
the boot self-test routine around `$F83580-$F835BA`:

```
$F83584: 21C0 0000    MOVE.L D0, $0000.W           ; write "HELP" magic
$F83588-$F8358E: ... save D7, A5 ...
$F83590: 2038 0004    MOVE.L $0004, D0             ; D0 = ExecBase
$F83594: 0800 0000    BTST  #0, D0                  ; ExecBase even?
$F83598: 6666         BNE.S $F83600                 ; bad alignment -> reboot
$F8359A: 2C40         MOVEA.L D0, A6                ; A6 = ExecBase
$F8359C: D0AE 0026    ADD.L  $26(A6), D0            ; D0 += ChkBase
$F835A0: 665C         BNE.S $F835FE                 ; ChkBase fail -> reboot
$F835A2: 2A6E 0114    MOVEA.L $114(A6), A5
$F835A6: 208D         MOVE.L A5, (A0)+
$F835A8: 203C F1E2 D3C4   MOVE.L #$F1E2D3C4, D0
$F835AE: 2F00         MOVE.L D0, -(A7)              ; push memory test value
$F835B0: B09F         CMP.L  (A7)+, D0              ; pop and verify
$F835B2: 663E         BNE.S  $F835F2                 ; MEMORY TEST FAIL -> reboot
$F835B4: 4A87         TST.L D7
$F835B6: 6B46         BMI.S $F835FE                  ; D7 negative -> reboot
```

ChkBase validation passes (verified in the test:
`chip[ExecBase+$26] = ~ExecBase`). The other candidates:

- ExecBase odd-aligned (very unlikely — alignment is structural).
- Memory test (`$F1E2D3C4` push/pop) — most likely culprit.
- `D7` sign check.

## A1200 Stage I findings — 2026-05-22

Per-tick capture of CPU state at the four candidate validation
branches:

| PC | Branch | D0 | D7 | Outcome |
|---|---|---|---|---|
| `$F83598` | BNE after `BTST #0, D0` (ExecBase even?) | `$000014B0` | `$8000000B` | OK (bit 0 clear → BNE not taken) |
| `$F835A0` | BNE after `ADD.L ChkBase, D0` | `$000014B0` | `$8000000B` | Branch taken (D0 → $FFFFFFFF ≠ 0) but to GOOD path |
| `$F835B2` | BNE after `CMP.L (A7)+, D0` (memory test) | `$F1E2D3C4` | `$8000000B` | Memory test PASSES |
| `$F835B6` | **BMI after `TST.L D7`** | `$F1E2D3C4` | **`$8000000B`** | **Branch TAKEN → reboot** |

The failing check is the `TST.L D7; BMI $F835FE` at `$F835B4-B6`.

### What `D7 = $8000000B` means

Per-tick tracking of D7's bit-31 acquisitions shows the value is
set at `$F83566` by `BSET #31, D7`. Right before that, `$F83560:
MOVE.L (A7)+, D7` pops `$0000000B` from the stack — that's the
exception vector number 11 (line F) treated as the low byte of an
alert code. Then `BSET #31` adds the "DeadEnd alert" flag.

So `D7 = $8000000B` is the **guru meditation alert code** for "line
F exception" with the deadend bit set.

### The guru-alert routing

The flow is:

1. F-line trap fires (FPU probe at `$F80CA0`).
2. CPU jumps to vec 11 = `$F80AE0` (a `BSR.S $F80B3C` trampoline).
3. The dispatcher at `$F80B3C` decodes the vector number,
   determines this is an unhandled exception, and **branches to
   `$F83558`** (the ColdReboot path).
4. `$F83558` calls `exec.library/ColdReboot` (LVO -726).
5. ColdReboot at `$F80D9E` disables INTENA, calls
   `exec/Supervisor` to run the reset trampoline at `$F80DB8` in
   supervisor mode.
6. `$F80DB8` computes the ROM base, derives the reset PC, executes
   RESET + JMP $F800D0.
7. KS reboots into the boot path, where the alert-recovery routine
   at `$F8355x` pops the alert code and validates it.
8. `TST.L D7` sees bit 31 set → `BMI` taken → branch to `$F835F2`
   (re-init OVL + DiagAlive) → DiagAlive eventually times out →
   `BRA $F80DB8` → reset again.

### Why the F-line trap routes to ColdReboot

On real Amiga, F-line traps for the FPU probe ARE expected to be
benign — KS detects "no FPU" and proceeds. But in our run, the
dispatcher at `$F80B3C` doesn't take the "benign return" path; it
branches to `$F83558` (ColdReboot path). That branch is one of
four `BRA.W $F83558` sites in the dispatcher: `$F80B0A`,
`$F80B38`, `$F80B4E`, `$F80B64`. Each likely corresponds to a
different "fatal exception" classification (bus error, address
error, illegal instruction, line A/F).

Our F-line trap must be matching one of those fatal-classification
branches when on a real Amiga it would take the "ignore + return
with no FPU" path. This is the bug to chase.

### Verification: ColdReboot LVO is installed correctly

- `chip[$11DA]` = `$4EF9 $00F80D9E` (JMP abs.L $F80D9E) ✓
- `chip[$143E]` = `$4EF9 $00F83208` (Debug LVO → calls Supervisor
  with A5 = $F8323A) ✓

So the LVO jump table is good; the bug is in the F-line dispatcher
logic at `$F80B3C`, not in the LVO infrastructure.

## A1200 Stage J findings — 2026-05-22 (landed)

The Stage-I hypothesis was wrong: the F-line trap path itself is
fine. The actual bug was in **RTE**, not exception entry.

**Root cause.** `Cpu68000::TAG_RTE_READ_PC_LO` always advanced SSP
by 6 bytes (SR + PC) and finished, regardless of
`variant_six_word_frame`. 68010+ short Format-$0 frames are
8 bytes (SR + PC + F-V word); 68020 Format-$2 frames are 12 bytes
(adds Instruction Address long above the F-V word). KS 3.1's
F-line dispatcher built a Format-$0 frame manually at `$F80BE0`
and called `(A5)` whose terminating `RTE` was supposed to pop all
8 bytes. Our RTE left SP 2 bytes too low. The trailing `RTS` at
`$F80C0C` then popped an unaligned long whose high word was the
frame's F-V slot ($0020) and low word was the byte past it
($00F8) → PC = `$002000F8` (open bus past 2 MB chip RAM) → vec
11 → fatal alert → reboot → repeat.

**Fix.** Three new follow-up tags in `motorola-68000`:

- `TAG_RTE_READ_FORMAT` — reads the F-V word for 68010+ and
  dispatches on the Format nibble.
- `TAG_RTE_READ_FMT2_HI` / `TAG_RTE_READ_FMT2_LO` — for Format-$2
  frames, pop the 4-byte Instruction Address above the F-V word.

Other formats ($1 throwaway, $9 coprocessor mid-instruction, $A/$B
access fault) currently fall through to "finish" rather than
raising a format-error exception. KS 3.1 boot doesn't generate
them; we'll add proper handling when something does.

**Verification.** Before: 12 vec-11 traps per cycle, perpetual
reboot, 2315 unique PCs visited, IPL never drops. After: 6 vec-11
traps (legitimate probes only), 48 vec-8 priv-violation probes
(first at `$F80BE0`, the protected ColdReboot entry), no wild
jumps, IPL drops to 0 at frame 115, 5538 unique PCs visited,
VPOSR / VHPOSR being polled.

## A1200 Stage K — chase the next alert

KS 3.1 still ends at the alert blinker (`$F80452` writes
`$DFF09A`, then BRA.W to the reboot trampoline at `$F80DB8`). The
alert code popped at `$F83560` differs per cycle:

- **Cycle 1:** `$0000039C` — furthest reach, the cleanest case to
  chase.
- **Cycle 2:** `$00000006` — `AN_MemCorrupt` ("Memory list
  corrupted"). Likely caused by leftover state from cycle 1's
  partial init.
- **Cycle 3+:** `$000003FF` — stabilises here.

### Stage K investigation findings

Cycle 1's failure path traced via diagnostic capture:

1. KS reaches `$F8F7EA` (interrupt-server iteration / exec
   internals)
2. Calls into `$F817C2` (loop with `JSR (A5)`)
3. Calls `chip[$148C]` = `LVO -36` = `exec.ExitIntr`
4. `$F8137E` (ExitIntr body): pops A6, BTSTs, then `MOVEM (A7)+`
   for 24 bytes + `RTE` at `$F81398`
5. `RTE` at `$F81398` pops 8 bytes (our Stage J fix) from a
   stack that only has 4 bytes of legitimate data
6. Popped PC = `$00F8FFFF` (odd — high word from a JSR return PC,
   low word from open-bus past chip RAM)
7. Address error fires (vec 3 = chip[$C] = `$F80B0E`)
8. Alert via dispatcher → `$F80452` blinker → reboot trampoline

The popped SR in the AE frame shows `$0018` because
`regs.sr = (popped_sr & $F71F)` where `popped_sr = $00F8` (high
word of a JSR return PC interpreted as SR). `$00F8 & $F71F =
$0018` — just X and N flags, no S bit, so the AE handler thinks
it was a user-mode trap and routes through the BEQ-taken branch
($F80BA8 → SHORTER pseudo-frame routine), accelerating the chaos.

### Root cause — frame-format mismatch chain

`exec.ExitIntr` is designed to be the EXIT of an interrupt
handler. Real entry: CPU pushes 8-byte Format-0 frame, handler
saves 24 bytes of registers via MOVEM, runs, JMPs to ExitIntr.
ExitIntr: pop A6 (left by the interrupt entry stub), MOVEM pop
24 bytes, RTE pops 8 bytes. Net stack delta: 0.

What our test sees: ExitIntr called via `JSR -36(A6)`. JSR
pushed 4 bytes (return PC). ExitIntr's `POP A6` pops the JSR
return PC into A6 (wrong). MOVEM pops 24 bytes from user-stack
data (garbage). RTE pops 8 bytes (more garbage). PC = odd.

This *might* be:

- KS calling ExitIntr via the wrong path (KS bug — would also
  fail on real 68020)
- Our 68020 corrupting some earlier state that led KS here
- Cycle 1's user-mode probe sequence creating a context where
  this is reached

The Stage J fix is correct (real 68020 pops 8 bytes on RTE).
Reverting it brings back the wild-jump-to-$002000F8 bug. Neither
fix alone solves both — the underlying chain needs frame-format
parity at a higher level.

### Stage K landed

68020+ now pushes a 28-byte Format-$A short bus-fault frame for
group-0 exceptions (vec 2, vec 3), replacing the 14-byte 68000
frame. RTE now pops the F/V word and dispatches on the Format
nibble — Format $0 (8 bytes), Format $2 (12 bytes), Format $A
(28 bytes). Implemented in `motorola-68000` behind a new
`variant_format_a_group0` flag enabled in the 68020 wrapper;
new follow-up tags: `TAG_AE_FMT_A_STEP` for the 11-step push,
`TAG_RTE_READ_FMTA_STEP` for the 10-step tail pop.

**Result:** KS 3.1 alert code went from chaotic `$03FF` →
specific `$80000003` = `AN_ExcptVect` ("exception vector check
failed"). The frame format is now agreeing with KS; the
remaining bug is at a higher level — the stack leak from
KS's SHORTER pseudo-frame path. See Stage L.

## A1200 Stage L — chase the AN_ExcptVect alert

Cycle 1 now alerts with `$80000003` (`AN_ExcptVect`). KS treats
any unexpected bus/address error as fatal exception-vector
corruption. The AE itself originates at `$F81398` (RTE in
`exec.ExitIntr`) popping a slightly-misaligned PC. The
misalignment comes from KS's SHORTER pseudo-frame routine at
`$F80BC0` (PEA + MOVE.W SR + JMP A5): it pushes 6 bytes, but
our 68020 RTE pops 8 (Format $0 spec). Each call leaks 2
bytes; multiple priv-viol probes per boot cycle accumulate
the leak until SSP overflows past `$00200000`.

Why does KS take the SHORTER path? The dispatcher BEQ at
`$F80B4C` tests bit 5 of the saved-SR byte in the AE frame.
We get here because the AE fires *during* one of KS's
user-mode priv-viol probes — saved SR has S=0 → KS routes to
SHORTER (user-mode AE recovery). On real 68020 the SHORTER
routine should still leak the same 2 bytes — so either real
KS doesn't actually reach this path on 68020, or some
CPU-detection bit is wrong in our setup that makes KS take a
68000-style recovery branch.

Investigation order for Stage L:

1. **Track A5's exact value at SHORTER-routine entry.** The
   `JMP (A5)` callback may end with `RTS` (4-byte pop) on real
   68020 instead of `RTE` (8-byte pop). If our A5 points to a
   bad callback, that explains the leak.
2. **Trace the cycle-1 chain that pushes `$0003` as alert
   code.** The popped value at `$F83560` is now `$00000003` —
   consistent with the AE pushing a frame and KS treating the
   vector number as alert code via the standard dispatcher.
3. **Consider whether the SHORTER routine is genuinely a
   68000-only path that KS skips on 68020.** If so, identify
   the CPU-detection bit our 68020 is failing.

Stage L is a deep-cut workstream and may take several days. The
Stage J and Stage K wins are real and standalone — the rest of
the Amiga family rollout (A600, CDTV, A4000-030, etc.) can
proceed in parallel using the Stage-J/K-fixed CPU.

## A1200 Stage L findings — 2026-05-24 (landed)

The Stage L hypothesis (SHORTER pseudo-frame at `$F80BC0` leaks
2 bytes per call) was **falsified**. Instrumentation in
`ks31_boot.rs` showed `$F80BC0` is reached only 2× across the
entire boot, and both times the alert chain has already started
(top of stack = alert dispatcher `$F83558`). `$F80BC0` is part
of the alert/blinker path, not the leak source.

**Actual root cause.** `Cpu68000::initiate_interrupt_exception`
always pushed a 6-byte frame (PC + SR) regardless of
`variant_six_word_frame`. Compare with
`begin_group1_exception` which correctly consults the flag and
pushes an 8-byte Format-$0 frame on 68010+. Every hardware
interrupt on our 68020 pushed 6 bytes, but RTE (post-Stage-J)
correctly pops 8 bytes for Format $0 — **net leak: 2 bytes per
interrupt**. With ~12 interrupts per boot cycle, that matches
the ~24-byte SSP overflow past chip-RAM top.

**Diagnostic that nailed it.** Per-RTE-at-`$F81398` (the RTE in
`exec.ExitIntr`) captures of pre-pop SSP across one boot cycle
showed every consecutive pair drifted by exactly 6 bytes
(`delta_from_prev_post=$-6`) — a 6-byte push pattern between
each 8-byte RTE pop. Six bytes is the 68000-style group-1
exception frame size; the missing 2 bytes is the F/V word the
68010+ frame is supposed to carry.

**Fix.** `motorola-68000/src/cpu.rs::initiate_interrupt_exception`
now branches on `variant_six_word_frame`. On 68010+ it pushes
the F/V word first (vector = 24 + level for autovectored
interrupts, which is what all retro 68010+ Amiga / Atari Falcon
targets use), then chains through `TAG_EXC_STACK_FORMAT` for
the PC + SR push — mirroring `begin_group1_exception`. The 68000
path is unchanged.

**Genuinely-vectored 68010+ systems** (Mac via VIA/SCC, some
VME) would need an IACK-first refactor before the F/V push so
the pre-pushed vector matches the external vector. No current
target uses vectored interrupts; deferred until one does.

**Result.** A1200 boot progression jumped from 5,539 unique PCs
visited (alert at `$F8044E` blinker) to **22,880 unique PCs**
visited — boot now reaches resident-module init at `$F96xxx`.
SSP no longer drifts past chip-RAM top. Tom Harte 68000/68010/
68020 suites + all Amiga machine tests green.

The new alert (`D7 = $80000004`, popped at `$F96424` /
`$F958E0` / `$F96856`) is a separate problem deep in
resident-module init — Stage M territory.

## A1200 Stage M findings — 2026-05-24 (in flight)

KS 3.1 was tripping vector 4 (illegal instruction) at
`$F85298` on a `BFEXTU (A1), …` opcode. Our 68020 implemented
the bit-field family only for Dn operands; memory EAs traced
through to a Phase 5 deferral and raised illegal-instruction.

A ROM scan turned up **150+ bit-field instruction sites** in
the KS 3.1 image across all 7 memory EA modes — heavily
concentrated in `graphics.library` and `intuition.library`
(BFINS alone has 44 `(An)` sites and 21 `(d8,An,Xn)` sites).
Implementing the full family is genuinely Phase 5 work; the
project comment at `motorola-68020/src/cpu.rs:13` already
listed bit-field as deferred. Stage M lands the foundation.

**Architecture.** A multi-step memory pipeline drives bit-field
reads, math, and (for R-M-W ops) writeback through the same
queue-driven micro-op model the rest of the CPU uses:

1. `execute_bf` (68020 hook) detects `ea_mode != 0` and hands
   off to `begin_bf_memory`, which decodes the BF extension
   word, resolves the EA, snapshots Dr / signed offset for the
   ops that need it, stashes the pipeline state on `Cpu68000`
   scratch fields (`bf_buf`, `bf_base_addr`, `bf_sub_op`,
   `bf_dr`, `bf_width`, `bf_bit_offset`, `bf_bytes_total`,
   `bf_bytes_done`, `bf_source_val`), and queues the first
   `ReadByte`.
2. `TAG_BF_MEM_READ` chains byte reads, packing them MSB-first
   into the 64-bit `bf_buf`.
3. `TAG_BF_MEM_EXEC` does the field math against the fully
   assembled buffer. BFTST / BFEXTU / BFEXTS / BFFFO finish
   here; BFCHG / BFCLR / BFSET / BFINS modify `bf_buf` and
   hand off to the writeback chain.
4. `TAG_BF_MEM_WRITE` chains byte writes back to the same
   address span, then leaves the queue empty so the CPU's
   "start next instruction" path auto-pushes `PromoteIRC`.

A new `continue_68020_opcode` continuation hook dispatches the
three BF tags and falls through to `continue_68010_opcode` for
anything it doesn't claim (notably `TAG_RTD_*`).

**Scope landed this iteration.** EA modes `(An)`, `(An)+`,
`-(An)` for **all 8 BF ops**. The post-increment / pre-
decrement modes step An by one byte per M68000PRM § 4.3.5, in
line with how 68020 hardware treats bit-field operands. Modes
needing extension words (`d16(An)`, `(d8,An,Xn)`, AbsShort,
AbsLong, PcDisp, PcIndex) still trap illegal — Stage M
follow-up.

**Result.** A1200 KS 3.1 boot:

| Metric                  | Pre-Stage-L | Pre-Stage-M | After Stage M (An) |
|-------------------------|------------:|------------:|-------------------:|
| Unique PCs visited      |       5,539 |      22,880 |             29,704 |
| Last PC in ROM          |   `$F8044E` |   `$F83626` |          `$FC1634` |
| Vec 4 (illegal) traps   |           0 |           6 |                  0 |
| Vec 8 (priv-viol) probes|          48 |         239 |              2,190 |
| INTENA writes           |         346 |       7,579 |            332,377 |
| Custom-reg writes total |     475,434 |     455,970 |            421,242 |

Boot now reaches resident-module init at `$FC1xxx` (well past
the early DiagAlive area at `$F83xxx`), drops IPL all the way
to 0, runs heavy interrupt-driven activity (45× more interrupt
enable writes than before), polls VHPOSR / VPOSR for beam
synchronisation, and writes copper-list pointers + sprite
pointers ~5K times each — graphics setup is genuinely live.

Boot does NOT reach STRAP within 4000 frames — module init is
still working through later resident modules. Whether the
remaining gap is more BF EA modes, other missing 68020
instructions (CHK2, CAS, MULL/DIVL on memory, …), or a
hardware-side issue (Paula audio, Gayle PCMCIA quirks,
graphics.library expectations) is the Stage M follow-up
investigation.

**Tom Harte 68000 / 68010 / 68020 + all Amiga machine tests
green** — no regressions from the new pipeline.

### Stage M follow-ups landed — 2026-05-24

All six extension-word EA modes followed in a single follow-up
commit:

- **`d16(An)`** — sign-extended 16-bit displacement after An
- **`(d8,An,Xn)`** — brief extension word, scale honoured under
  `variant_scaled_index`
- **AbsShort** — sign-extended 16-bit absolute
- **AbsLong** — two ext words, staged via `TAG_BF_MEM_EA_ABSLONG_LO`
- **PcDisp** — PC-at-extension + d16
- **PcIndex** — PC-at-extension + brief ext

KS 3.1 boot still doesn't exercise any of the new modes (vec 4
illegal-instruction count remains 0), so the BF family is now
complete from an instruction-completeness standpoint.

## A1200 Stage N findings — 2026-05-25

KS 3.1 boot reaches `$FC1xxx` / `$F84xxx` and steady-states
there. A series of diagnostics pin down the situation:

**IRQ delivery is healthy.** The first diagnostic round reported
"0 autovec IRQs taken" via `exc_counts`, which led to a long
hypothesis chain about mask-correlation and self-blocking loops.
That report was wrong: `initiate_interrupt_exception` intentionally
leaves `exc_vector` unset (so the shared follow-up tag chain can
distinguish interrupts from group-1/2 exceptions), so the test's
exc_vector-based counter missed every IRQ. A dedicated counter
(`Cpu68000::interrupts_taken`) reveals **89,057 IRQs taken in 30
000 frames** (~3K/sec, expected rate for VBL + CIA + Paula).

**Paula → CPU IPL path is healthy.** IPL pin = 3 for 7.49% of
ticks (42M out of 566M). INTENA peak `$602C` has SOFT, PORTS,
VERTB, EXTER, master enable bits set — KS configured the IRQ
sources as expected.

**The remaining gap is a polling loop, not a CPU issue.** With
IRQs firing at full rate, the boot reaches only 19 new unique
PCs between the 4K-frame and 30K-frame runs. Same hot PCs
(`$F84E98`, `$FC15A8`, `$FC1630`, etc.) dominate both runs.
PCs cycle among `$F84xxx`, `$FC1xxx`, `$FC51xx` — all in
timer.device, scsi.device, or similar resident-module code.

The boot is waiting for some chipset/peripheral condition our
emulation isn't providing. Likely candidates (Stage O):

- **trackdisk.device floppy poll** — KS may be waiting for a
  specific drive-status transition before completing init or
  routing to the STRAP "insert disk" screen.
- **timer.device EClock comparison** — if our EClock rate is
  wrong, KS's timer waits could be off.
- **scsi.device IDE/Gayle probe** — the Gayle PCMCIA / IDE
  surface may be returning unexpected values.
- **keyboard.device handshake** — input.device init might be
  waiting for a keyboard handshake completion.

**Stage L / M / M-2..M-5 / N together** took the A1200 boot
from `$F8044E` (alert blinker, 5,539 PCs) to deep module init
(29,723 unique PCs at the 30K-frame steady state) — a 5.4× PC
coverage expansion. Three CPU-completeness gaps closed:

- 68010+ interrupt frame F/V word (Stage L)
- 68020+ bit-field memory operands (Stage M / M-2..M-5)
- Test-only IRQ-counter visibility (Stage N)

Tom Harte 68000 / 68010 / 68020 + all Amiga machine tests stay
green throughout.

### Diagnostic surface left in `ks31_boot.rs`

The test now reports, per run:

- Direct `interrupts_taken` counter (CPU-side, unconditional)
- Per-vector autovec counts (for exception-style IRQs)
- Paula INTENA peak + decoded bit names + last 10 INTENA writes
  that changed the register
- Paula → CPU IPL pin distribution (per level + max seen)
- CPU mask register distribution (per level)
- "Ticks where IPL pin > mask" + "Instruction boundaries with
  IPL > mask" tallies (note: the latter is tautological — the
  counter only increments at PromoteIRC times when no IRQ fires,
  so it always reads 0 if any IRQ ever fires)
- First 40 mask RAISES (tick / instr_start_pc / old / new)

Plus all the Stage J / K / L / M diagnostics that were already
in place. Total runtime at 4000 frames: ~23s.

## A1200 Stage Q — MCP debugging surface — 2026-05-25

Built an `--mcp` JSON-RPC stdio mode for `emu198x-amiga` so the
KS-internals investigation can be driven interactively without
recompiling. Eighteen tools:

- **Control:** `run_frames`, `run_ticks`, `run_until_pc`,
  `run_until_any_pc`, `run_until_mem_change`, `step`, `reset`.
- **CPU + chip queries:** `query_cpu` (regs + IRQ state +
  `interrupts_taken` + `instruction_starts`), `query_chipset`
  (BPLCON0 / DMACON / ADKCON / COLOR00 / COP1LC / copper PC /
  overlay), `query_paula` (INTENA / INTREQ with bit-name decode),
  `query_cia` (both CIAs, ICR / ports / TOD / halted),
  `query_agnus` (vpos / hpos / bpl_pt / blitter pointers),
  `query_blitter`, `query_copper_list` (MOVE / WAIT / SKIP decoded
  from a copper list at any address).
- **Memory:** `memory_read`, `memory_read_long`, `query_stack`
  (longs off SSP/USP), `disasm` (m68k disassembly).

ROM lookup mirrors `ks31_boot.rs` (`$EMU198X_KS31_A1200_ROM` env
var → `~/.emu198x/roms/commodore-amiga/kick31a1200.rom`). Integration
test (`tests/mcp_smoke.rs`) drives the same `Server::handle` path
the binary uses, asserting the tool registry and one end-to-end
boot. Skips loudly if the ROM isn't available.

### Stage Q findings that change the picture

Driving the MCP for a few hundred KS 3.1 frames produced a result
worth booking up-front: **the system is no longer wedged after
Stages L / M / N.** PC advances across wide ROM regions and
drops to user mode (supervisor = false). At successive snapshots:

| Frame ~ | PC          | Mode  | A6      | Notes                              |
|---------|-------------|-------|---------|------------------------------------|
|     600 | `$F847E8`   | super | `$14B0` | Mid-instruction, `instr_start_pc`=`$F847E2`, in_followup=true |
|     800 | `$F808FE`   | user  | `$14B0` | Different ROM region; SSP at top of chip ($80000) |
|    1000 | `$FC1102`   | user  | `$6AE4` | Yet another module, different A6 (different library base) |

`interrupts_taken` advances ~600 per 200 frames (vblank + a CIA
timer source), `ipl_pin` reads as 3 at most snapshots — IRQs are
firing and being accepted as expected.

### What we now think is happening

The OS is past STRAP and into the "Insert Workbench" idle state.
Three bitplane pointers are programmed in Agnus (`bpl_pt =
[$12666, $14E0C, $175B2, 0, 0, 0, 0, 0]`) but **BPLCON0 reads
`$0302` — BPU=0**. The copper list at `$C00` writes `$8302`
(HIRES + BPU=0) every frame. So the screen is "valid but empty":
DMA is enabled, copper is iterating, blitter is idle, but no
bitplanes are being displayed because `BPLCON0[14:12] = 0`.

Two readings of this are still on the table:

1. **The OS hasn't enabled bitplanes yet** because some other
   guard condition (intuition.library init, disk-prompt animation
   trigger, etc.) hasn't been satisfied.
2. **Bitplane enable is being deferred** because there's no disk
   in the drive — and the "Insert Workbench" splash itself does
   end up showing bitplanes once `trackdisk.device` reports a
   drive ready with no medium.

The next investigation step (Stage R) should attach an ADF to the
drive via the MCP (need to expose `insert_adf` and `eject_disk` as
tools), or alternatively force the drive to report DRIVE_READY +
NO_DISK without a backing file, and see whether BPU goes non-zero.

Closed during Stage Q:

- The "interrupt mask stays at 7 forever" suspicion: with the new
  `query_cpu` data, `interrupt_mask = 0` is observed at every
  user-mode snapshot. IRQs are accepted whenever the CPU reaches
  an instruction boundary.
- The "STRAP wedge" framing: the system isn't wedged at all. The
  loops Stage N pinned in `$F84xxx` / `$FC1xxx` are Exec
  dispatching tasks, not the CPU spinning on a flag.

The MCP itself is the durable artifact — every subsequent Amiga
investigation should reach for it first instead of recompiling
new instrumentation into `ks31_boot.rs`.

## A1200 Stage R — disk-attached probe, "wedge" reframed — 2026-05-25

Stage R added `insert_media` / `eject_media` / `query_disk` MCP
tools (ADF for now; HDF/IPF reserved) and used them to probe
whether bitplane enable was gated on a present disk medium.

### What happened with WB 2.04 inserted at frame 900

| Signal              | Before insert | After insert + 2300 frames |
|---------------------|---------------|----------------------------|
| BPLCON0             | `$0302`       | `$0302` (briefly `$8302`)  |
| BPU bits            | 0             | 0                          |
| DMACON              | `$03D0`       | `$03F0` (BPLEN re-enabled) |
| Drive cylinder      | 3             | 1 (after stepping to 55)   |
| Drive motor         | off           | off (parked after read)    |
| `disk_change_low`   | true          | false (acknowledged)       |
| `bpl_pt[0..3]`      | `$12666, $14E0C, $175B2` | `$2A476, $2F476, $175B2` |
| Bitplane $2A476     | (untouched)   | `FF FF 70 00 00 00 ...`     |
| Sprite 0 data       | (cleared)     | populated control + pixels |
| Copper $C58–$C5C    | absent        | `MOVE $0120=$0001 / $0122=$4454` (SPR0PT=$00014454) |

The drive stepped from cylinder 3 to cylinder 55 and back, motor
spun up and parked — **trackdisk.device, the MFM decoder, Paula
disk DMA, and the CIA-A floppy control path all work end-to-end.**
KS recognised the disk insertion, read it, and then re-idled the
drive.

Bitplane memory at `$2A476` contains real rendered content
(`FF FF 70 00` then zeros — classic glyph→background transition,
not garbage). Sprite memory at `$14454` is populated with control
words + pixel data — almost certainly the OS mouse pointer.
Everything that needs to be in chip RAM for a visible display
*is* in chip RAM.

### The reframe

The original "STRAP wedge" / "Stage P render-loop blockage"
framing was wrong. The emulator runs KS 3.1 end-to-end:

1. CPU streams instructions across `$F8xxxx` / `$FCxxxx`
2. Exec dispatches tasks (supervisor ↔ user transitions observed)
3. 89K + IRQs taken (vblank + CIA timer + Paula)
4. trackdisk.device reads the inserted disk (head to cyl 55)
5. Bitplanes are rendered into chip RAM
6. Sprite pointer is loaded with real data
7. The copper list is iterating

The one remaining gap is narrow: **BPU bits in BPLCON0 never go
non-zero**. The copper writes `$8302` (HIRES, BPU=0), the CPU
writes `$0302` (no HIRES, BPU=0), and nobody writes a value with
BPU ≥ 1. So the chipset has been told "draw a display with zero
bitplanes" — the screen ends up as background colour only, despite
the rendered content sitting at the pointed-to addresses.

This is no longer an emulation problem in the broad sense. It is
either:

- **WB 2.04 incompatibility with KS 3.1** — KS 3.1 may need
  WB 3.0/3.1 disks (3.1.4 specifically expects DOS 3.0+). The
  boot block parse may be rejecting WB 2.04, and the "this isn't
  a valid Workbench disk" rendering uses a path we don't hit.
- **A final-mile init step** that KS 3.1 wants and our emulator
  doesn't deliver — possibly something around `intuition.library`
  screen-open, or a graphics-library `LoadView()` call that's
  waiting for a condition (vsync alignment?).

### Stage S candidates

- **Obtain a WB 3.0 or 3.1 ADF.** That's the obvious next move —
  WB 2.04 isn't the right disk for KS 3.1.
- **Stub a "no Workbench disk" boot path.** Force the OS into its
  "please insert workbench" rendering by deliberately failing the
  boot block validation, and see whether that path enables BPU.
  Falsifies the medium-rejection hypothesis cleanly.
- **Capture BPLCON0 writes over a long boot.** Add an MCP tool
  that records every write to `$DFF100` with PC + value, so we
  can see whether KS ever *intends* to set BPU and is being
  pre-empted before it gets there.
- **Compare with vAmiga / fs-uae** on the same ROM + WB 2.04 ADF
  to see how *they* handle this combination.

### Tools added in Stage R

- `insert_media` (`path` + `kind=adf` + `change_pending`)
- `eject_media`
- `query_disk` — cylinder / head / motor / selected / status bits
  (disk_change, write_protect, track0, ready — all decoded
  active-low per the hardware)

## A1200 Stage S — WB 3.1 + the real diagnosis — 2026-05-25

Two amendments to Stage R, both important:

### 1. `insert_media` now reads from .zip

`insert_media` looks at the path extension. If `.zip`, it opens
the archive and either picks a single `.adf` automatically or
takes an explicit `entry` argument when there's more than one.
The response gains a `source` field that surfaces `path#entry`
so it's never ambiguous what was loaded. Lets the MCP point
directly at the TOSEC-style `Workbench v3.1 ... (Disk 1 of
6)(Install).zip` archives without manual unzip.

### 2. The "PC parked at $F81476" reading was wrong

Disassembling `$F81460..$F81490` revealed what the CPU is
actually doing at the steady-state PC:

```
$F81468: addq.l #1,(280,A6)   ; ++ExecBase.IdleCount
$F8146C: bset   #7,(292,A6)   ; ExecBase flag — "currently idle"
$F81472: STOP   #$2000        ; halt until IRQ (mask 0)
$F81476: bra.s  $F8145E       ; back to scheduler check
```

`$4E72 2000` is the m68k **STOP** instruction with immediate
`$2000` (supervisor, IRQ-mask 0). Our disassembler doesn't
decode STOP and printed it as `dc.w $4E72`, which is what threw
the earlier reading off.

So PC = `$F81476` doesn't mean "wedge". It means **the CPU is in
Exec's idle loop**, waking on every IRQ, checking whether a task
is ready, finding none, and sleeping again. That's the correct
behaviour of an idle Amiga. The reason we always observe PC
`$F81476` is that's the instruction the CPU was *about* to
execute when STOP got cancelled by the IRQ — i.e. it's the
single instruction the scheduler spends the most time at.

### 3. So what's actually missing

With either WB 2.04 *or* WB 3.1 Install inserted, the trajectory
is identical:

- The disk is recognised (`disk_change_low` goes false)
- The drive steps to cyl 40–55 and back to ~cyl 1 (boot block +
  file-system probe — KS got further than just the boot block)
- The drive is parked (`motor_on=false`, `selected=false`)
- Bitplane pointers are programmed
- Bitplane chip RAM is populated with real rendered data
- A sprite pointer is programmed with real sprite data
- The copper is iterating
- The CPU is idle in Exec's STOP loop

What is *not* happening:

- No task ever runs that enables BPLCON0 BPU bits
- intuition's `OpenScreen` for the Workbench screen never fires
- The boot continuation that would bring up the display
  workflow never gets queued

That means the gap isn't in the CPU, the chipset, the floppy, or
the MFM decode (those all work). It's somewhere in the disk-boot
hand-off: KS reads the boot block + probes the file system,
something rejects the disk silently, and the OS falls back to
"keep idling because there's nothing to do." Comparing with
vAmiga / fs-uae on the same ROM + same ADF is the most efficient
next move — if they boot to Workbench and we idle, the gap is
ours; if they idle too, the disk image is the problem.

### Tools added in Stage S

- `.zip` support inside `insert_media` (auto-detect single `.adf`,
  `entry:` to disambiguate when multiple, `source:` field in
  response reports `path#entry`)

## What this plan does not cover

- **PMOVE / PFLUSH / PTEST** (68030 + 68040 MMU instructions). Tracked in
  [`m68k-test-oracle-strategy.md`](m68k-test-oracle-strategy.md). Will
  surface as a Stage C failure on A4000-030 or A3000.
- **FPU FLINE handling** for 68040. The 705-line `motorola-68040/src/fpu.rs`
  exists but isn't wired through `variant_decode_hook`. Will surface as a
  Stage C failure on A4000-040 if KS 3.1 probes for the FPU.
- **WinUAE second-oracle** ([`m68k-test-oracle-strategy.md`](m68k-test-oracle-strategy.md)
  Mitigation B). Becomes timely once A1200 boots — extracting WinUAE's
  CPU as a callable library is its own project.
- **RTG framebuffer surface.** Captured by Seam 3 of the architecture
  review; lands when Vampire or Picasso-class cards become a real target.

## Drift triggers

- **A new Amiga variant appears** that doesn't fit the chipset / CPU
  axes above. PiStorm is the obvious candidate (68000 emulated on Pi
  silicon + real Amiga hardware) — it should land its own track here.
- **A chip turns out to be heavier than the donor estimate.** Gayle's
  donor crate is 2334 lines but most is NE2000; if the IDE / PCMCIA
  side surfaces unexpected complexity at Stage A, that's a sequencing
  signal to re-cost.
- **Akiko ends up gating CD32 longer than expected.** The chunky-to-planar
  path is fundamental; if it's not portable from donor / WinUAE, CD32
  may slip behind A3000.

## Related

- [Amiga full-family architecture review](amiga-full-family-architecture-review.md) — the architectural seams this plan sequences through.
- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md) — the wrap-don't-clone pattern that makes CPU swaps mechanical.
- [Motorola 68k test-oracle strategy](m68k-test-oracle-strategy.md) — the verification ladder, with Mitigation B gated on the first real 68020 machine (i.e., A1200 Stage C).

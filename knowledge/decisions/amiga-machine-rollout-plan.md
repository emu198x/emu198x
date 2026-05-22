# Decision: Amiga machine rollout plan

**Date:** 2026-05-22
**Status:** A1200 Stages A–I landed 2026-05-22. Stage I identified
the failing validation: `TST.L D7; BMI $F835FE` at `$F835B4`. D7
holds the value `$8000000B` — bit 31 = "DeadEnd alert" flag, low
byte = `11` = exception vector number (line F). KS is in a guru-
alert loop: F-line trap fires → dispatcher routes to the alert
handler at `$F83558` → ColdReboot → KS reboots into the alert-
recovery boot-self-test → TST.L D7 sees the alert code → fails →
reboots again. Stage J needs to fix the F-line trap handling so
it doesn't trigger a guru-alert.

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

## A1200 Stage J — what to investigate next

1. **Disassemble the full dispatcher at `$F80B3C`** to identify
   the decision tree for "fatal vs benign" exception handling.
   Find which branch our F-line trap falls into and why.
2. **Compare the dispatcher's branch evaluation with KS's
   expected F-line behaviour.** The 68k FPU probe protocol is:
   F-line trap fires → handler sets D1 to a flag indicating "no
   FPU" → RTE. If our trap fires but D1 isn't being set correctly
   before reaching the dispatcher, KS treats the trap as fatal.
3. **Check whether our Cpu68020's F-line trap behaviour matches
   real 68020.** Specifically: when the F-line opcode has
   cpID=1 (FPU coprocessor) but no FPU is present, the trap is
   *normal* and the handler should be able to detect the
   coprocessor's absence. If our trap sets the exception frame
   differently from a real 68020, the dispatcher misclassifies.
4. **Compare against WinUAE booting the same ROM.** Trace the
   first F-line trap and see what value D1 holds before the
   dispatcher decides.

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

# KS 1.3 Trackdisk Bootblock Path

Saved notes for the exact Kickstart 1.3 / `trackdisk.device` ranges involved in
the bootblock read. This is the path we keep re-disassembling while chasing the
Workbench 1.3 boot stall.

For the broader ROM flow see [ks-1.3.md](ks-1.3.md). For common debugging
patterns see [debugging-guide.md](debugging-guide.md).

## Why this note exists

The current WB 1.3 stall is not "no floppy activity". STRAP opens
`trackdisk.device`, issues `CMD_READ`, gets a successful return, sees `DOS\0`,
and then rejects the 1024-byte bootblock because only sector 0 has been
decoded into the destination buffer on the second successful attempt.

The recurring questions are:

- What exact `IOStdReq` shape does STRAP submit at `$FE859C`?
- What does the `trackdisk.device` READ loop at `$FEA552` compare against when
  it exits at `$FEA57E`?
- Which validation branches correspond to the later `$1B` failures?

This note and the in-tree diagnostics keep those answers in one place.

## Reusable tools

- Focused inserted-disk trace:
  [wb13_cmd_read_trace.rs](/crates/machine-commodore-amiga-ocs/tests/wb13_cmd_read_trace.rs:1)
- ROM disassembly helper:
  [ks13_disasm_bootblock.rs](/crates/machine-commodore-amiga-ocs/tests/ks13_disasm_bootblock.rs:1)
- Broad runtime picture:
  [diag_wb13_boot_state.rs](/crates/runtime-commodore-amiga/tests/diag_wb13_boot_state.rs:1)

Run them with:

```sh
cargo test -p machine-commodore-amiga-ocs trace_wb13_cmd_read_request_and_loop_state -- --ignored --nocapture
cargo test -p machine-commodore-amiga-ocs trace_wb13_later_read_block_writers -- --ignored --nocapture
cargo test -p machine-commodore-amiga-ocs trace_wb13_later_request_origin_context -- --ignored --nocapture
cargo test -p machine-commodore-amiga-ocs dump_ks13_trackdisk_bootblock_regions -- --ignored --nocapture
cargo test -p runtime-commodore-amiga wb13_boot_state_checkpoints -- --nocapture
```

## `IOStdReq` offsets used by the trace

These are the standard Amiga `IOStdReq` offsets relative to the request base:

| Offset | Field |
|---|---|
| `+$14` | `io_Device` |
| `+$18` | `io_Unit` |
| `+$1C` | `io_Command` |
| `+$1E` | `io_Flags` / `io_Error` |
| `+$20` | `io_Actual` |
| `+$24` | `io_Length` |
| `+$28` | `io_Data` |
| `+$2C` | `io_Offset` |
| `+$30` | `io_HighOffset` |

Those are the fields the inserted-disk trace dumps at STRAP's `CMD_READ` call
site (`$FE859C`) and just after the synchronous `DoIO` returns (`$FE85A0`).

## STRAP bootblock-read path

The ROM-level expectation is still the simple one already captured in
[ks-1.3.md](ks-1.3.md#disk-detection-loop):

1. `CMD_READ` 1024 bytes from offset 0
2. verify `DOS\0`
3. verify the one’s-complement bootblock checksum
4. jump to boot code at `12(A4)`

The focused `trap_strap_branch` diagnostic already shows the no-disk path uses
two `CMD_READ` attempts that both return `D0=0`, then retries based on content
rather than `io_Error`.

For the inserted Workbench case, the missing state is the live `IOStdReq`
contents at the exact `CMD_READ` site. That is what
`trace_wb13_cmd_read_request_and_loop_state` now records.

Saved disassembly for the setup sequence:

```asm
$FE857E  lea     (44,A5),A1              ; IOStdReq at A5+$2C
$FE8582  move.w  #$0002,(28,A1)          ; CMD_READ
$FE8588  move.l  #$00000400,(36,A1)      ; io_Length = 1024 bytes
$FE8590  move.l  A4,(40,A1)              ; io_Data = bootblock buffer
$FE8594  move.l  #$00000000,(44,A1)      ; io_Offset = 0
$FE859C  jsr     (-456,A6)               ; DoIO(request)
$FE85A0  tst.l   D0
$FE85A4  move.l  (A4),D0                 ; check for 'DOS\\0'
$FE85AE  move.w  #$00FF,D1               ; 256 longs = 1024 bytes
$FE85B4  add.l   (A0)+,D0                ; bootblock checksum loop
$FE85C6  jsr     (12,A4)                 ; execute boot code on success
```

The first live run of the new trace already confirms the request shape at
`$FE859C`: `CMD_READ`, `io_Length=$400`, `io_Offset=0`. That makes the
"STRAP intentionally retried with one sector" theory much weaker.

## `trackdisk.device` READ loop

The ranges to keep in mind are:

| Range | Meaning |
|---|---|
| `$FEA484..$FEA5D8` | READ validate / extract loop body |
| `$FEA552` | READ extract loop head |
| `$FEA57E` | READ loop exit / compare point |
| `$FEA996` | READ-side blit setup (`BLTCON0=$1DD8`) |
| `$FEACFA` | validation branch: header checksum mismatch |
| `$FEAD10` | validation branch: format byte != `$FF` |
| `$FEAD1C` | validation branch: track byte mismatch |

Saved disassembly for the commit/continue path:

```asm
$FEA544  lea     (64,A4),A1              ; decode scratch / source ptr
$FEA548  movea.l (86,A3),A0              ; current destination pointer
$FEA54C  move.l  #$00000200,D0           ; 512-byte sector
$FEA552  bsr.w   $FEA932                 ; decode/copy one sector
$FEA556  move.l  #$00000200,D1
$FEA55C  add.l   D1,(86,A3)              ; advance destination pointer
$FEA560  move.l  (32,A2),D0
$FEA564  add.l   D1,D0
$FEA566  move.l  D0,(32,A2)              ; advance running byte count
$FEA57A  cmp.l   (36,A2),D0
$FEA57E  bcc.s   $FEA5A0                 ; stop when count >= limit
$FEA580  movea.l (78,A3),A2
$FEA584  addq.b  #1,(73,A3)              ; next sector index
$FEA588  cmpi.b  #$0B,(73,A3)
$FEA58E  blt.w   $FEA484                 ; continue validation loop
$FEA592  move.b  #$00,(73,A3)
$FEA598  addq.w  #1,(74,A3)              ; advance track-side state
$FEA59C  bra.w   $FEA438
$FEA5A0  movea.l (68,A3),A1
$FEA5A4  bsr.w   $FE9E30
$FEA5B6  movem.l (A7)+,A2/A3/A4
$FEA5BA  rts
```

Two annotations matter here:

- `A3+$56` is the running destination pointer, which explains why sector 0 can
  land correctly even if the later loop state goes bad.
- `movea.l (78,A3),A2` means `A2` is not guaranteed to remain the original
  `IOStdReq` across iterations. The trace therefore logs both the saved STRAP
  request and the raw `A2[$20]` / `A2[$24]` counters seen inside the loop.
- `$FEA5A0` is a completion / cleanup path, not another sector-commit path. If
  a live trace still shows `$FEA57E -> $FEA580` when the counters look equal,
  that is meaningful evidence rather than an uninteresting fall-through.

Saved disassembly for the READ-side blit setup:

```asm
$FEA996  move.w  #$1DD8,(64,A0)          ; READ decode BLTCON0
$FEA99C  move.w  #$0002,(66,A0)
$FEA9A2  move.w  (20,A1),(88,A0)
$FEA9A8  move.l  #$00FEA9B4,(4,A1)       ; callback
```

Saved disassembly for the validation exits that all map to later `$1B` traces:

```asm
$FEACF4  bsr.w   $FEAA0E                 ; header checksum helper
$FEACF8  cmp.l   D0,D6
$FEACFA  bne.w   $FEAE6C                 ; header checksum mismatch
$FEAD0A  cmpi.b  #$FF,(0,A7)
$FEAD10  bne.w   $FEAE6C                 ; format byte != $FF
$FEAD18  cmp.b   (75,A3),D0
$FEAD1C  bne.w   $FEAE6C                 ; track byte mismatch
```

The runtime WB diag already proved:

- the raw DMA buffer contains valid syncs and valid header checksums
- our own decoder recovers sectors 0 and 1 correctly from that buffer
- only one READ-side decode blit lands in the bootblock buffer on the second
  successful `CMD_READ`

So the next useful distinction is:

- `io_Length` is already `+$200` at STRAP time on the second attempt, or
- the READ loop exits early because its internal running state says the request
  is complete, or
- the next iteration re-enters validation and branches out through one of the
  `$1B` sub-failures before the second blit is armed

The new inserted-disk trace logs all three:

- `IOStdReq` at `STRAP_CMD_READ_CALL = $FE859C`
- `IOStdReq` again at `STRAP_POST_CMD_READ = $FE85A0`
- the raw loop-side counters `A2[$20]`, `A2[$24]`, `unit[$49]`, `unit[$4E]`,
  `unit[$56]`, and the *next PC* after `$FEA57E`
- validation-branch context with `D2`, `D3`, `unit[$49]`, and `unit[$4B]`

That should be enough to separate "short request" from "early loop exit" from
"next-iteration validation failure".

Important correction from the BCC-specific follow-up probe:

- The earlier "`$FEA57E -> $FEA580` means the branch fell through" reading was
  wrong because raw `PC` there was still showing prefetch movement.
- When we trace the actual `BCC` instruction at `instr_start_pc = $FEA57E`
  with `IR = $6420`, the branch behaves correctly:
  - first loop completion: `SR=$0009`, carry set, branch not taken, continue at
    `$FEA580`
  - normal 1024-byte completion: `SR=$0004`, carry clear / zero set, branch
    taken, complete at `$FEA5A0`
  - later one-sector completion: `SR=$0004`, carry clear / zero set, branch
    taken, complete at `$FEA5A0`

That means the new anomaly is not "bad BCC decode". It is that the later READ
loop reaches `$FEA57E` with `A2[$20]=$00000200` and `A2[$24]=$00000200`, so
the loop's live limit is already one sector by the time the branch executes,
even though STRAP's external `IOStdReq` still says `io_Length=$00000400`.

## Internal request-block handoff

The new two-pass trace
`trace_wb13_later_read_block_writers` answers the next missing question:
where does the later one-sector block come from, and who sets its limit?

Saved live result:

- `unit[$44]` starts as `0`
- at `cck=13152900`, trackdisk writes `unit[$44] = $00C014E2`
  This is STRAP's original `IOStdReq`.
- at `cck=17202432`, trackdisk writes `unit[$44] = $00C05B18`
  This is the later internal block that the one-sector READ loop uses.
- when the later READ loop finally enters at `cck=19957257`, `A2 = $00C05B18`

The saved disassembly for that repointing sequence is:

```asm
$FEA3BC  movea.l A1,A2
$FEA3BE  move.l  A2,(68,A3)             ; unit[$44] = active request block
$FEA3C2  move.l  #$00000000,(32,A2)     ; clear active block io_Actual
$FEA3CA  move.l  (40,A2),(86,A3)        ; seed unit[$56] from block io_Data
$FEA3D0  move.l  (44,A2),D0
$FEA3D4  bsr.w   $FE9E02
```

So the handoff into `$00C05B18` is not accidental memory corruption. Trackdisk
explicitly swaps `unit[$44]` from STRAP's request block to another block and
then runs the later READ loop against that block.

## Who sets the one-sector limit

The second pass watches `$00C05B18+$20..+$27` directly. The live sequence is:

- an Exec-side block init routine fills the block with `$ABABABAB`
- later, the same init path writes `$00000200` into `$00C05B18+$24`
- only after that does trackdisk clear `$00C05B18+$20` back to `0`
- when the later READ loop starts, the block is already:
  `+$20 = 0`, `+$24 = $200`

Saved live trace:

```text
cck=17172368 block=$00C05B18 +$20 00000000->ABABABAB +$24 00000000->00000000
cck=17172380 block=$00C05B18 +$20 ABABABAB->ABABABAB +$24 00000000->ABABABAB
cck=17199691 block=$00C05B18 +$20 ABABABAB->ABABABAB +$24 ABABABAB->00000200
cck=17202444 block=$00C05B18 +$20 ABABABAB->00000000 +$24 00000200->00000200
cck=19957257 later READ loop head sees block=$00C05B18 +$20=$00000000 +$24=$00000200
```

The corresponding saved ROM ranges are:

```asm
$FF4412  move.l  #$ABABABAB,D3
$FF441C  move.l  D3,(A3)+                ; fill new block with ABABABAB
$FF441E  cmpa.l  D2,A3
$FF4420  blt.s   $FF441C
...
$FF4510  move.l  D3,(40,A3)
$FF4514  move.l  D4,(36,A3)              ; later block +$24 = D4
$FF4518  move.l  (16,A1),(44,A3)
```

Two details matter:

- The raw CPU write watch only records the current `PC`, so for these writes it
  reports `$FF4420` / `$FF451A` while the actual writing instructions are the
  immediately preceding `move.l` operations shown above.
- The one-sector limit is therefore established before trackdisk enters the
  later READ loop. The loop does not shrink `$A2+$24` from `$400` to `$200`;
  it inherits a block whose `io_Length` is already `$200`.

## This Is The `Validator` Task's Request

The next follow-up trace,
`trace_wb13_later_request_origin_context`, shows that the later one-sector
request is not STRAP retrying its bootblock `CMD_READ`.

Saved live result:

- the block fill and field-init sequence runs in the current task named
  `Validator`
- the internal block is finalized as:
  `cmd=$0002 len=$00000200 data=$0000604C off=$00000000 flags=$00`
- only after that does trackdisk switch `unit[$44]` from STRAP's request block
  to `$00C05B18`
- when the later READ loop starts, it is servicing that already-built
  `Validator` request

The key live trace points are:

```text
cck=17199683 task=Validator ... data $ABABABAB->$0000604C
cck=17199691 task=Validator ... len  $ABABABAB->$00000200
cck=17199705 task=Validator ... off  $ABABABAB->$00000000
cck=17202432 task=trackdisk.device ... unit[$44] $00C014E2->$00C05B18
cck=19957260 task=trackdisk.device ... final req=$00C05B18 cmd=$0002 len=$00000200 data=$0000604C off=$00000000
```

That matters because it changes the interpretation of the later `$200` loop:

- it is not STRAP's 1024-byte bootblock read mysteriously shrinking itself
- it is not evidence that the later READ loop is mis-decoding sector 1
- it is a separate single-sector request built upstream by the `Validator` task

The saved ROM range around the builder already shows the request fields being
materialized:

```asm
$FF446E  suba.l  A1,A1
$FF4470  movea.l ($0004).w,A6
$FF4474  jsr     (-294,A6)
$FF4478  movea.l D0,A1
...
$FF450A  move.w  D2,(28,A3)             ; io_Command
$FF4510  move.l  D3,(40,A3)             ; io_Data
$FF4514  move.l  D4,(36,A3)             ; io_Length
$FF4518  move.l  (16,A1),(44,A3)        ; io_Offset
```

The live task name plus that `A1=0` Exec call strongly suggests this path is
working from the current task context rather than STRAP's bootblock loop.

## Current working model

As of the latest diagnostics:

- The broad "bad MFM stream" explanation is no longer the best fit.
- The sector-1 corruption visible in the bootblock buffer is downstream reuse of
  that chip RAM by graphics / Intuition blits after trackdisk has already
  stopped touching it.
- The later successful one-sector READ loop is now explained: it belongs to the
  `Validator` task's own `CMD_READ len=$200 data=$604C off=0`, not to STRAP's
  earlier 1024-byte bootblock read.
- The next useful question moves back earlier in the chain: why STRAP's own
  bootblock-read / checksum path still fails to reach a clean boot before this
  later `Validator` activity starts.

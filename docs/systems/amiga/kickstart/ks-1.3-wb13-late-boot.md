# KS 1.3 Workbench 1.3 Late-Boot Notes

This note saves the live-task and ROM-context work for the point *after* the
bootblock has already loaded and executed.

The earlier STRAP / `trackdisk.device` bootblock investigation established that:

- STRAP issues a real 1024-byte `CMD_READ` from offset 0.
- The initial bootblock read completes successfully.
- Bootblock code in chip RAM does execute.

So the remaining question is later: why does the machine settle into the Exec
idle region instead of continuing on to a visible Workbench boot?

## Rerun

```bash
cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_late_boot_tasks_and_signals -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_lifecycle -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_transition_window -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_signal_window -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_task_field_writers -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_ports_and_sigalloc -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trace_wb13_validator_idcmp_port_traffic -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  force_wake_validator_experiments -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_validator_idcmp_creator_path -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_validator_idcmp_ref_holder_writers -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_validator_requester_ram_chain -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_validator_requester_payload_strings -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_disk_write_path_gap -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_trackdisk_beginio_requests -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs --test wb13_late_boot_trace \
  trace_wb13_root_block_read_compare -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  timer_device_request_trap -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  trap_timer_vbl_handler -- --ignored --nocapture

cargo test -p machine-commodore-amiga-ocs \
  dump_ks13_trackdisk_bootblock_regions -- --ignored --nocapture
```

The first two tests live in
`crates/machine-commodore-amiga-ocs/tests/wb13_late_boot_trace.rs`.
The disassembly test lives in
`crates/machine-commodore-amiga-ocs/tests/ks13_disasm_bootblock.rs`.

## Late-Window Facts

Measured over frames 801-900 with the real `workbench-1.3.adf` inserted:

- The sampled PC is `$FC0F94` on all 100 late-frame samples.
- `DispCount` still advances by 69 over that window.
- `IdleCount` advances by 100 over that window.
- The runnable work is not dead. The running-task histogram is:
  - `input.device`: 80 frame samples
  - `trackdisk.device`: 13
  - `File System`: 7
- `TaskReady` is empty at the late snapshot, so the CPU is repeatedly falling
  back into Exec idle between bursts of task activity.

The important negative result:

- Late-window `DoIO` / `SendIO` traffic is timer-only housekeeping.
- No late `trackdisk.device` `CMD_READ` traffic shows up in frames 801-900.
- `Validator` remains in `TaskWait` with `sigWait=$80000000` and does not
  appear in the late running-task histogram.
- No late-window `Signal()` calls target `Validator`.

That makes this a live-but-quiescent state, not a dead CPU and not a generic
MFM decode failure.

## Validator Lifecycle

A focused `Validator` trace through the first 700 frames shows that the task is
not immediately dead. Its state changes are:

- frame 243: first seen, already waiting on `$00000100`
- frame 363: runs again
- frame 366: waits on `$00000010`
- frame 367: runs again
- frame 371: moves through `READY`
- frame 372: runs again
- frame 376: enters `WAIT` on `$80000000`
- frame 377 onward: not observed running again through frame 700

So the late `$80000000` wait is a *final* state reached only after Validator
does some real work. The failure is therefore later than task creation and
later than its first wakeup.

## Timer Findings

The older "CIA/TOD timers never started" suspicion is not the best fit for the
current KS 1.3 / WB 1.3 failure.

Standalone timer probes show that timer machinery is alive:

- `trap_timer_vbl_handler` records the timer VBL handler at `$00FE935A` and the
  CIA-TB handler at `$00FE93A6`.
- Over 700 frames in the slow-RAM machine, the VBL handler fires 2100 times and
  the CIA-TB handler fires 25 times.
- `timer_device_request_trap` records 311 `timer.device` `BeginIO` hits over
  400 frames, including 22 `TR_ADDREQUEST` calls from `trackdisk.device`.

The WB 1.3 transition-window probe says the same thing in the real failing
boot:

- `timer.device` resolves to `$00C022EE` when discovered earlier in boot.
- Over frames 340-390, the timer VBL handler fires 177 times and the CIA
  handler fires 68 times.
- In that same window, `trackdisk.device`, `input.device`, and `File System`
  all hit `timer.device` `BeginIO`.
- `trackdisk.device` repeatedly queues MICROHZ requests via
  `io=$00C0478E dev=timer.device unit=$00C02318 cmd=$0009`, commonly with
  `len=$00000BB8` (3000 microseconds), plus some longer requests.

That means the timer subsystem is running and the trackdisk delay path is being
queued in the failing WB 1.3 boot. The broad class of bug where CIA / TOD
timers never start automatically is therefore not supported by the current
evidence.

## Validator Signal Window

A tighter probe over frames 360-380 keeps the state transitions from the
lifecycle trace:

- frame 360: `WAIT`, `sigWait=$00000100`, `sigRecvd=$00000000`
- frame 363: `RUN`
- frame 366: `WAIT`, `sigWait=$00000010`, `sigRecvd=$00000000`
- frame 367: `RUN`
- frame 371: `READY`
- frame 372: `RUN`
- frame 376: `WAIT`, `sigWait=$80000000`, `sigRecvd=$00000000`

The important negative result is that this focused window did **not** capture a
matching Exec-side `Signal()`, `PutMsg`, `GetMsg`, `ReplyMsg`, `WaitPort`,
`DoIO`, or `SendIO` event from `Validator`, `File System`, `trackdisk.device`,
or `input.device` around the final transition.

So the current shape of the bug is narrower still:

- Validator does reach the final `$80000000` wait.
- The timer subsystem is active while this happens.
- `tc_SigRecvd` never shows the wake bit arriving.
- The wakeup or reply path is not showing up as a normal high-level Exec call
  from the obvious tasks in that short window.

## Validator Task-Field Writers

A watch on the `Validator` task struct over frames 360-380 finally shows the
useful contrast between the *working* wakeups and the failing final wait.

Normal wakeups do write `tc_SigRecvd` and move the task to `READY`:

- frame 363: `tc_SigRecvd` is written to `$00000100`, then `tc_State` becomes
  `READY`, then the dispatcher marks the task `RUN`, then Exec clears
  `tc_SigRecvd` back to zero after the wait returns.
- frame 367: the same sequence happens again with `tc_SigRecvd=$00000010`.

The writer PCs line up with the saved Exec disassembly:

- around `$FC1E84..$FC1EC8`: signal delivery and wakeup path
  - `or.l D0,(26,A1)` updates `tc_SigRecvd`
  - if the task is waiting and the mask matches, Exec removes it from
    `TaskWait`, sets `tc_State = READY`, and queues it
- around `$FC1F10..$FC1F62`: `Wait()` bookkeeping
  - `move.l D0,(22,A1)` stores `tc_SigWait`
  - `move.b #$04,(15,A1)` moves the task to `WAIT`
  - `and.l (26,A1),D1` / `eor.l D1,(26,A1)` consume delivered signals after
    wakeup
- around `$FC0F96..$FC0FC4`: dispatcher path
  - `move.b #$02,(15,A3)` marks the selected task `RUN`

The failing final wait is different:

- frame 376: Exec writes `tc_SigWait` from `$00000010` to `$80000000`
- frame 376: Exec writes `tc_State = WAIT`
- there is **no** preceding write of `$80000000` into `tc_SigRecvd`

So the critical fact is now explicit:

- the final `$80000000` wait is being installed normally by Exec `Wait()`
- but the matching delivered bit never arrives in `tc_SigRecvd`

This is stronger than the earlier inference from sampled state alone. The wake
mechanism for bit 31 is absent, while the earlier `$100` and `$10` wakeups work
through the standard Exec signal-delivery path.

## Bit 31 Is IDCMP

The next probe closes the gap on what bit 31 actually is.

`trace_wb13_validator_ports_and_sigalloc` shows:

- `Validator` is first seen at frame 244 with `sigAlloc=$0000FFFF`
- the extra high bit does **not** exist from task creation
- at frame 368, `Validator.tc_SigAlloc` changes to `$8000FFFF`
- at that exact frame, two `Validator`-owned ports appear:
  - `$00C073B0 name=IDCMP flags=$02 sigBit=31 mask=$80000000`
  - `$00C073D8 name=IDCMP flags=$00 sigBit=31 mask=$80000000`

So the late `$80000000` wait is not a random raw signal bit. It is tied to
late-created IDCMP-related ports owned by `Validator`, and the bit-31
allocation happens at the same time those ports appear.

That meaningfully changes the shape of the bug:

- the missing wake is now more likely UI / Intuition / IDCMP-related than
  floppy / timer-related
- the bug happens after `Validator` transitions into a phase where it expects
  IDCMP-style message traffic

## Validator IDCMP Ports Stay Empty

The longer follow-up trace, `trace_wb13_validator_idcmp_port_traffic`, carries
those same two ports forward through frame 900.

Saved snapshots:

- frame 368: both IDCMP ports exist, both `msgCount=0`
- frame 376: both still `msgCount=0`
- frame 500: both still `msgCount=0`
- frame 700: both still `msgCount=0`
- frame 900: both still `msgCount=0`

So the important late fact is not just that `Validator` waits on bit 31. It is
that the two bit-31 IDCMP ports behind that wait remain empty for the entire
observed late boot.

That rules out a large class of earlier theories:

- this is not explained by the bootblock read shrinking to one sector
- this is not explained by a generic trackdisk decode failure
- this is not explained by timers never starting

The live machine gets far enough to create `Validator`'s IDCMP ports, but no
message ever lands there.

## Force-Wake Split

The next step was to stop observing and inject the missing wake by hand.

`force_wake_validator_experiments` runs four fresh-boot variants at the moment
`Validator` first reaches:

- frame 376
- `sigWait = $80000000`
- signal IDCMP port = `$00C073D8`
- ignore IDCMP port = `$00C073B0`

The outcomes are:

- `signal-only`
  - force Exec-style wake for bit 31
  - final `Validator` state becomes `READY`
  - `sigRecvd = $80000000`
  - both ports still empty
- `putmsg-ignore-port`
  - queue a dummy message on the `PA_IGNORE` IDCMP port only
  - final `Validator` state remains `WAIT`
  - `sigRecvd` stays zero
- `putmsg-signal-port`
  - queue a dummy message on the `PA_SIGNAL` IDCMP port and force the bit-31 wake
  - final state matches `signal-only`
  - the dummy message remains queued on the signal port
- `putmsg-both-ports`
  - queue dummy messages on both IDCMP ports and force the bit-31 wake
  - final state again matches `signal-only`
  - the signal-port dummy message remains queued

The useful split is:

- a bit-31 wake is sufficient to move `Validator` out of the stuck
  `WAIT sigWait=$80000000 sigRecvd=$00000000` state
- merely putting a message on the `PA_IGNORE` port is not enough
- a naive dummy message on the signal-backed IDCMP port is also not enough to
  produce obviously correct follow-on behavior

So the immediate blocker is now clearer:

- the real system is missing the bit-31 wake that should reach `Validator`
- and the wake is tied to IDCMP / Intuition state, not to floppy decode or
  timer startup

What is *not* yet proven is the exact shape of the missing upstream event. The
dummy `PutMsg` experiments do not recreate a real IDCMP message flow, so they
do not yet tell us whether the emulator is failing to:

- generate the first IDCMP message
- route it to the signal-backed port
- or deliver the associated signal when the message is posted

## Validator Builds Its Own IDCMP Ports

The creator-path probe, `trace_wb13_validator_idcmp_creator_path`, shows that
the two late IDCMP ports are created by the `Validator` path itself.

The important facts are:

- frame 368 is still the creation point
- `Validator` is the running task during the port-setup writes
- no public `Intuition` LVO hit was captured in frames 330-390 for
  `OpenScreen`, `OpenWindow`, `InitRequester`, or `AutoRequest`

The saved disassembly in `ks13_disasm_bootblock.rs` makes the path concrete:

- `$00FD56F0..$00FD5768` creates and stores the paired IDCMP ports
- `$00FE00A4..$00FE0124` allocates and initializes a `MsgPort`
- `$00FE0298` is the wrapper used to add that port to Exec

That helper does exactly what the late watch suggested:

- allocate a signal bit
- allocate `0x22` bytes of Exec memory for the port
- set name pointer, node type, `mp_Flags`, `mp_SigBit`, and `mp_SigTask`
- add the port to Exec

Then the `Validator` owner path does this:

1. create the first `IDCMP` port and store it at owner offset `+$5A`
2. read its signal bit and free that bit back to Exec
3. mark that first port `mp_Flags = $02` (`PA_IGNORE`)
4. create the second `IDCMP` port and store it at owner offset `+$56`
5. store a separate control word at owner offset `+$52`

So the late `$80000000` wait is definitely part of an intentional paired-IDCMP
setup. The two ports are not accidental byproducts of unrelated activity.

## IDCMP Owner Structures

The external-ref probe, `trace_wb13_validator_idcmp_ref_holder_writers`, shows
where those port pointers live.

The main owner structure is rooted at `$00C07328`:

- `+$52` becomes `$00008440`
- `+$56` becomes the signal-backed port `$00C073D8`
- `+$5A` becomes the `PA_IGNORE` port `$00C073B0`
- nearby helper fields at `+$62`, `+$63`, and `+$64` are populated
  immediately afterward by the same `Validator` path

There is also a second small structure around `$00C003FE/$00C00406` that holds
copies of the port pointers:

- `$00C003FE` is set to the ignore port `$00C073B0`
- `$00C00406` is first aligned with that ignore port, then upgraded to the
  signal-backed port `$00C073D8`

The interesting part is not the later churn around nearby words. The decisive
owner-field writes all happen at frame 368 when `Validator` enters this IDCMP
phase.

## What This Changes

This moves the late-boot diagnosis again.

What is now clearly *not* happening:

- the ROM is not randomly sleeping on bit 31 without context
- `trackdisk.device` is not the direct owner of the late IDCMP wait
- the two IDCMP ports are not half-initialized artifacts

What is now clearly happening:

- `Validator` intentionally enters a UI / IDCMP phase
- it allocates two real Exec ports named `IDCMP`
- one is deliberately `PA_IGNORE`
- the other is the real signal-backed wake port

So the next useful question is narrower than "why didn't the signal arrive?":

- does earlier emulator-side disk or filesystem behavior push `Validator` into a
  requester / validation path that a clean WB 1.3 boot should avoid?
- or, once `Validator` is in that path, does the emulator fail to generate or
  route the real event that should service the owner object at `$00C07328`?

## The Earlier "Caller PC" Was Data, Not Code

The newer `trace_wb13_validator_requester_ram_chain` probe resolves the
odd-looking `$00C06D30` / `$00C06C34` values that previously looked like a
slow-RAM caller into `$00FD56F0`.

Those values are *not* useful code return PCs. They are live data/state
pointers passed into the ROM requester path:

- at frame 368, the requester setup flow reaches `$00FDEDD2..$00FDEE86`
- in that window, `D2 = $00C06D30`, `A2 = $00C06C34`, and `A3 = $00C07328`
- the ROM then pushes `(10,A2)` and `A3`, calls `$00FD56DA`, and from there
  enters the IDCMP-port setup path at `$00FD56F0`

So the previous stack-top interpretation was misleading. The meaningful caller
context is the ROM state machine around `$00FDEDD2`, not "code at `$00C06D30`".

That matters because it removes one false lead: we are not chasing an unknown
slow-RAM code segment here. We are watching a ROM requester/helper path operate
on slow-RAM state blocks.

## Validator Polls The Signal Port Directly

The same RAM-chain probe also pins down the final wait path at frame 376.

Right before the final stuck `Wait($80000000)`, `Validator` executes:

1. `$00FDE3D0`: push owner `+$56`, the signal-backed IDCMP port
2. `$00FE02D4`: wrapper around `GetMsg(port)`
3. `GetMsg($00C073D8)` returns `0`
4. `$00FDE3E4..$00FDE3EE`: read `mp_SigBit` from that same port and compute
   `1 << sigBit`
5. `$00FE0244..$00FE024E`: wrapper around `Wait(mask)`
6. final call is `Wait($80000000)`

So the stuck path is more specific than "bit 31 never arrives":

- `Validator` is not waiting on the ignore port
- it is not blocked on an internal ignore-to-signal forwarding step
- it explicitly polls the real signal-backed port `$00C073D8`
- that port is empty
- then it waits on that port's signal bit

This weakens the old "maybe the ignore port never forwards" theory. The
observable late failure is now:

- no message is available on the signal-backed IDCMP port when `Validator`
  checks it
- therefore no signal is pending on that port's bit either

That points even more strongly at the *producer* side of IDCMP / Intuition /
input behavior, not at Exec message delivery itself.

## Requester Payload Says "Error validating disk"

The newer `trace_wb13_validator_requester_payload_strings` probe resolves the
late requester payload out of the live slow-RAM blocks that feed the ROM path
at `$00FDEDD2..$00FDEE92`.

At frame 368:

- `D2 = $00C06D30`
- `A2 = $00C06C34`
- `A3 = $00C07328`
- `A5 = $00C06BFC`

The `D2` state block contains a direct ROM string pointer:

- `[$00C06D24] -> $00FFFDDD "Error validating disk"`

and also carries two pointers to:

- `$00FDE888 "Workbench Screen"`

The `A2` requester block contains:

- `[$00C06C4E] -> $00FDE4C4 "System Request"`
- `[$00C06C68] -> $00FF4E24 "Cancel"`
- control word `$00008440`
- pointer `$00C05B18`, the internal trackdisk request block already seen in the
  later one-sector read traces

So by the time `Validator` builds the late IDCMP ports, the machine is not in a
generic "show Workbench UI" path. It is explicitly building a system requester
for `Error validating disk`.

That materially changes the interpretation of the late empty IDCMP wait:

- the UI wait is real
- but it is downstream of a validation failure the emulator has already caused
- so the more interesting bug is earlier than the bit-31 wait

## A Real Porting Gap Exists, But It Is Not Yet The Blocker

The current machine port still has an unimplemented disk-write DMA path in
`crates/machine-commodore-amiga-ocs/src/lib.rs`: when a live disk DMA transfer
is armed with `DSKLEN.WRITE`, `service_disk_dma_word()` returns immediately and
does not consume any words.

That is a real implementation gap, and the archive machine does have the
corresponding write logic.

However, the focused `trace_wb13_disk_write_path_gap` probe shows that this is
not the immediate cause of the late WB 1.3 failure through frame 390:

- `writeArms=0`
- `dskdatWrites=0`
- `paula writeDMAWords=0`
- `paula writePIOWords=0`
- `driveCapturedWords=0`

So "something we haven't implemented" is still a plausible class of bug in the
codebase, but the specific unimplemented write-DMA path is not what drives the
machine into `Error validating disk`.

## Validator Fails On The Root-Block Read

The more important new evidence comes from the request-level trackdisk probes.

`trace_wb13_trackdisk_beginio_requests` shows that `Validator` uses its
internal request block `$00C05B18` for two key reads:

- frame 243: `CMD_READ len=$200 off=$00000000 data=$0000604C`
- frame 282: `CMD_READ len=$200 off=$0006E000 data=$0000604C`

`$0006E000` is block 880, the standard Amiga DD root block location.

The follow-up `trace_wb13_root_block_read_compare` probe shows that this read
does not actually land root-block data into RAM:

- frame 282: `BeginIO` for `off=$0006E000` sees the request block still showing
  the previous `actual=$200`
- frame 283: the request is reset to `actual=$00000000`, `err=0`, `flags=$00`
- by frame 364: the same request block changes to `err=27`
- the destination buffer at `$0000604C` still begins with the old bootblock
  bytes `44 4F 53 00 ...`
- those bytes do *not* match the ADF root block at `$0006E000`, which starts
  `00 00 00 02 ...`
- throughout the failed request window, the live drive stays at `cyl=0 head=0`
- CIA-B PRB only flips between `$FF` and `$75`
- `$75` means DF0 selected with motor on, but `/STEP` never pulses low, so the
  drive never seeks away from track 0 during the root-block request

So the late validation failure is now much more concrete:

- `Validator` asks trackdisk for the root block
- the request never transfers the expected sector data into the destination
  buffer
- the request later fails with error 27 / `$1B`
- the physical DF0 model never leaves cylinder 0 while that request is live
- only *after that* does the system build the `Error validating disk`
  requester and fall into the empty IDCMP wait

That means the late IDCMP stall is secondary. The primary bug is that the
root-block read at offset `$0006E000` is failing on the emulated machine.

The current best hardware-side lead is therefore narrower still:

- either trackdisk never emits the expected step pulses for the seek to block
  880 in this emulator state
- or our CIA-B / drive-control integration is missing the transitions that
  should move DF0 off cylinder 0 before the read begins

## Saved Wait Sites

### `$FC0F94` is Exec idle

The hot PC is the idle loop, not an arbitrary spin:

```asm
$FC0F84  BNE.S   $FC0F96
$FC0F86  ADDQ.L  #1,(280,A6)      ; IdleCount++
$FC0F8A  BSET    #7,(292,A6)
$FC0F90  STOP    #$2000
$FC0F94  BRA.S   $FC0F7C
```

So the CPU is sleeping in Exec idle and waking on interrupts, then falling
 back to idle because no ready task remains.

### `$FC1F0C` / `$FC1F4C` are Exec `Wait()`

The common stack candidate across waiting tasks is the tail of `Wait()`:

```asm
$FC1F0C  MOVEA.L (276,A6),A1      ; ThisTask
$FC1F10  MOVE.L  D0,(22,A1)       ; tc_SigWait = mask
...
$FC1F48  JSR     (-30,A6)         ; dispatcher / scheduler
$FC1F4C  MOVEA.L A0,A5
$FC1F4E  MOVEA.L (276,A6),A1
$FC1F52  MOVE.L  (22,A1),D0
$FC1F56  MOVE.L  (26,A1),D1       ; tc_SigRecvd
$FC1F5A  AND.L   D0,D1
$FC1F5C  BEQ.S   $FC1F22          ; still waiting
```

So `$FC1F4C` on a blocked task stack is exactly what it looks like: the task
went through Exec `Wait()` and is re-checking received signals after dispatch.

### Validator waits through `$FE0252`

Validator's unique late stack site is a thin wrapper around `Wait(mask)`:

```asm
$FE024A  MOVE.L  (8,A7),D0
$FE024E  JSR     (-318,A6)        ; Wait()
$FE0252  MOVEA.L (A7)+,A6
$FE0254  RTS
```

At the late snapshot the Validator task is:

- state = `WAIT`
- `sigWait = $80000000`
- not observed running in the final 100 frames
- not a target of any late `Signal()`

The lifecycle trace tightens that further: Validator reaches this wait state
around frame 376, then does not re-enter the runnable set afterward.

### `trackdisk.device` waits on `$300`

The late trackdisk wait site is explicit:

```asm
$FEAAE6  MOVE.L  #$00000300,D0
$FEAAEC  MOVE.L  A6,-(A7)
$FEAAF2  JSR     (-318,A6)        ; Wait($300)
$FEAAF6  MOVEA.L (A7)+,A6
$FEAAF8  BRA.S   $FEAAE4
```

That matches the late task snapshot:

- `trackdisk.device` in `TaskWait`
- `sigWait = $00000300`

### `input.device` waits on its input mask

The late input task site is also a direct `Wait()` path:

```asm
$FE5F36  MOVE.L  D7,D0
$FE5F38  JSR     (-318,A6)        ; Wait(...)
$FE5F3C  BTST    #0,(61,A5)
```

Late snapshot:

- `input.device` in `TaskWait`
- `sigWait = $C0000000`

### File System loops on `$100`

The file-system helper region shows the classic wait-and-retry loop:

```asm
$FF4692  MOVE.L  #$00000100,D0
$FF4698  JSR     (-318,A6)        ; Wait($100)
$FF469C  BRA.S   $FF467A
```

That matches the late task snapshot and signal traffic:

- `File System` in `TaskWait`
- often waiting on `$00000100`
- signaled by `input.device` and occasionally `trackdisk.device`

## Late I/O Summary

The saved late-window I/O summary is:

- `trackdisk.device` only issues timer requests (`timer.device`, command `$0009`)
- `File System` only shows timer-style activity in this window (`$0009` / `$000A`)
- no `DoIO` / `SendIO` entries target `trackdisk.device` for new disk reads

So by frames 801-900 the system is no longer trying to read more disk data.

## Current Working Model

The late WB 1.3 failure is not:

- STRAP failing to read the initial bootblock
- a broken `BCC` in the trackdisk extract loop
- a CPU that is hard-stuck with no scheduling

The current best model is:

1. The bootblock path succeeds far enough to hand control onward.
2. The machine stays alive and keeps scheduling `input.device`,
   `trackdisk.device`, and `File System`.
3. No further disk-read traffic survives into the late window.
4. `Validator` does run earlier, but by about frame 376 it is left asleep on
   signal bit 31 (`$80000000`) and never gets reawakened in the observed
   late phase.
5. That bit now looks like an IDCMP / UI wait, not a floppy wait: `Validator`
   allocates bit 31 only when two `IDCMP` ports appear at frame 368.
6. Those two `Validator`-owned IDCMP ports stay empty through frame 900.
7. That does not appear to be caused by a dead timer subsystem: timer VBL,
   timer CIA, and trackdisk timer requests are all active during the failing
   transition window.
8. Earlier Validator wakeups on `$00000100` and `$00000010` do take the normal
   Exec signal-delivery path, but no equivalent `tc_SigRecvd=$80000000` write
   ever appears for the final wait.

That is a much narrower target than "Workbench boot never starts". The useful
next probe is not more bootblock decode instrumentation and not generic timer
bring-up. It is the Intuition / IDCMP path that is supposed to post or route a
message into `Validator`'s bit-31 port before the system falls back to pure
idle/timer/input churn.

## Display-Side Follow-Up

The later display investigation changed the picture again.

### CPU byte writes are not driving the Workbench display mode

The current machine still has a real archive-vs-port difference for CPU byte
writes to custom registers: the archive machine has an explicit merge latch for
`MOVE.B` into custom registers, while the current machine still dispatches raw
16-bit values on CPU byte writes.

That is a plausible emulator bug in general, but the focused Workbench probe
does **not** support it as the cause of the current WB 1.3 desktop corruption:

- through frame 430 there are no CPU byte writes to the display custom
  registers involved in the Workbench setup path
- specifically, no byte writes hit `DIWSTRT`, `DIWSTOP`, `DDFSTRT`, `DDFSTOP`,
  `BPLCON0`, `BPLCON1`, `BPLCON2`, `BPL1MOD`, `BPL2MOD`, or the bitplane
  pointer registers

So the suspicious `BPLCON0` state is not being created by CPU-side custom
byte-write corruption in this boot path.

### Workbench really does switch into hi-res in the desktop area

The focused copper-move trace shows a stable three-phase `BPLCON0` pattern once
Workbench reaches its desktop display setup:

- `vpos=$02B hpos=$057` → `BPLCON0=$0302`
- `vpos=$02C hpos=$003` → `BPLCON0=$A302`
- `vpos=$100 hpos=$003` → `BPLCON0=$0302`

This repeats every frame in the late Workbench window.

That means the desktop display is **not** simply staying in `0302` all the way
through. The visible desktop phase enters `A302`, then later drops back to
`0302`.

In other words:

- the Workbench desktop region really is switching into a hi-res, two-bitplane
  mode during the visible area
- the earlier "sampled `BPLCON0=$0302`, so this is not hi-res" conclusion was
  too coarse because it ignored the mid-frame copper changes

### The bitplane DMA scheduler is already following hires cadence

The next probe traced a full late desktop line after the copper switched into
`A302`:

- first observed stable hi-res desktop line: frame 417, `vpos=$02C`
- on that line, the bitplane fetch pattern is:
  - `hpos=$03D` → `BPL2`
  - `hpos=$03F` → `BPL1`
  - `hpos=$041` → `BPL2`
  - `hpos=$043` → `BPL1`
  - continuing in the same alternating `BPL2/BPL1` pattern through `hpos=$0DB`

That is hires-style fetch behaviour, not lores-style 8-CCK grouping.

So the broad theory:

- "Workbench is in hi-res, but the machine is still using lores 8-CCK fetch
  groups"

is **not** supported by the live line trace.

The scheduler is already doing the important hires-side thing here:

- `BPLCON0` really switches to `A302` for the desktop region
- the active line fetch cadence follows that hi-res mode

### What this rules out

These newer probes rule out two attractive but overly broad explanations for
the current Workbench screen corruption:

- the display corruption is not explained by CPU byte writes clobbering the
  display custom registers in this path
- the display corruption is not explained by the bitplane DMA sequencer still
  behaving like lores after Workbench enters its hi-res desktop mode

That pushes the remaining display-side target further downstream:

- Denise-side output / pixel-expansion during the hi-res desktop phase
- interpretation of the fetched Workbench bitplane data
- or Workbench-era drawing / memory contents rather than the high-level copper
  mode switch itself

## Root Cause Found: Machine Denise Wrapper Still Used Lores Fetch Scheduling

The next round of tracing resolved the remaining ambiguity.

The earlier "hi-res line fetch cadence" probe only sampled
`agnus.current_slot()`, which is Agnus's *intended* DMA arbitration view.
That was not enough to prove what the machine-side Denise wrapper was actually
doing to the live bitplane pointers.

The focused pointer-delta probe, `trace_wb13_hires_line_actual_bitplane_fetches`,
showed the real bug:

- on the Workbench hi-res desktop line (`BPLCON0=$A302`, `vpos=$02C`)
- `BPL1PT` advanced at `hpos=$042, $04A, $052, ...`
- `BPL2PT` advanced at `hpos=$03E, $046, $04E, ...`
- both were stepping every **8 CCK**, i.e. lores cadence
- before the fix, the line only fetched about 20 words per plane

That matched the code in the machine wrapper:

- `crates/machine-commodore-amiga-ocs/src/denise.rs`
- phase-0 bitplane fetches still called `lores_fetch_plane(slot_in_block, bpu)`
- so the wrapper kept using the old 8-CCK lores fetch grouping even while the
  desktop region had switched into hi-res mode

This was the real display-side cause of the "destroyed Workbench" rendering.

### Archive Comparison

The archive machine does not duplicate bitplane scheduling inside Denise.
Instead, it asks Agnus for the live DMA grant:

- `bus_plan.bitplane_dma_fetch_plane`

That grant already handles:

- lores vs hires slot cadence
- current BPU depth
- DDF window expansion

So the correct fix in the OCS machine wrapper was to stop deriving fetches with
`lores_fetch_plane()` and instead follow Agnus's live bitplane DMA grant.

### Fix Result

After switching the wrapper to use `agnus.cck_bus_plan().bitplane_dma_fetch_plane`:

- the same `trace_wb13_hires_line_actual_bitplane_fetches` probe now shows:
  - `BPL1PT` advancing at `hpos=$03F, $043, $047, ...`
  - `BPL2PT` advancing at `hpos=$03D, $041, $045, ...`
  - i.e. every **4 hpos** / every **2 CCK**, the expected hi-res cadence
- each plane now fetches 40 words across the desktop line
- a fresh 900-frame Workbench screenshot became mostly readable instead of
  dotted garbage

The new screenshot is materially different:

- title bar and borders render coherently
- Workbench version text is readable
- the system requester body is readable enough to show the next real blocker

So this was not a minor display tweak. It was one of the main reasons the
Workbench screen looked catastrophically corrupt.

## Follow-up Display Fix: DDFSTOP Is Not A Hard Blank Edge

The next visual defect after the hi-res fetch fix was subtler:

- the large right-edge wrap into the next line disappeared only part-way
- the requester still looked horizontally phase-shifted
- the right edge of the requester border came up short

The machine Denise wrapper was still treating `DDF` as a hard visibility gate.
That is too aggressive.

`DDFSTOP` controls when Agnus stops **fetching** new bitplane words. It does not
instantly blank the pixels that are already sitting in Denise's shift
registers. The archive path effectively models that correctly because it keeps
calling `output_pixel_with_beam()` across the viewport and lets an empty shifter
naturally fall back to `COLOR00`.

The OCS wrapper was still doing two wrong things:

- it had previously stopped calling `output_pixel_with_beam()` when `DDF`
  closed, which let tail pixels leak into the next line
- after fixing that, it was still blanking output when `DDF` closed, which
  clipped the trailing edge of the current line

The correct wrapper behavior is:

- keep ticking Denise across the whole viewport
- use the DIW-visible line gate, not `DDF`, for the coarse blanking decision
- let the shifter contents determine when the line's playfield pixels are
  actually exhausted

That follow-up fix restored the requester's right edge and made the 900-frame
Workbench capture look coherent end-to-end instead of truncated on the right.

The corresponding regression test is:

- `hires_line_drains_shift_register_before_next_line`

## RTC Follow-up: `SetClock load` Uses The Old-Address Clock

The remaining "FS-UAE shows the current date/time" question turned out not to
be a renderer issue at all.

Workbench 1.3 is running `SetClock load`, and on KS 1.3 that utility talks
directly to the **old-address RTC** at `$DC0000`. There is no
`battclock.resource` yet in KS 1.3; the disk utility probes the clock hardware
itself.

The useful probe here is:

- `cargo test -p runtime-commodore-amiga --test diag_wb13_rtc trace_wb13_setclock_rtc_accesses -- --ignored --nocapture`

That run on the `A500OcsPalA501` model shows:

- 38 RTC accesses during the late Workbench boot
- reads across the calendar registers (`$0..$B`)
- reads and temporary writes to control registers `CD` (`$D`) and `CF` (`$F`)
- a final Workbench frame with the clock warning gone and the date/time line
  populated

So the underlying behavior is:

- the new `$DC0000` RTC implementation is enough for WB 1.3 `SetClock load`
- the earlier "warning: clock is at old address / invalid" result was not
  because Workbench could not use the RTC
- it was because the headless Amiga script runner was still hard-wired to the
  stock `A500OcsPal` model, which has no RTC

To make that path reproducible from the CLI, `emu198x-script-amiga` now exposes
the model selection explicitly:

- `--model a1000`
- `--model a500`
- `--model a500-a501`
- `--model a500-plus`
- `--model a500-maxed`

For the RTC-equipped KS 1.3 Workbench path, use:

- `cargo run --release -q -p emu198x-script-amiga -- --model a500-a501 --kickstart ~/.emu198x/roms/commodore-amiga/kick13.rom --disk ~/.emu198x/media/commodore-amiga/workbench-1.3.adf --frames 900 --screenshot wb13.png`

# 1541 Disk Bring-up Notes

This note is for the current live-drive blocker in the fresh Rust workspace.

Current symptom:

- `LOAD"*",8,1` reaches the C64 KERNAL `SEARCHING FOR *` banner.
- The attached `1541` ROM is running, the head moves, and the disk read path is active.
- The C64 does not yet advance to `LOADING`.

This document is the shortest useful map of what is already ruled out, what the 1541 references say, and where to focus the next debugging pass.

## Current status

What is already known to be working well enough:

- `D64` mount and directory parsing are alive enough to identify real media in `drive-8`.
- The live `1541` runtime is executing real DOS ROM code over the shared IEC bus.
- The static `D64 -> GCR` build now round-trips back to the original sector bytes in tests.
- The live read head is advancing and producing changing GCR bytes, not one stuck value.
- The C64 side now enters the real disk path by typing `LOAD"*",8,1`, not by reusing the tape-oriented `SHIFT+RUN/STOP` flow.

What is no longer the leading suspect:

- the `mos-6502` core
- basic `D64` container parsing
- a totally broken GCR track build
- a totally stuck live read head

What is still the leading suspect:

- the live `1541` DOS / VIA / IEC handoff after the search phase and before the first file transfer reaches the C64

## Board map from the 1541 service manual

Primary source:

- `/Users/stevehill/Downloads/1540-1541_Service_Manual_Preliminary_314002-01_1985_Jan.pdf`

Important hardware facts pulled from the manual:

- `UC3` is the serial-interface `6522 VIA`, mapped at `$1C00-$1C0F`.
- `UC3 Port B` drives the serial interface logic for `CLOCK`, `DATA`, and `ATNA`.
- The service manual explicitly describes `CLOCK` and `DATA` as bidirectional on the serial side.
- `UC2` is the read/write and mechanism-control `6522 VIA`, mapped at `$1800-$180F`.
- During reads, the read amplifier and PLA assemble serial disk bits into parallel bytes.
- The CPU reads that parallel byte stream from `UC2 Port A`.
- The manual says the CPU reads `UC2 Port A` when `BYTE READY` asserts.
- `UC2 Port B` controls the stepper motor and spindle motor.
- `UC2` also sees write-protect state.

That gives the immediate implementation boundary:

- `UC3` correctness matters for the serial protocol seen by the C64.
- `UC2` correctness matters for converting rotating media data into bytes the DOS ROM can consume.
- `BYTE READY` is not optional glue; it is central to the live read path.

## VIA behavior that matters here

Primary source:

- `/Users/stevehill/Downloads/mos_6522_preliminary_nov_1977.pdf`

Useful points from the datasheet:

- `IFR` is a read / bit-clear register, not a read-to-clear register.
- `IER` masks `IFR` bits into the interrupt output; `IFR` bit 7 is summary status, not an independent flag.
- `CA1` and `CB1` are edge-sensitive interrupt inputs.
- `CA2` and `CB2` can be interrupt inputs, handshake outputs, pulse outputs, or manual outputs depending on `PCR`.
- Handshake timing on `CA2` / `CB2` is explicitly part of normal device operation, not a corner case.
- `CA1` / `CB1` transitions can be the event that releases a handshake line back high.

Immediate consequence for the 1541 bring-up:

- if `IFR` semantics are wrong, the DOS ROM can spin forever in polling loops
- if `PCR` / handshake mode is wrong, the ROM can miss `BYTE READY` or serial-side completion transitions
- if `CA1` / `CB1` edge polarity is wrong, the drive can look alive but never actually complete the search-to-transfer handoff

## DOS ROM landmarks

Primary sources:

- `https://g3sl.github.io/c1541rom.html`
- `/Users/stevehill/Downloads/Inside_Commodore_DOS_OCR.pdf`

These two references are the current best pair:

- `c1541rom.html` is the fast searchable labeled ROM listing.
- `Inside Commodore DOS` gives the same territory in prose, including the job queue, `BYTE READY`, and the controller split between the command processor and the floppy controller.

Useful RAM labels:

- `JOBS $0000-$0005`: job queue entries
- `HDRS $0006-$0011`: track / sector header table paired with the job queue
- `WPSW $001C`: write-protect state-change flag
- `DRVST $0020`: drive status byte
- `DRVTRK $0022`: current track under the head
- `BMPNT $006D`: pointer to the BAM / bit map area
- `T0 $006F`
- `T1 $0070`
- `T3 $0072`
- `DRVNUM $007F`: current drive number
- `LINDX $0082`: active channel / buffer index in many file-loading paths

Useful job codes from the ROM listing:

- `$80`: read sector
- `$90`: write sector
- `$A0`: verify sector
- `$B0`: seek
- `$C0`: bump
- `$D0`: jump to buffer code
- `$E0`: execute buffer code once the drive is ready

Useful ROM landmarks for the current stall:

- `$EBFF`: `IDL1`, top of the main idle / command-processing loop
- `$EC07`: idle path after no pending `ATN` work is detected
- `$EC98`: writes LED state to `$1C00`, then jumps back to `$EBFF`
- `$EC9E`: `STDIR`, start directory-loading path
- `$D313`: `CLDCHN`, used in cleanup / channel-management paths reached from the idle loop and directory handling

Why these matter:

- the current stuck behavior repeatedly pulls the drive back through the `$EBFF` / `$EC07` / `$EC98` territory instead of progressing into a visible C64-side `LOADING` state
- `STDIR` confirms the ROM has a dedicated path for turning a directory read into a pseudo-file stream
- the job queue and header table are the real boundary between high-level DOS command handling and low-level disk-controller work

## Working hypothesis

The current evidence points to one of these:

1. `UC3` serial-side VIA behavior is still slightly wrong, so the drive search phase completes enough to keep the C64 at `SEARCHING FOR *`, but the talker/listener handoff never reaches a proper data phase.
2. `UC2` read-side `BYTE READY` / handshake behavior is still slightly wrong, so the DOS ROM sees rotating bytes and head motion but never completes the sector-transfer conditions it expects.
3. The job-queue or active-buffer path progresses far enough to spin the drive, but some drive-side polling loop never sees the completion condition because a flag, edge, or handshake line is modeled incorrectly.

What this does not currently look like:

- a frontend presentation problem
- a tape-path problem leaking into disk code
- a bad `LOAD"*",8,1` command path
- a simple single-byte parser bug in `D64`

## Next debug pass

The next useful instrumentation should stay narrow and drive-specific.

Trace these first:

- `UC3` / serial-side VIA:
  - `PB`
  - `PCR`
  - `IFR`
  - `IER`
  - `CA1`
- `UC2` / read-side VIA:
  - `PA`
  - `PB`
  - `PCR`
  - `IFR`
  - `IER`
  - any modeled `BYTE READY` event
- 1541 DOS state:
  - `JOBS $0000-$0005`
  - `HDRS $0006-$0011`
  - `DRVST $0020`
  - `DRVTRK $0022`
  - `BMPNT $006D`
  - `T0/T1/T3 $006F/$0070/$0072`
  - `DRVNUM $007F`
  - `LINDX $0082`
- 1541 CPU:
  - `PC`
  - only around the hot loop, not a full unbounded trace

Compare that against:

- the labeled path in `c1541rom.html`
- the prose in `Inside Commodore DOS`
- VICE or MiSTer behavior for the same `LOAD"*",8,1` case, if a comparative trace is available

Success condition for this bring-up stage:

- `Bruce Lee` or another plain single-disk title advances from `SEARCHING FOR *` to `LOADING` over the live attached `1541` path

## Good validation media

For the current stage, prefer:

- plain single-disk `D64` titles
- standard DOS paths
- minimal crack / trainer / multi-side complexity

Good examples already used in the workspace:

- `Bruce Lee (1984)(Datasoft)`
- `Aztec Challenge (1983)(Cosmi)`

Defer for later:

- multi-side titles
- custom fastloaders
- heavily cracked images with custom DOS or serial behavior
- anything that muddies a first `SEARCHING FOR * -> LOADING` proof

## Sources

- `/Users/stevehill/Downloads/1540-1541_Service_Manual_Preliminary_314002-01_1985_Jan.pdf`
- `/Users/stevehill/Downloads/mos_6522_preliminary_nov_1977.pdf`
- `/Users/stevehill/Downloads/Inside_Commodore_DOS_OCR.pdf`
- `https://g3sl.github.io/c1541rom.html`

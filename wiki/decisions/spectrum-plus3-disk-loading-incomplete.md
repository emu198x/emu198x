# Spectrum +3 disk loading

**Status:** ~~Known limitation.~~ ~~Loader runs end-to-end architecturally.~~ **Chase H.Q. (+3) loads to its full title screen as of 2026-05-10 (late afternoon).** Five root causes fixed in one session:

1. Z80 IO bus multi-fire (host bus dispatcher polled level-driven IORQ/RD/WR every half-cycle, multi-firing every `IN` and `OUT`).
2. +3 drive-select wiring (only US0 is routed, so drive 2 aliases drive 0).
3. Multi-sector ReadData (chip reads R..=EOT in one Execution phase; we only delivered one sector).
4. Real per-sector CHRN in `ReadId` (Speedlock disks deliberately record non-matching C/H/N that the loader checks against).
5. Physical-track lookup in `ReadData` (the C parameter is the *expected* cylinder header to verify, not the index to look up sectors — Speedlock seeks to track 7 then asks for `C=2`).

PC histogram now spends 0 % of frames in the boot menu and 89 % in the loaded game's idle loop at $81xx. Framebuffer PNG shows the full title screen: "CHASE H.Q." logo, the two cops, city skyline, OCEAN logo, copyright lines for Ocean Software and Taito Corporation.

## Observation

On the +3 with a `.dsk` (or `.edsk`) file inserted via File > Open
Disk..., selecting **Loader** from the boot menu sits on the Loader
screen indefinitely — no error, no advance, no "loading" indicator,
no boot. Reset and re-try produces the same hang.

## What we know works

- The native UI's Open Disk... dialog picks `.dsk` / `.edsk` / `.zip`,
  routes through the asset loader (zip-aware), and reaches
  `runtime.load_media(MediaSet { slot: "disk-a", … })`.
- `runtime-sinclair-zx-spectrum::variants::SpectrumAmstradClassCore<V>::load_disk_image`
  parses the bytes via `format-amstrad-dsk::parse` and calls
  `self.fdc.insert_disk(0, image)`. The +3's `Plus3Marker` enables
  the µPD765A FDC.
- `format-amstrad-dsk` itself parses both legacy `.dsk` and EDSK
  formats; its tests are green.

## What's not wired

The path from the +3 BIOS's Loader option through the µPD765A's
read-sector commands and back to the BIOS's track / sector
expectations isn't completing. Possibilities (none investigated yet):

- The FDC's command set is partially implemented — `insert_disk`
  populates the chip's drive state, but the BIOS's specific READ /
  SEEK / SENSE_INTERRUPT command sequences may not be answered the
  way the +3 BIOS expects.
- The FDC's interrupt or DRQ signalling isn't routed back to the Z80
  in a way the BIOS reads correctly.
- The disk image's track layout is fine for parsing but doesn't
  match what the +3 BIOS reads at boot (e.g., a CPC-format `.dsk`
  rather than a +3-format one — though the Loader hang suggests the
  BIOS gets nowhere, not "wrong data").

## Why we're not fixing it now

- Spectrum SOLID criterion 7 (Native UI) acceptance bar is
  "runtime file picker, snapshot save/load, runtime window-scale
  selector". The menu *wiring* for Open Disk works correctly: the
  item enables on +3, accepts `.dsk` / `.edsk` / `.zip`, parses, and
  reaches the FDC. The hang is a deeper issue at the FDC ↔ BIOS
  boundary, not a menu bug.
- Code198x's curriculum uses tape-based loading (`.tap`, `.tzx`)
  exclusively — no curriculum unit needs the +3 disk path for the
  October launch.
- The catalogue (criterion 1) currently has one +3 entry (Chase
  H.Q. +3) authored as a future-state placeholder; the harness
  doesn't run +3 disk-load assertions today.
- The FDC fix is genuinely cross-stack — needs tracing the BIOS's
  command sequence against the chip's responses, possibly with a
  reference emulator (Fuse, ZEsarUX) for comparison. Real engineering
  with its own focused commit when it lands.

## What would unblock the fix

1. Pick a known-good +3 disk image (a TOSEC "+3" tagged `.dsk` such
   as a Chase H.Q. +3 dump).
2. Add an FDC command-trace facility — log every µPD765A command the
   Z80 issues plus the chip's response. Compare against a reference
   emulator's trace of the same disk.
3. Identify where the trace diverges. Likely candidates:
   - A command opcode the chip stub doesn't handle.
   - Status register bits the BIOS polls that never flip.
   - The boot disk's first track layout vs what the BIOS expects.
4. Fix the divergence; re-run the trace; the boot menu's Loader
   should now reach the program.

This is criterion 3 (Formats) territory — DSK parsing is present
but DSK *loading* on +3 isn't end-to-end. Tracking it under that
criterion's existing PARTIAL state.

## 2026-05-10 — partial diagnosis

A temporary FDC command/result eprintln trace, driven by a one-off
test that booted Chase H.Q. (+3) and pressed ENTER on Loader for
3000 frames, captured the BIOS's actual sequence:

```
Specify(0x03 0x03 0x03)         ; pre-menu init
SeekTrack(0xaf 0xaf 0xaf)       ; pre-menu init (note: bogus
                                ;   parameters — junk on the bus
                                ;   from earlier code, but the
                                ;   FDC accepts and replies)
Specify(0x03 0x03 0x03)
SenseDriveStatus(drive=0)        → ST3=0x30 (T0|RY)
=== ENTER pressed ===
SenseDriveStatus(drive=0)        → ST3=0x30
Recalibrate(drive=3)            ; probing all drives
Recalibrate(drive=0)
SenseInterruptStatus            → ST0=0x20, PCN=0  (drive 0 SE)
SenseInterruptStatus            → ST0=0x23, PCN=0  (drive 3 SE)
SenseInterruptStatus            → ST0=0x80, PCN=0  (no more pending)
SenseInterruptStatus            → ST0=0x80, PCN=0
SenseInterruptStatus            → ST0=0x80, PCN=0
…(infinite loop)
```

The BIOS keeps polling SenseInterruptStatus indefinitely **even
after the FDC reports ST0=0x80 (Invalid Command, "no pending
interrupt")**. Per the µPD765A datasheet, ST0=0x80 is the
documented "queue drained" signal. So the BIOS is using a
different exit condition we don't model — most likely either a
status-register bit transition (e.g. drive-busy bits D0B–D3B in
MSR pulsing during seek) or the FDC's INT line going low. Our
FDC's `interrupt` field exists but isn't wired to anything; our
MSR doesn't model the per-drive busy bits.

What landed in `nec-upd765a` from this session — strictly
datasheet-correctness improvements, none enough to unblock the
hang on its own:

- **Per-drive `seek_pending: [Option<u8>; 4]`.** Multi-drive
  recalibrate/seek now queues per-drive interrupts; SenseInt
  walks the drives in order, returning each pending result then
  ST0=0x80 once drained. Replaces the previous behaviour that
  always returned the same cached ST0.
- **Recalibrate without disk fails as Abnormal | Not Ready | EC
  (ST0 = 0xD0 | drive).** Real µPD765A fails recalibrate on a
  non-existent drive (TR0 signal never asserts, step counter
  exhausts). Previously we returned Normal Termination for every
  drive 0-3, so the BIOS thought all four drives existed.
- **SenseDriveStatus ST3 includes HD + US bits from the command
  byte and TS (two-sided) when a disk is present.** Previously
  st3 was missing the head bit and never set TS, which would
  make the BIOS treat every disk as single-sided.
- **MSR drive-busy bits + seek timing.** A new
  `seek_remaining: [u32; 4]` countdown, decremented on every
  `Peripheral::tick`, holds the corresponding `1 << drive` bit
  set in the main status register and defers the seek-end
  interrupt until the countdown hits zero (per FUSE
  `peripherals/disk/upd_fdc.c`). Previously seeks completed
  instantly with no MSR transition for the BIOS to observe.

The infinite SenseInt loop persists with all four fixes in
place: the BIOS still polls SenseInterruptStatus indefinitely
even after MSR's drive-busy bits transition, both seek
interrupts drain, and ST0=0x80 is returned. So whatever exit
condition the BIOS uses isn't `MSR.D0..D3 == 0`, isn't
`ST0 == 0x80`, and isn't multi-interrupt drain count. The next
investigative step is a side-by-side trace against FUSE running
the same DSK — likely needed to identify which status bit (or
the FDC's INT line, which we don't currently route to the Z80
on the +3) the BIOS is actually waiting on.

## 2026-05-10 afternoon — root cause was a Z80 IO bus multi-fire

The infinite SenseInt loop wasn't a missing FDC signal at all.
A single OUT/IN instruction on Z80 holds IORQ + RD (or IORQ +
WR) **active for three consecutive half-cycles** — T2Rise, T2Fall,
T3Rise. The Spectrum bus dispatcher (`handle_bus` in each
machine class) was polling those signals every CPU clock tick:

```rust
} else if self.z80.iorq && self.z80.rd && !self.z80.m1 {
    self.z80.data_in = self.io_read(self.z80.addr);
} else if self.z80.iorq && self.z80.wr {
    self.io_write(self.z80.addr, self.z80.data);
}
```

So `io_read` fired **three times per `IN` instruction**. Each
call consumed and advanced one byte of the FDC's result FIFO,
so the BIOS only ever saw the first byte of a multi-byte status
reply and read 0xFF for the rest. SenseInt's two-byte result
(ST0, PCN) became one byte to the BIOS; ReadData's seven-byte
status block became one. `io_write` had the same triple-fire
shape — every BIOS `OUT (cmd_port), n` was being interpreted
as `OUT n, OUT n, OUT n` by the FDC, which is exactly why the
earlier trace showed bogus parameters like `Specify(0x03 0x03 0x03)`
and `SeekTrack(0xaf 0xaf 0xaf)`. They weren't junk on the bus
— they were the BIOS's intended single-byte command bytes,
multi-fired into command-and-parameter sequences.

**Fix:** introduced `Z80::bus_request() -> Option<BusOp>`
(crate `zilog-z80`). It tracks edge-detection shadows of
`(mreq && rd)`, `(mreq && wr)`, and `iorq`, and returns
`Some(BusOp::…)` on the rising edge of each, `None` thereafter.
All three Spectrum core classes (`48k-class`, `128k-class`,
`amstrad-class`) now dispatch through this single method. The
raw level pins remain on the Z80 for the FUSE corpus tests and
unusual peripherals.

**Effect on Chase H.Q. (+3) trace:**

- The BIOS no longer loops on SenseInt. It drains pending
  interrupts cleanly, issues a `Recalibrate(drive=0)`, then
  `ReadID(drive=0)`, then `ReadData(0x66, drive=0, cyl=0,
  head=0, sector=1, N=2, EOT=1)`.
- Our FDC streams the boot sector's 512 bytes back; the bytes
  are real Z80 code (CD BA FE = CALL $FEBA, 21 CA FE = LD HL,
  $FECA, 3E 04 D3 FE = LD A,4 / OUT (FE),A — the Loader's
  border-flash routine).
- BIOS reads the result phase: `ST0=0x00, ST1=0x00, ST2=0x00,
  C=0, H=0, R=1, N=2` — a clean Normal Termination.
- BIOS now hands control to the loaded boot sector, which
  begins issuing its own FDC commands (PC moves into RAM at
  $FEB8 and similar — that's loaded code, not ROM).

The boot sector executes. The +3 BIOS is no longer stuck.

## 2026-05-10 afternoon — remaining issue: loaded code uses drive 2

The post-fix trace shows a new failure mode. After the BIOS
hands off, the loaded boot-sector code at PC ≈ $FEB8 issues:

```
OUT $3FFD, $46     ; ReadData (MFM, no SK)
OUT $3FFD, $02     ; HD/Drive byte — bits 0-1 select drive 2 (US1=1, US0=0)
OUT $3FFD, $00     ; cylinder
OUT $3FFD, $00     ; head
OUT $3FFD, $02     ; sector 2
OUT $3FFD, $02     ; N=512
OUT $3FFD, $09     ; EOT 9 — multi-sector read
OUT $3FFD, $2a     ; gap
OUT $3FFD, $ff     ; DTL
```

The drive byte's `US1:US0` selects drive 2. The BIOS's own
loader code (PC ≈ $2126) had used drive 0 correctly for the
first `ReadData`. So the loaded code is making a different
choice — and our `insert_disk(0, image)` puts the disk in slot
0, so the FDC returns "no disk" (ST0 = 0x42, abnormal
termination).

The screen sits on the boot menu indefinitely after this
because the boot loader gets the not-ready error, retries a
couple of times, and eventually returns control to the BIOS's
menu loop.

Hypotheses, none yet confirmed:

1. **+3DOS drive numbering quirk** — Amstrad's BIOS may map
   the `A:` drive to FDC US=2 in the boot loader's runtime
   convention even though the BIOS's own ROM code uses US=0.
   Would need a +3 BIOS / +3DOS disassembly or a side-by-side
   FUSE trace to confirm.
2. **Port `$1FFD` drive-select bits we ignore** — the +3 gate
   array (port `$1FFD`) has bits beyond paging (notably bit 3
   = motor on, possibly drive-select bits we don't honour).
   The boot loader may be writing $1FFD with a drive-select
   pattern, then expecting subsequent FDC commands to address
   that drive. Our `MemoryPlus::write_1ffd` handles paging
   only; if we're dropping a drive-select bit, the FDC's
   internal "currently selected drive" wouldn't match the
   command's US bits.
3. **Memory paging bug exposing wrong sector content** —
   PC $FEB8 is in upper RAM (banked $C000–$FFFF range). If
   our paging puts a different bank than the BIOS expected,
   the bytes the loader is executing wouldn't match the bytes
   the loader **wrote** there from the boot sector. A subtle
   off-by-one in `write_1ffd` paging could turn a "drive 0"
   immediate operand into "drive 2" if the surrounding
   instructions were also misaligned.
4. **Disk-image-specific custom loader** — Chase H.Q. (+3)'s
   loader could be using drive 2 deliberately to access a
   "B: drive image" embedded in the same DSK. This would be
   unusual but not impossible.

Hypothesis (2) is the most testable: enumerate every `$1FFD`
write the BIOS makes, compare to the +3 hardware doc's bit
assignments, and see whether we're dropping a drive-select
bit. If that's not it, (1) is next — likely needs a FUSE-side
trace of the same DSK to compare.

## 2026-05-10 afternoon (later) — drive-2 mystery solved by FUSE source

Hypothesis (1) was correct. FUSE's `specplus3_765_init` in
`machines/specplus3.c` documents the wiring quirk explicitly:

```c
/*!!!! the plus3 only use the US0 pin to select drives,
 so drive 2 := drive 0 and drive 3 := drive 1 !!!!*/
specplus3_fdc->drive[0] = &specplus3_drives[ 0 ];
specplus3_fdc->drive[1] = &specplus3_drives[ 1 ];
specplus3_fdc->drive[2] = &specplus3_drives[ 0 ];
specplus3_fdc->drive[3] = &specplus3_drives[ 1 ];
```

The Spectrum +3 board only routes the µPD765A's US0 pin to its
disk selector. US1 is electrically a don't-care, so a drive
byte of `0x02` (US1=1, US0=0) addresses physical drive 0 —
exactly the same as `0x00`. The +3 BIOS's second-stage loader
takes advantage of this: after the boot sector runs, it
issues `ReadData` with US1=1 deliberately (we don't yet know
why — possibly to distinguish second-stage commands from the
first-stage ROM's, or to share the drive-select latch with
some other gate-array bit). On real hardware it Just Works
because of the wiring.

**Fix:** added `Upd765a::set_drive_select_mask(mask: u8)` and
a `drive_select_mask: u8` field. The FDC ANDs the unit-select
bits of every command byte against the mask before indexing
its per-drive state (`disks`, `track`, `seek_pending`, etc.).
On the +3 we set mask = `0x01`; everywhere else the default
`0x03` keeps the full 4-drive standard µPD765A behaviour. The
ST0 / ST3 status bytes still echo the *unmasked* US bits the
BIOS sent, matching real-chip semantics where the chip does
not know how the host's wiring routes them.

## 2026-05-10 afternoon (later still) — multi-sector ReadData

After the drive alias landed, the second-stage loader at
PC=$FEB8 issued `ReadData(R=2, EOT=9)` and got back exactly
one sector of data (512 bytes). The BIOS expected sectors
2..9 (4 KiB) in a single Execution phase, then a Result
phase. Our `read_sector` only ever returned one sector.

**Fix:** in `Command::ReadData`, loop `r = sector..=eot`,
calling `read_sector` for each and concatenating the bytes
into `exec_buf`. Real µPD765A behaviour:

- All sectors found → enter Execution with the full buffer,
  ST0/ST1 = success, Result phase echoes
  `R = last_ok.wrapping_add(1)`.
- A sector goes missing mid-run → still enter Execution
  with whatever was buffered so far, but flag ST0 with the
  abnormal-termination IC and ST1 with No-Data. The host
  reads the partial data, then sees the failure flags in
  Result and retries from the missing sector.
- No sector found at all → skip Execution, go straight to
  Result with abnormal termination.

## 2026-05-10 afternoon — current trace state

After all three fixes (`bus_request` edge dispatch, +3
drive-select mask, multi-sector ReadData) the diagnostic
trace for Chase H.Q. (+3) now shows:

- 0 frames in the boot menu loop ($1800 page) — the BIOS
  has handed off completely.
- 81 % of frames in RAM pages $B100 / $8200 — loaded game
  code.
- µPD765A streaming through SeekTrack → ReadID → ReadData
  cycles across multiple cylinders, hundreds of OUTs and
  thousands of data-byte reads.
- Multi-sector ReadDatas at PC=$822F and PC=$FEB8 (loaded
  code) — each fetching 4 KiB of program data per command.
- Two SenseInt drains per Recalibrate, no spurious infinite
  polling loops.

**Visual confirmation:** the diagnostic test now dumps the
framebuffer to `/tmp/plus3_disk_trace.png`. After ENTER on the
Loader and ≥1000 frames the PNG shows a black main screen with
multi-colour stripey borders — the classic Spectrum loader
"progress stripes" pattern, written by the loader's `INC A /
OUT (FE),A / JR -5` border-cycle loop.

**The loader is hung in its error path, not the active load
path.** Wider memory dumps reveal the actual code the loader
is stuck on. At `$B197`:

```
$B197  LD A, ($B269)     ; expected magic byte
$B19A  CP (IX+1)         ; compare against just-loaded data
$B19D  RET Z             ; match → continue loading
$B19E  LD BC, $1FFD      ; mismatch → motor off
$B1A1  LD A, $00
$B1A3  OUT ($1FFD), A
$B1A5  INC A
$B1A6  OUT (FE), A       ; stripey border (error hang)
$B1A8  JR  $B1A5         ; loop forever
```

So the loader has detected that the data we delivered doesn't
match an expected check byte and dropped into a deliberate
hang. CPU state at the hang confirms it: `iff1 = false` (IRQs
masked), so no service routine can unstick it.

The likely culprit is **`ReadID` returning hardcoded `C=track,
H=0, R=1, N=2`** instead of the actual sector header recorded
on the disk. Trace shows the loader does `SeekTrack(track=7) +
ReadID` (PC=$B23E) just before falling into the error path —
on a Speedlock-protected DSK that track will have non-standard
CHRN values, which the loader uses as part of its key/CRC
verification. Our `format-amstrad-dsk` parser keeps per-sector
header data, but the FDC's `ReadId` arm currently doesn't
consult it. Plumbing the disk image's actual recorded CHRN
back through `Command::ReadId` is the next obvious step. After
that, look at multi-sector reads using the FDC's *internal*
current-cylinder (set by Seek) for header verification rather
than `cmd_buf[2]`, in case the loader is sending mismatched
cyl headers across cylinders.

This means criterion 3 (Formats) for the +3 should move from
PARTIAL → DONE for `.dsk` / `.edsk` once a catalogue entry
that actually exercises the +3 disk path lands. Plan: pick
2–3 +3 DSKs (Chase H.Q., Daley Thompson's Olympic Challenge,
Robocop are all in TOSEC), author catalogue entries for each
with frame/audio hash assertions tuned to a deterministic
reproduction window, and run them under the same harness as
the 51 existing tape-based entries.

## 2026-05-11 morning — catalogue survey results

`runtime-sinclair-zx-spectrum/tests/plus3_disk_survey.rs` boots
each of ten TOSEC +3 DSKs with the autoload helper, runs 6000
frames after pressing ENTER on the Loader menu, and dumps a PNG
plus frame/audio xxh64 to `/tmp/plus3_survey_*.png`. Results:

| Title (publisher / protection) | Outcome |
| --- | --- |
| Chase H.Q. (Ocean / Speedlock 7+) | **Full title screen.** Logo, cops, OCEAN logo, copyright lines for Ocean Software and Taito Corporation. |
| Rainbow Islands (Ocean / Speedlock) | **Credits screen.** "RAINBOW PROJECT / PROGRAM BY DAVID O'CONNOR / GRAPHICS BY JOHN CUMMING / PRODUCED BY GRAFTGOLD LTD" with the 1UP / HI SCORE / CREDIT header. |
| Cybernoid (Hewson / custom) | **Main menu.** Logo, "BY RAFFAELE CECCO / MUSIC BY DAVE ROGERS", 5-option control menu. |
| Cybernoid II (Hewson / custom) | **Pre-game key blurb + Loader bar.** "NOTE KEY CHANGES / Y = SMART BOMBS / U = TRACERS / PRESS SPACE". |
| Saboteur II (Durell / speed-up) | **Loader title bar.** "Saboteur 2 (Speed up)" in blue. Also the only +3 entry whose audio buffer captures non-silence — the Durell loader is pulsing the AY. |
| Operation Wolf (Ocean / Speedlock) | ❌ Gray screen with "© 1982 Amstrad" — same exact framebuffer hash as RoboCop and Where Time Stood Still. The three Ocean Speedlock titles fail at the same state. |
| RoboCop (Ocean / Speedlock) | ❌ Same gray-Amstrad state as Operation Wolf. |
| Where Time Stood Still (Ocean / Speedlock) | ❌ Same gray-Amstrad state. |
| Turrican (Rainbow Arts) | ❌ Black screen. The loader runs but doesn't paint anything visible within 6000 frames. |
| Tetris (Mirrorsoft) | ❌ DSK parse error: "Track 12 (offset 58624): sector ID 0x07 runs past track block (need 8192 bytes at offset 14464)." Image is structurally invalid or uses a track-block layout `format-amstrad-dsk` doesn't yet handle. |

So five titles land on stable, recognisable screens and are
authored as catalogue entries in `manifest/spectrum.toml`. The
five failures split into three independent unsolved problems:

1. **The three Ocean Speedlock titles that share a hash with the
   "© 1982 Amstrad" empty BIOS screen.** The fact that they all
   stop at *exactly* the same framebuffer state means there's a
   common code path the loader takes when a verification step
   fails — probably an `OUT $1FFD` that resets the gate-array
   into a state where the lower 32K is empty RAM and the BIOS's
   © message is the only thing visible. Worth a side-by-side
   trace against FUSE to find which check is failing.

2. **Turrican's silent black screen.** The loader either runs
   off into unreachable code or successfully decrypts data and
   then waits for an interrupt-driven trigger we're not firing.
   Same diagnostic-test setup as Chase H.Q. should reveal the
   PC hot spot.

3. **Tetris's DSK parse error.** A `format-amstrad-dsk` bug, not
   an emulation issue — the parser uses the track's default N as
   the data length when the EDSK per-sector length is zero, but
   this image has zero data lengths in the SIL combined with a
   N=2 default that exceeds the actual block. Either the image
   is malformed (which several TOSEC entries are) or the parser
   needs to fall back to the actual sector positions recorded in
   the SIL.

## 2026-05-11 morning — ST1/ST2 + DDAM + EDSK zero-length fixes

Three independent fidelity gaps, all surfaced by tracing failing
catalogue titles. Committed in `caaa9df`.

### Discovery: every TOSEC +3 DSK is actually EDSK

Probing the file signatures of the survey set:

```
$ xxd -l 21 chase-hq.dsk
00000000: 4558 5445 4e44 4544 2043 5043 2044 534b  EXTENDED CPC DSK
00000010: 2046 696c 65                              File
```

…and every other +3 DSK in TOSEC for the protected titles looks
the same. Standard DSK simply can't represent the metadata that
protection schemes rely on (per-sector ST1 / ST2, variable
length sectors, format-track-only sectors), so dumpers default
to EDSK. The parser already handles EDSK; this realisation just
clarifies that *all* the fidelity work below is on the
"extended" path — the standard-DSK fallbacks below it are kept
for the rare modern non-protected `.dsk` someone might author.

### Bug 6: DDAM (Read Deleted Data) unimplemented

Operation Wolf at PC=`$FEFE` issues `OUT $3FFD, $4C` after the
BIOS-driven first read finishes. `$4C` is `ReadDeletedData`
(opcode `0x0C` with MFM=1, SK=0). Our `decode_command` only
matched `0x06` (ReadData), so the opcode fell into the
`Command::None` arm and the FDC silently dropped the command.
Speedlock writes protection-key sectors with a deleted data
address mark (DDAM) and reads them back via this opcode; with
the chip ignoring the request the loader got back garbage and
gave up.

`Command::ReadDeletedData` now shares the ReadData arm — the
data-delivery path is identical, the difference is *which*
address-mark type the chip is willing to match.

### Bug 7: per-sector ST1 / ST2 dropped at parse time

The DSK SIL stores eight bytes per sector: C, H, R, N, ST1,
ST2, length_lo, length_hi. The parser was reading C/H/R/N and
the length, then discarding the two status bytes. Those bytes
are exactly what the chip would have returned reading the
sector at dump time — ST1.DE (data CRC error), ST2.CM (deleted
mark), ST2.DD (data field CRC error), ST1.ND (no data), and so
on. Speedlock-protected disks deliberately record sectors with
these bits set so a loader can verify "this is an original
disk, not a copy from a tool that only knows about clean
sectors."

DiskSector now carries `st1` and `st2`. The ReadData arm OR's
each delivered sector's recorded ST1/ST2 into the result-phase
status block, so a loader reading a sector with recorded DE
sees the same DE flag the real drive would have produced.
Multi-sector reads terminate the moment they hit a recorded
CRC error (real chip behaviour: it stops on the first sector
where the data field's CRC mismatches), with ST0's
abnormal-termination IC set.

### Bug 8: EDSK zero-length sector treated as N's worth

`Command::ReadData`'s parser fallback for "SIL length == 0"
was 128 << N. Right on standard DSK where the SIL length
column is unused; wrong on EDSK where length=0 specifically
means "the dumper saw the address mark but couldn't or wouldn't
capture data bytes." Tetris's track 12 is a format-track
protection where sectors 7..15 are address-mark-only with N
values up to 7 (claimed 16 KiB per sector); the parser
computed 16384 bytes for sector 7 and walked off the end of
the 14592-byte track block, panicking with "sector ID 0x07
runs past track block."

The parser now keys off the disk's extended-vs-standard
signature: on EDSK, `length == 0` is taken at face value (the
sector enters the image with an empty `data` Vec and the
recorded ST1/ST2 flags tell the host what state the chip
would see). On non-extended DSK we still fall back to track
default size.

### Effect on the survey

| Title | Before | After |
|---|---|---|
| Chase H.Q. | Title screen | **In-game options scoreboard** with AY music active |
| Operation Wolf | "© 1982 Amstrad" BIOS empty | Loader stripes (same new hash as RoboCop / WTSS — still hung at a later check) |
| RoboCop | "© 1982 Amstrad" | Loader stripes |
| Where Time Stood Still | "© 1982 Amstrad" | Loader stripes |
| Tetris | DSK parse panic | Parses cleanly; loader runs, no visible output yet |
| Rainbow Islands / Cybernoid I & II / Saboteur II | already passing | unchanged |

So one of the three Speedlock failures (Chase H.Q.) progressed
all the way to "ready to play"; the other three moved past the
BIOS empty-screen state into the loader's progress display but
hang at a later Speedlock check. Their identical post-fix hash
(`5b942a2bc9de21c2`) means they're all hitting the same next
bug — narrowing the next investigation.

### Outstanding +3 disk diagnostics

- ~~**The "loader stripes" Speedlock-6 cluster.**~~ **RESOLVED
  2026-05-12** via the marginal-encoding model
  (`wiki/decisions/marginal-encoding-model.md`). The actual
  failure mode wasn't a status-byte shape mismatch — it was a
  weak-sector check. Op Wolf's loader (and the other three
  titles in the cluster) re-reads track 0 sector 2 indefinitely,
  expecting the bytes to differ between reads because Speedlock
  wrote that sector with deliberately marginal magnetic
  encoding. Real silicon returns different bytes each read; our
  chip was returning the same bytes. An audit of the +3
  reference library found zero EDSKs with multi-copy weak-sector
  data preserved, so we model the chip's marginal-encoding
  behaviour deterministically on any sector whose recorded ST1.DE
  or ST2.DD is set. Op Wolf, RoboCop, Where Time Stood Still,
  and Bad Dudes vs Dragon Ninja all load past the protection
  check after the fix.
- **Turrican's / Tetris's black-screen loaders.** Loaders run,
  PC moves through loaded RAM, but nothing visible paints
  within 6 000 frames. Both share `xxh64:99bf46ee0b35abc0` (an
  all-black framebuffer that hashes to the same value as the
  Spectrum's clear-screen state). May be waiting on an
  interrupt-driven event or stuck in self-modifying code.
  Probably independent of the Speedlock-6 issue.

### Catalogue coverage after 2026-05-11

Ten +3 disk entries now in `manifest/spectrum.toml`, picked to
exercise distinct loader code paths:

| Entry | Publisher / loader | Captured state |
| --- | --- | --- |
| chase-hq-plus3 | Ocean / Speedlock 7+ | In-game options scoreboard, AY music live |
| rainbow-islands-plus3 | Ocean / Speedlock | Graftgold credits screen |
| operation-wolf-plus3 (todo) | Ocean / Speedlock 6 | loader stripes — hangs at sector-2 check |
| robocop-plus3 (todo) | Ocean / Speedlock 6 | same |
| where-time-stood-still-plus3 (todo) | Ocean / Speedlock 6 | same |
| dragon-ninja-plus3 (todo) | Imagine / Speedlock 6 | same |
| cybernoid-plus3 | Hewson / custom (early) | Main menu |
| cybernoid-2-plus3 | Hewson / custom (early) | Pre-game blurb |
| stormlord-plus3 | Hewson / Alkatraz (late) | Title + control select |
| saboteur-ii-plus3 | Durell / speed-up | Loader title bar |
| sim-city-plus3 | Infogrames / plain +3DOS | Difficulty-select screen — the unprotected coverage anchor |
| starglider-2-plus3 | Rainbird | Title screen (filled-vector ship) |
| lotus-esprit-plus3 | Gremlin | In-car dashboard |
| combat-school-plus3 | Ocean (pre-Speedlock, 1987) | Compilation menu |

Seven distinct loader code paths covered. One genuine
"unprotected +3DOS LOAD" anchor (Sim City) so a regression in
the BIOS filesystem layer surfaces independently of any
copy-protection machinery.

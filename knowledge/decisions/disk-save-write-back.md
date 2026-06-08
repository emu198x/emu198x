# Disk/tape SAVE write-back — read-only archive, explicit writable work image

**Status:** Decided 2026-06-07; implementation in progress. Driven by the
Code198x "Meet C64 BASIC" primer needing a real `SAVE`/`LOAD` round-trip
(unit 14, "Keeping Your Work").

## The problem

The cores can `LOAD` from disk but cannot persist a `SAVE`. On the C64 the
cycle-accurate 1541 presents a **read-only** D64-backed GCR layer
(`Drive1541::write_protected` hardcoded `true`); the write-head value is
captured at the VIA2 port but never applied to the track surface, and nothing
flushes back to the host file. A `SAVE"X",8` prints `SAVING X` / `READY.` but
writes nothing, so a `LOAD"X",8` afterwards gives `?FILE NOT FOUND`. The
Spectrum has the same gap on every path: tape recording is unimplemented
(playback only), the +3 µPD765A `WriteData` execution phase is a stub, and the
Beta-disk WD179x returns `ST_WRITE_PROTECT` for all writes.

## The decision

**1. Archive media is immutable. It mounts write-protected — always.**
Everything under `198x/assets/` is DAT-verified preservation material
(Cat198x's remit). A learner `SAVE` must never alter it. Real hardware already
modelled this: the **write-protect tab**. Preservation = tab on. So an archive
mount sets the drive's write-protect sense, and a `SAVE` to it yields the
authentic `?WRITE PROTECT ERROR` (C64) / write-protect status (Spectrum) —
a true, teachable behaviour, not a fudge.

**2. The learner's SAVE targets a separate, explicitly-writable work image.**
Writability is a property of the mount, **default false**. `load_media` gains a
`writable` flag. Archive disks mount read-only; the curriculum (or a learner)
explicitly creates and mounts a writable scratch `.d64` / `.tap` — a blank disk
of their own, exactly as a real user kept beside their shop-bought games — and
that work image is the *only* thing write-back ever touches.

**3. Source bytes are never mutated in place.** The core works on its in-memory
image and flushes to the writable file on demand; it does not stream edits into
the file the host handed it. (Chosen mechanism is the explicit writable flag,
not a copy-on-write overlay — see the rejected option below.)

**4. Parity across cores.** The same model applies to C64 (1541), Spectrum
(tape + +3 + Beta), and any future disk/tape core. The write-protect-tab framing
and the writable-work-image rule are platform-neutral.

## Why not the alternatives

- **Auto blank work disk per session** — friendlier (SAVE "just works"), but
  hides where the bytes live and couples persistence to session lifecycle.
  Rejected for the explicit flag: a learner who types `SAVE` should be able to
  point at the disk it lands on.
- **Copy-on-write overlay on any mount** — most uniform, never touches source
  bytes, but "where did my save go?" is fuzzier and overlays are a new concept
  to teach. The explicit writable flag keeps the model concrete: this disk is
  writable, that one is not.
- **A KERNAL/ROM SAVE trap** — would violate the cycle-accurate drive design
  (the 1541 ROM does the BAM/directory/sector work itself over the serial bus).
  The authentic path is to make the physical GCR surface writable and let the
  ROM do its job; persistence is then GCR→sector decode + host flush.

## Implementation shape (C64 first)

The cycle-accurate 1541 already scaffolds most of this: `is_read_mode()`,
`write_protect_not_asserted()` (reads `disk.write_protected()`), the
`gcr_write_value` capture, `gcr_head_offset`, and byte-ready timing all exist.
The materialized per-track GCR buffers (`tracks: Vec<Vec<u8>>`) are built from
the D64 by `encode_sector_to_gcr`; the inverse GCR→sector decoder currently
lives only in tests.

1. **`format-commodore-c64-d64`** — public `write_sector` + a GCR→sector
   decode-back path (promote the test decoder), so a modified track buffer can
   be turned back into D64 bytes. Pure, unit-testable.
2. **`machine-commodore-1541`** — configurable `write_protected` (from the
   mount), apply `gcr_write_value` into the current track buffer at the head
   position during write mode, mark tracks dirty, expose a flush that returns
   updated D64 bytes.
3. **`runtime-commodore-c64` + `load_media`** — thread a `writable` flag
   through the mount; on flush/detach write the D64 bytes back to the host file
   for writable mounts only.
4. **`emu198x-c64` MCP** — `load_media` gains the `writable` param.

Spectrum parity (tape record → `.tap`/`.tzx`; µPD765A `WriteData`; Beta
`WriteSector`) follows the same writable-flag gating as a separate effort.

## Implementation status (2026-06-07)

**Done + unit-tested (C64 path, increments 1–4):**
- `format-commodore-c64-d64`: public `write_sector`.
- `machine-commodore-1541`: configurable `write_protected`; `load_d64_bytes_writable`;
  GCR decoder promoted out of tests; `flush_image` (whole-surface GCR→D64,
  proven a byte-exact round-trip on an unwritten disk); write-mode rotation +
  `write_one_track_bit` (latch byte lands on the surface, protected disks drop
  writes — both unit-tested).
- `emu198x-shell`: `MediaImage::writable` + `LoadMedia.writable` (serde default
  false) + `load_media` tool schema gains `writable`.
- `runtime-commodore-c64`: mount honours `writable`; `flush_drive8_image`.
- `emu198x-c64`: `save_disk` MCP tool (flush drive 8 → host `.d64`).

**Blocked at end-to-end — but the block is upstream of this work.** The
`#[ignore]` integration test `crates/runtime-commodore-c64/tests/disk_save_roundtrip.rs`
drives a real `SAVE"GREETING",8` (real ROMs) and flushes the surface. Full
instrumented diagnosis (2026-06-07):

- The C64 reports the SAVE complete: screen shows `SAVING GREETING` then `READY.`.
- **The write-back surface works.** Write mode engages, ~29,160 bits land on the
  surface, `track_bytes_mut` resolves every time (0 misses), and **18 of track
  18's 19 sectors decode cleanly** from the live GCR. The GCR is real and
  structured, not garbage. So `write_one_track_bit` + `flush_image` + the decoder
  are correct.
- **Yet nothing persists.** The flushed image is byte-identical to the blank
  disk (0 bytes changed), and the head's last (only) written track is 18
  (`head=36`) — it **never seeks to a data track to write the file's bytes**.
  The directory sector (18,1) stays the blank `00 FF 00…`.

**Root cause, pinpointed: the SAVE data never reaches the drive.** Peeking the
1541's RAM right after the SAVE is decisive:
- `$0500` holds the directory entry under construction — the PETSCII bytes
  `47 52 45 45 54 49 4E 47` = **"GREETING"** — and `$0700` holds the BAM. So the
  **OPEN/filename path works** (command channel + directory/BAM handling).
- But the program's data bytes are **absent from all drive RAM** — neither `"HI"`
  (`48 49`) nor the `PRINT` token (`$99`) appears anywhere in `$0300–$07FF`.

So the drive gets the filename, allocates, builds a directory entry — then waits
for file data that never arrives. No data sector is written, the directory entry
is never finalised, and the C64 still prints `READY.`. The failure is the **IEC
serial C64→drive bulk data transfer**: the C64-talker → drive-listener path for
non-ATN data bytes during the SAVE data phase. (LOAD — drive talks, C64 listens —
works; the command/filename listen path works; only the drive's *bulk data
receive* is missing/broken.)

This is **not** the 1541 job loop and **not** the GCR write-back built here — both
are downstream of data that never lands. It is a focused IEC-serial effort: the
bit-level CLK/DATA handshake for the drive receiving a stream of data bytes
(including the EOI/turnaround), at `common-commodore-iec` + the C64 CIA2 serial
lines + the 1541 ROM's listen loop. The integration test is the repro — fast,
deterministic, real ROMs; add a drive-RAM peek to watch for the data landing.

### Sharper diagnosis (2026-06-07, second pass)

Per-track write instrumentation + PC histograms during the SAVE pin it down
further:

- The head writes **only track 18** (the directory/BAM track; ~29k bit-writes),
  **never a data track** — so the file's data sector is never even attempted.
- The C64 returns to its READY input loop (`$E5CD`) almost immediately — it does
  **not** wait the ~1s a real disk write takes. So the C64's data send
  "completes" without the drive ever performing a data write.
- Sequence: filename/OPEN works (drive idle and listening → builds the dir entry
  on track 18); then the drive goes busy doing OPEN disk I/O on track 18; when
  the C64 starts the **data-channel** (`SECOND $61`) transfer, the drive isn't
  servicing the serial bus, so the C64 clocks the data out to no listener and
  proceeds.

**Hypothesis to test first:** the drive, while busy with the OPEN's disk I/O,
isn't holding the serial bus (DATA low = "not ready") or isn't servicing the
data-channel `LISTEN`/`ATN`, so the C64 talker races ahead. Look at how the 1541
interleaves disk-controller work with serial-bus service (ATN handling /
hold-off) and whether the emulator's CPU-interleave + `IecBus` bus state during
the drive's busy window matches hardware. Compare the working LOAD path (drive is
talker, paces the bus) with the failing SAVE data path (drive must listen while
busy). The suspicious `recompute_drive_bus_entry` ATN-acknowledge fold is worth
auditing against VICE.

## Drift triggers

- "Just write the SAVE back to the mounted image" — **stop.** Never write to an
  archive image. Writability is opt-in per mount; archive = read-only.
- "Add a SAVE/KERNAL trap to shortcut it" — **stop.** Authentic GCR-surface
  write + decode-back, not a ROM trap.
- "Default `writable` to true so it's convenient" — **stop.** Default false;
  the work image is mounted writable explicitly.
- Treating `assets/` as scratch space for any test/capture — **stop.** Work
  images live outside the archive tree.

## Next-session execution plan — the IEC data-phase handshake (2026-06-08)

Resourced and ready: working repro, VICE 3.10 vendored, hardware refs staged.
This plan is the turnkey start for a focused session.

### What we know (settled)
- GCR write-back + decode + flush are **done and tested** — downstream of data
  that never arrives. Do not touch them.
- The C64↔1541 **interleave is sound**: `runtime-commodore-c64/src/runtime.rs`
  `run_until` (~L501-516) is a rational-clock model — it ticks whichever CPU is
  next due by true relative Hz (`next_drive_tick <= next_c64_tick`), then
  `sync_iec_bus` propagates bus state. Relative timing is therefore unlikely to
  be the root cause.
- The failure is the **C64-talker → drive-listener BULK DATA receive** during the
  SAVE data phase (after `SECOND $61`). OPEN/filename (drive idle, listening)
  works; the drive builds the dir entry on track 18, goes busy with disk I/O,
  and the C64's subsequent data bytes never land in drive RAM. C64 returns to
  READY (~$E5CD) without the ~1s a real write takes.

### Prime suspect (the crux)
The CBM serial **byte handshake** + the drive holding **DATA low while busy**:
- Protocol (see refs): talker holds CLK low; listener signals "ready to receive"
  by releasing DATA high; talker releases CLK; listener pulls DATA low to ACK the
  byte. A busy device keeps DATA low so the talker WAITS. If our drive releases
  DATA (looks ready) while busy, the C64 clocks bytes to no listener and proceeds.
- The hardware **ATN-acknowledge** (VICE "ATNA"): drives pull DATA low in response
  to ATN via a gate. Our equivalent is the `IecBus::recompute_drive_bus_entry`
  fold (`common-commodore-iec/src/lib.rs:128-132`) — flagged suspicious. Compare
  bit-for-bit against VICE `iec_update_cpu_bus` + the drv_bus/ATNA logic.

### Ranked hypotheses to test
1. **H1 — drive doesn't hold DATA low while busy.** During the OPEN's track-18
   I/O the 1541 ROM should keep DATA asserted (not-ready) until its main loop
   returns to serial service. Audit our VIA1 PB → `write_drive_port_b` mapping
   and whether the ROM's DATA output is actually reflected on the bus in the busy
   window. (Most likely.)
2. **H3 — the `recompute_drive_bus_entry` ATN/DATA fold is wrong**, so the
   C64-visible DATA-in during the data phase is computed incorrectly. Diff
   against VICE `iecbus.c`.
3. **H4 — the drive never enters the data-channel listen state** (SECOND $61),
   so it isn't set up to receive at all (an upstream handshake/timing miss).
4. **H2 — bus sync ordering** in `run_until` (which side updates the bus vs syncs
   first) drops a transient edge. Lower probability given the rational clock.

### Instrumentation (extend the repro)
`crates/runtime-commodore-c64/tests/disk_save_roundtrip.rs` is the repro (~28s,
ROMs at `~/.emu198x/roms/commodore-c64/`). Add a per-tick trace during the data
phase capturing: ATN/CLK/DATA (both `cpu_port` and `drive_port` views of IecBus),
the C64 KERNAL PC (the serial-send routine ~`$ED40`/`$EDAD`), and the 1541 PC.
Build the handshake sequence and diff against the protocol (and, if needed, a
VICE run with `-verbose`/IEC debug). Pinpoint the exact edge where the C64 sends
without the drive holding DATA / acknowledging.

### Reference cross-checks (staged in `~/bitsavers-staging/pdf/commodore/`)
- **VICE** `emulators/c64/vice-3.10/src/iecbus/iecbus.c` (`iec_update_cpu_bus`,
  ATNA) + `src/drive/iec/iec.c` — the authoritative bus + drive model to match.
- **pagetable "Commodore Peripheral Bus Part 4 — Standard Serial"** (Steil) +
  **Butterfield "How the Serial Bus Works"** — the byte handshake + EOI + the
  busy/ready DATA semantics.
- **Anatomy of the 1541** — the drive ROM's serial listen loop addresses.
- **R6522 VIA datasheet** — PB/CB2 serial-line behaviour in the 1541.

### Fix + validation
Fix the responsible layer (most likely the IecBus DATA-hold/ATNA fold or the VIA1
mapping; possibly the sync ordering). Validate with:
- A new **fast IecBus unit test** asserting the data-phase handshake states
  (drive-busy ⇒ DATA held low ⇒ C64 sees not-ready), so the regression is cheap.
- The repro test going green (a readable `GREETING` dir entry + extractable PRG).
Success criterion: `disk_save_roundtrip` passes; then the GCR→D64 flush already
in place persists the file.

## Session 2 — diagnosis corrected (2026-06-08)

Re-instrumented the repro with an env-gated IEC line trace (log
`cpu_bus`/`cpu_port`/`drive_port`/`drive_bus[8]` + both CPU PCs on any change,
in `runtime.rs::run_until`) + a post-SAVE drive-RAM dump. Findings **revise** the
2026-06-07 diagnosis:

- **The serial bus is ACTIVE during the SAVE** — not dead. Both CPUs spend ~6M
  cycles in their serial routines (C64 in the KERNAL `$ED..`/`$EE..` send/receive,
  drive in `$E8xx` ATN handler + `$E9C9` receive + `$EAxx` bit loops + `$EA53`
  idle). The "C64 returns to READY almost instantly" framing was wrong.
- **OPEN fully works.** Drive RAM `$0500` holds the dir entry under construction:
  `00 FF 02 11 00 "GREETING" A0…` — file type **`$02` = UNCLOSED PRG** (bit 7
  clear; a closed file is `$82`), data pointer **track 17 / sector 0**. BAM is
  loaded (`$0700`: `12 01 41 …`). ATN releases correctly for data phases.
- **The ATN IRQ works.** The drive's ATN handler (`$E8xx`) runs even while busy —
  so "drive busy, ignores ATN" is disproven. ATN→VIA1 CA1 is wired (machine.rs
  `sync_iec_bus`), the fold matches VICE, the interleave is a rational clock.
- **The program DATA never reaches the drive's data buffer** — no `$99` PRINT
  token, no `48 49` "HI" anywhere in drive RAM `$0300-$07FF`. The file is left
  **unclosed**, so nothing is committed and (separately) the dir sector the drive
  does write to track 18 doesn't carry a closed entry.
- **~6M cycles of repeated command phases** (the C64 re-issuing under ATN) — the
  data-channel **per-byte talker→listener handshake fails and the C64 retries**.

**Ruled out this pass:** IecBus DATA/ATNA fold (byte-identical to VICE
`drv_bus[8]`), CPU interleave, ATN release, ATN→CA1 IRQ delivery.

**Sharper root cause:** the **data-channel per-byte handshake** (C64 `ISOUR`
send-byte ↔ the 1541 listener receive) fails — the byte is clocked but the
drive's ready/ACK (DATA-line) handshake doesn't route the byte into the file
channel's buffer. Likely a per-byte DATA-ready/ACK timing edge specific to the
data channel (the *command* bytes route fine; the *file-channel data* bytes do
not).

**Next step (reference-ready):** with the *Anatomy of the 1541* ROM disassembly,
identify the drive's data-channel listener-receive routine and the buffer-routing
(channel/secondary-address dispatch), and trace one data byte end-to-end against
the C64 `ISOUR` ($EDDD-ish) handshake to find the broken edge. The env-gated IEC
trace + drive-RAM dump (≈25 lines, reverted) are the tools — re-add them.

## Session 3 — root cause found: GCR write-verify failure, NOT IEC (2026-06-08)

**The IEC-serial framing (Sessions 1–2) was a symptom, not the cause.** Drove the
*Anatomy of the 1541* ROM disassembly against an env-gated drive-PC probe in
`runtime.rs::run_until` (peek drive RAM + regs at sync; ≈30 lines, reverted).
Followed the discarded data backwards through the 1541 DOS and reached the real
defect — in the **disk layer**, three levels below the serial bus.

### The chain (each step proven, not inferred)
1. **Data discarded at the listener dispatch.** The 1541 receive-data routine
   `$EA2E` only stores incoming bytes if `$D107` reports the channel *active for
   write* (carry clear) **or** the command is OPEN (`$F0`). In the SAVE data phase
   (command `$60`, SA 1), `$D107` returns **carry set — channel inactive** → it
   falls to `$EA4E`, releases the bus, and drops every data byte. (The filename
   survives only because OPEN takes the `$F0` special-case path.)
2. **The channel is inactive because the OPEN closed it.** `$022B+SA` (the
   channel-association table; `$FF` = free) is allocated for SA 1 (`$022C` → `$81`
   at `$D1F8`) then freed (`$FF` at `$D240`) **during** the OPEN. The free is the
   normal close routine `$D22E`, reached from `$D313` ("close channels of this
   drive") called at `$D045` inside the OPEN-file routine `$D042` — **no DOS error
   abort** (`$C1C8` never runs).
3. **The OPEN-file routine `$D042` runs twice, 5.2M cycles apart** (≈5 s of drive
   time for an operation that should take ~150k cycles). The second pass closes
   the channel; the data phase arrives 164k cycles later, before any re-open.
4. **The 5M cycles are disk-read thrashing.** By the data bail: **586 SYNC
   searches** (`$F556`) and ~24 disk jobs, on a freshly-formatted blank disk that
   should need a handful. The DOS's believed track (`$22`) tracks the physical
   head correctly and the stepper phase is consistent (`VIA2 PB&3 == (head-2)&3`),
   so **seeking is NOT the bug**.
5. **Root cause: the drive's own read-after-write VERIFY fails.** Job-completion
   (`$F969`, result code in A) on track 18 returns mostly **code 7 = "25 WRITE
   VERIFY ERROR"**, interleaved with a few OK. The 1541 writes a directory/BAM
   sector, reads it back to verify, and the read-back **does not match what it
   wrote** → retries forever → the file-create never completes. (Code 3 = "21 no
   sync" appears only when the head momentarily drifts to the odd half-track 37,
   where only even track-slots carry GCR — a secondary symptom, not the driver.)

### Why this was never caught
The only disk-read test through the real drive (`disk_autoload.rs`) asserts the
screen reaches `LOADING` — it never verifies a *completed* read. The GCR
write-back was "tested" only via the lenient `flush_image` GCR→D64 decoder
("18 of 19 sectors decode"), **never against the drive ROM's strict
read-after-write verify**. So the write path looked done but cannot survive the
1541's own verify pass.

### The actual fix target (next session)
The bug is in `machine-commodore-1541`'s GCR **write path** — `write_one_track_bit`
+ the write/read byte-ready cadence (`rotate_one_track_bit`, `schedule_byte_ready`,
`rotation_ref_phase`) — and/or SYNC/byte-boundary alignment between what the ROM
writes and what the read path reassembles. The drive must be able to **write a
sector and read it back identically** (header checksum + data-block checksum +
SYNC intact). Build the repro at the **unit level**: drive the ROM's WRITE (job
`$90`) then VERIFY (job `$A0`) on one sector and assert code 1, OR a pure
`write_one_track_bit` → read-path round-trip on a synthetic sector. That turns the
30 s integration test into a sub-second loop. The IEC serial bus, the channel
dispatch, the seek/stepper, and the GCR→D64 flush decoder are all **correct and
downstream** — do not touch them.

**Supersedes the IEC framing above.** The data-channel handshake was never the
problem; it is the visible end of a disk-write-verify failure. Drift trigger:
"the SAVE data bytes don't reach the drive — fix the IEC handshake" → **stop**,
the bytes are dropped because the write channel was closed by a thrashing OPEN
that can't pass its own write-verify; fix the GCR write path.

## Session 4 — Session 3 fixed; data block now the front line (2026-06-08)

### What was fixed
The Session 3 root cause is **resolved**. `machine-commodore-1541`'s write path
emitted each surface bit by indexing the `gcr_write_value` *port* live
(`bit = (gcr_write_value >> (7 - write_bit_index)) & 1`). The DOS write loop runs
**one byte ahead in the pipeline** — it stores the next byte into the port partway
through the current byte's emission — so live indexing spliced the next byte into
the byte already going onto the surface, failing the drive's own read-after-write
verify (job code 7).

Fix: model the serialiser as the hardware does (and as VICE `rotation.c` lines
537–556 do) — a **shift register** (`write_shift`) that emits its MSB and shifts
left each bit, reloading from the `gcr_write_value` **latch** only at the byte
boundary. A mid-byte store can no longer corrupt the byte in flight. The read
branch keeps `write_shift` pre-loaded with the latch so a write phase starts
aligned. New unit test `write_latch_ignores_mid_byte_store` (red→green); existing
`write_mode_lays_the_latch_byte_onto_the_surface` seeds the serialiser as a
boundary load would. All 66 crate tests green. (Committed — see git log.)

**Cross-check used:** VICE `emulators/c64/vice-3.10/src/drive/rotation.c` — the
write branch reloads `last_write_data = GCR_write_value` only at `bit_counter == 8`.

### Proven effect on the real SAVE
`disk_save_roundtrip` advanced from **"no directory entry written"** (the
`GREETING` assertion failed) to **"`GREETING` directory entry written to track
18"**. The track-18 directory/BAM sector now passes the drive's read-after-write
verify — the write path itself is fixed.

### The new front line — the file is left UNCLOSED (traced, not inferred)
Re-instrumented `runtime.rs::run_until` with an env-gated (`EMU_SAVE_TRACE`)
drive-PC probe over the receive dispatch (`$EA2E`/`$EA44`/`$EA4E`) and job
completion (`$F969`), plus sector dumps of the flushed image. Findings (probe
reverted):

1. **Data IS received and buffered.** In the data phase the channel is active
   (`$022C` = `$81`, SA `$61`) and the program bytes flow through `$EA44`
   (`JSR $E9C9` receive) → `$EA47` (byte in A: `01 08 0C …` = the `$0801` load
   address + tokenised program) → `$EA48` (`JSR $CFB7`, the "write data byte in
   buffer" routine). **No bytes take the `$EA4E` bail.** Session 3's "bytes
   dropped at dispatch" no longer applies post-fix — disproven by trace.
2. **The directory entry is written but UNCLOSED.** Raw track-18/sector-1 bytes:
   `00 FF 02 11 00 "GREETING" A0…` — file type **`$02` = UNCLOSED PRG** (bit 7
   clear; a closed PRG is `$82`), data pointer **track 17 / sector 0**. `blocks`
   stays 0. (`parse_directory` reports `type = Prg` because it masks the closed
   bit — do **not** read that as "closed". Check the raw `$02` vs `$82`.)
3. **No data block is ever written.** Across the whole SAVE only **6 disk jobs**
   complete (`$F969`): track 18 (`$22`=`$12`) ×5 and track 19 (`$22`=`$13`) ×1,
   all result `01` OK bar one `0F` (drive-not-ready, power-on). **The drive never
   seeks to or writes track 17** (head 34). Track-17/0 and track-19/0 both flush
   back empty — the data sits in the channel buffer and is never committed.

So the failure is now squarely the **CLOSE phase**: after the data phase the
final `CLOSE` (LISTEN + SECOND `$E1`, SA 1) must flush the partial file buffer to
the allocated track 17/0, set its link bytes `(00, count)`, mark the dir entry
closed (`$82`) with the block count, and write the BAM. None of that runs. This
matches **Session 2's "file left unclosed"** observation — a layer the
write-verify fix correctly did not touch.

**Next step (Session 5):** trace the CLOSE end to end. First question: does the
`CLOSE`/`$E1` command even reach the drive (is the C64 back at `READY`, or is it
stuck holding the bus after the data phase)? Then follow the drive's close-file
routine for SA 1 → the partial-buffer block write (the `$90` write job to track
17) → dir update + BAM write. Tools: the same env-gated drive-PC probe; watch the
C64 KERNAL `CLOSE` (`$F291`-ish) and the 1541 close-file path. Do **not** re-open
the GCR write path or the IEC byte handshake — both are confirmed working for the
directory write and the data receive respectively.

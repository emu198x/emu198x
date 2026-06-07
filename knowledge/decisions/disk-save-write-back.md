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

Conclusion: the 1541 accepts the SAVE over IEC and goes through motions on track
18, but the file's data is never written to a data track and the directory entry
is never laid down. The gap is in the **1541's SAVE/job-write sequencing or the
C64↔1541 IEC data transfer** — *not* in the GCR-surface write or flush decode
built here. (LOAD works; only the write/job path is incomplete.)

**Next step (where to look):** trace the 1541 ROM's job loop during a SAVE —
does it receive the program bytes over IEC into its buffer, queue a WRITE job,
seek to the allocated data track, and execute the head-write? The 18/19-on-track-18
signature suggests the drive is re-writing the directory/BAM track it already had
but never advancing to the data-write job. This is a focused 1541/IEC effort of
its own, separate from (and unblocked-by) the surface write-back. The integration
test is the repro — fast, deterministic, real ROMs.

## Drift triggers

- "Just write the SAVE back to the mounted image" — **stop.** Never write to an
  archive image. Writability is opt-in per mount; archive = read-only.
- "Add a SAVE/KERNAL trap to shortcut it" — **stop.** Authentic GCR-surface
  write + decode-back, not a ROM trap.
- "Default `writable` to true so it's convenient" — **stop.** Default false;
  the work image is mounted writable explicitly.
- Treating `assets/` as scratch space for any test/capture — **stop.** Work
  images live outside the archive tree.

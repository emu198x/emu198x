# Amiga floppy drive — port-gap analysis (2026-04-21)

Phase 1 gap list for tasks #166–#172, following the archive-port
methodology proven on CIA, Paula, Agnus, Blitter, and Denise.

## What the Amiga floppy subsystem is

The Amiga's 3.5" floppy subsystem spans three chips + one peripheral:

1. **Paula disk DMA** — owns DSKPT / DSKLEN / DSKBYTR / DSKSYNC /
   ADKCON disk bits. Already fully ported (#126, #127). It consumes
   MFM words via `Paula8364::note_disk_read_word(word)`.
2. **CIA-A PRA inputs** — /DSKCHANGE, /DSKPROT, /DSKTRACK0, /DSKRDY
   status pins. Currently stubbed with a static `$EB` constant.
3. **CIA-B PRB outputs** — /STEP, DIR, /SIDE, /SEL0-3, /MTR control
   pins driving the mechanism. Currently unread.
4. **The drive itself** — the physical mechanism: head stepper,
   motor, spindle timing, index pulse, MFM track encoder. Absent
   from the live tree entirely; the `-archive` crate has a complete
   implementation.

The live machine reaches the insert-disk screen because CIA-A PRA
latches `/DSKCHANGE` low at power-on. It can't advance further
because nothing answers step pulses, motor-on, or delivers MFM
sector data to Paula's DMA engine.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| Paula DSKPT / DSKLEN / DSKDAT / DSKSYNC registers | ✅ in `commodore-paula-8364` |
| Paula DSKBYTR read-clears + WORDEQUAL delay | ✅ |
| Paula `note_disk_read_word(word)` entry point | ✅ raises INT_DSKSYN on match |
| Paula disk DMA engine tick | ✅ `tick_disk_cck()` in machine |
| CIA-A PRA /DSKxxx inputs | ❌ static `cia_a.set_external_a(0xEB)` stub |
| CIA-B PRB /STEP/DIR/SIDE/SEL/MTR outputs | ❌ ignored (no drive to receive them) |
| Drive mechanism (head, motor, spin-up, index) | ❌ absent |
| MFM track encoder / decoder | ❌ absent |
| ADF-driven track-read path into Paula | ❌ absent |
| Boot-from-ADF integration test | ❌ absent (blocked on all above) |

## Archive coverage (`crates/peripheral-commodore-amiga-floppy-archive/`)

| Area | Archive state |
| --- | --- |
| `pub trait DiskImage` — image abstraction | ✅ `encode_mfm_track` / `write_sector` / `save_data` |
| `pub struct AdfDiskImage` — ADF wrapper | ✅ uses `format-commodore-amiga-adf` |
| `pub struct DriveStatus` — CIA-A PRA bits | ✅ `disk_change/write_protect/track0/ready` |
| `pub struct AmigaFloppyDrive` with full state machine | ✅ ~390 LoC |
| `update_control(step, dir, side, sel, motor)` | ✅ CIA-B PRB parse |
| `tick()` — motor spin-up + 300 RPM index pulse | ✅ E-clock rate |
| `status()` — drive status for CIA-A PRA | ✅ active-low pin bits |
| Drive ID shift register ($FFFFFFFF through /DSKRDY) | ✅ per HRM §Device I.D. |
| `encode_mfm_track()` — raw MFM output | ✅ vAmiga-compatible encoder |
| Write capture + MFM decode back to sectors | ✅ `flush_write_capture()` |
| `mod mfm` — encode + decode MFM words | ✅ 363 LoC, 5 tests |
| In-crate tests | ✅ 17 drive tests + 5 MFM tests |

## HRM cross-check

**Drive status bits on CIA-A PRA** (archive `DriveStatus`) match the
HRM "Disk Subsystem" table: PA2=/CHNG, PA3=/WPRO, PA4=/TK0,
PA5=/RDY. All active-low.

**Drive control on CIA-B PRB** per HRM Appendix F:

```
PB0 /STEP    — step pulse (falling edge advances head)
PB1 DIR      — 0 = step outward, 1 = step inward
PB2 /SIDE    — 0 = upper head, 1 = lower head
PB3 /SEL0    — drive 0 select
PB4 /SEL1    — drive 1 select
PB5 /SEL2    — drive 2 select
PB6 /SEL3    — drive 3 select
PB7 /MTR     — motor enable (latched on DSKSEL falling edge)
```

The archive's `update_control` takes already-decoded active-high
booleans. The machine's CIA-B bridge will extract them from CIA-B's
PRB value.

**Drive ID protocol** (archive `id_shift_register`) matches HRM
§Device I.D.: with motor OFF, each DSKSEL falling edge shifts one
bit of `$FFFFFFFF` out on /DSKRDY (MSB first). Kickstart's trackdisk
sniffs this 32-bit stream before trusting /DSKRDY as a speed signal.

**Motor spin-up** (MOTOR_SPINUP_TICKS = 350,000 E-clock ticks =
~500ms at 709 kHz) and **index pulse** (INDEX_PULSE_TICKS = 141,876
= ~200ms = 300 RPM) match real-drive specs.

## Known divergences / simplifications

1. **Single drive only** — the archive's `AmigaFloppyDrive` models DF0
   only. Multiple-drive support would mean an array plus /SEL0–3
   dispatch. Out of scope for the initial port.

2. **Write protect hard-coded off** — `DriveStatus.write_protect =
   false` always. Fine for ROM boot + most games; non-issue until a
   test demands it.

3. **Synchronous per-track encode** — `encode_mfm_track` materialises
   the whole 12,668-byte track on every fetch. Real hardware streams
   bits. This matches Paula's word-granularity API so no loss of
   fidelity observable from the CPU side.

4. **No IPF (copy-protected format) support** — the `DiskImage` trait
   is designed for it (see the archive's doc comment mentioning
   `IpfImage` in an external `format-ipf` crate) but the port only
   ships `AdfDiskImage`. Adding IPF is purely additive later.

5. **Write capture is session-global** — `note_write_mfm_word` pushes
   into an append-only `Vec<u16>` until `clear_write_mfm_capture()`.
   Not a problem because `flush_write_capture()` drains the pending
   subset, but it accumulates memory until explicitly cleared. Fine
   for normal boot-and-run sessions.

## Register / pin ownership map

| Signal | Real owner | Post-port routing |
| --- | --- | --- |
| CIA-A PRA bits 2-5 (drive status inputs) | CIA-A | Machine updates `cia_a.set_external_a` each tick from `drive.status()` |
| CIA-B PRB bits 0-7 (drive control outputs) | CIA-B | Machine watches CIA-B PRB writes and forwards to `drive.update_control` |
| CIA-B PRB bit 2 `/SIDE` | CIA-B | Machine feeds into `drive.update_control` |
| Paula DSKPT/DSKLEN/DSKDAT/DSKSYNC | Paula | Already done |
| Paula `note_disk_read_word(word)` | Paula | Machine calls when drive selected + motor spinning + DMA requested |
| Drive MFM track encoder | Drive | `drive.encode_mfm_track()` per-side/cylinder |
| ADF image bytes | DiskImage | `AdfDiskImage::new(Adf::from_bytes(...))` |

## Per-phase plan

### Phase 1 — characterisation tests (#167, #168)

- **#167 drive-state tests:** step direction & clamping (0..=79),
  track0 status, motor spin-up timing, motor off + DSKSEL ID stream,
  motor on + spin-up then index pulse, deselect stops index pulses,
  DSKCHANGE cleared by step, head-side select.
- **#168 MFM + ADF tests:** `encode_mfm_track` round-trip through
  `decode_mfm_track`, sync-word pattern `$4489 $4489` at sector
  offset 4, encoded byte count = `MFM_TRACK_BYTES`, header info
  round-trip with known track number, write-capture → flush →
  `save_adf` round-trip.

Archive already has 22 internal unit tests; Phase 1 promotes the
critical ones to integration tests in the live crate's `tests/`
directory so the spec is frozen when Phase 2 starts.

### Phase 2 — port (#169, #170, #171)

- **#169 drive state + CIA signals:** move the `AmigaFloppyDrive` +
  `DiskImage` + `AdfDiskImage` source into the live crate; wire CIA-A
  PRA updates from `drive.status()`; decode CIA-B PRB writes into
  `drive.update_control(...)`. Replace the `$EB` static stub.
- **#170 MFM encode + ADF loader:** land `mod mfm` in the live crate
  with encode+decode, keep the existing format-adf dependency.
- **#171 track-read path to Paula disk DMA:** machine sees
  `paula.disk_dma_pending() && drive.read_data_available()` and feeds
  MFM words from the current track into `paula.note_disk_read_word()`
  at the right CCK cadence (disk byte pacing is already Paula's
  responsibility via `tick_disk_cck`).

### Phase 3 — integrate + retire (#172)

Rename `peripheral-commodore-amiga-floppy-archive` →
`peripheral-commodore-amiga-floppy`, update workspace + machine
`Cargo.toml` path references. Add a boot-from-ADF integration test
(ADF with a trivial bootblock, assert Paula sees the bootblock sync
pattern and fetches the right tracks).

## Conclusion

The floppy port is larger in surface area than CIA (single chip) but
**less invasive** than Agnus or Denise because:

- Paula's disk DMA + CIA-A/CIA-B register storage are already live.
- The archive's `AmigaFloppyDrive` is a self-contained state machine;
  nothing inside the machine already pretends to do its job (the
  $EB stub is a constant, not a mechanism).
- The MFM module has no external dependencies beyond the existing
  `format-commodore-amiga-adf` crate.

Blast radius is one new live crate + ~30 lines of machine wiring to
route CIA-B PRB → drive and feed drive MFM → Paula. Everything else
falls out of the archive's existing shape.

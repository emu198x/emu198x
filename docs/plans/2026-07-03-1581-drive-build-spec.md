# 1581 drive — build spec (slices 2 & 3)

Resumption spec for the Commodore 1581 drive (#69). Slice 1 (the
`format-commodore-c64-d81` crate) is done and green. This captures the
research (VICE 3.10 + our existing plumbing) so slices 2–3 execute fast.

## Slice 2 — `machine-commodore-1581` crate

Model on `machine-commodore-1541`, but far simpler: **no GCR**. The WD177x
ingests a flat sectored image directly.

### Struct
```rust
pub struct Drive1581 {
    cpu: M6502,              // mos-6502, 2 MHz
    cia: Cia6526,            // mos-cia-6526 (VICE models the 8520 with the 6526 core)
    fdc: Wd1770,             // western-digital-wd1770 (reused from the Einstein)
    ram: [u8; 0x2000],       // 8 KB
    rom: [u8; 0x8000],       // 32 KB DOS ROM
    device_number: u8,       // default 8
    cycles: u64,
}
pub const DRIVE1581_CPU_HZ: u64 = 2_000_000;
```
Constructor `new(Drive1581Config { dos_rom: &[u8] })`, ROM must be `0x8000`.
Snapshot DTO `Drive1581Snapshot` (Cia6526, Wd1770, Disk all already derive
serde — embed directly; simpler than the 1541's GCR rebuild).

### Memory map (VICE `memiec.c:249-256`)
| Range | Contents |
|-------|----------|
| `$0000-$1FFF` | RAM (8 KB) |
| `$2000-$3FFF` | open bus (`0xFF`) |
| `$4000-$5FFF` | 8520 CIA — `addr & 0x0F` |
| `$6000-$7FFF` | WD177x — `addr & 0x03` (0=cmd/status,1=track,2=sector,3=data) |
| `$8000-$FFFF` | DOS ROM (32 KB) |

### Tick (mirror 1541 `tick_with_iec_bus`)
```
apply_drive_inputs(bus)          // CIA pa_in/pb_in from bus + WP + disk-change
cpu.irq = cia.irq_active()       // WD177x is POLLED (DOS reads status DRQ/BUSY); verify INTRQ wiring at boot
read/write memory (CIA + WD177x decode)
cpu.tick(); cia.tick(); fdc.tick()
refresh side/motor from CIA Port A
cycles += 1
```

### IEC glue — **identical bit layout to the 1541 VIA1 Port B**
1581 CIA Port B == 1541 VIA1 Port B: PB0 DATA-in, PB1 DATA-out, PB2 CLK-in,
PB3 CLK-out, PB4 ATNA, PB7 ATN-in. So:
- Output: `bus.write_drive_port_b(device_number, cia.port_b_drive_state())` — verbatim from the 1541.
- Input: fold `bus.drive_port()` into `cia.pb_in` at PB0/PB2/PB7 (same `via1_bus_port` logic), plus PB6 = WP sense (`writable ? 0x40 : 0`).
- **ATN**: the 1581 wires /ATN to the CIA FLAG pin (edge → interrupt), unlike the 1541's VIA1 CA1. Set `cia.flag` from ATN level each tick; PB7 also reads ATN level. PB4 ATNA drives the hardware auto-ATN-response (pulls DATA low when ATN active and ATNA set) — model in `port_b_drive_state` fold or the input recompute.

### CIA Port A (VICE `cia1581d.c:128-202`)
- PA0 → side select: `fdc.set_side((pa0 & 1) ? 0 : 1)` — **and** apply the D81
  head-invert so a raw .d81 reads correctly (verify the single-bit convention
  at boot; flip if the directory reads wrong).
- PA2 → motor: `fdc.set_motor((pa2 & 1) ? 0 : 1)` (our crate: motor is implicit).
- PA6 → activity LED (cosmetic).
- PA7 (read) → disk-change sense from the FDC.
- low read bits → device# jumpers.

### Disk
`fdc.insert_disk(0, Disk::new(d81_bytes, 80, 2, 10, 512).with_first_sector_id(1).write_protected(!writable))`.
Raw .d81 works directly IF the side convention above matches (worked out from
VICE `fdd.c:453-457`: physical head H → Disk side index (H^1); the CIA PA0
inversion should produce that — confirm at boot).
`load_d81_bytes[_writable]`, `flush_image` (fdc `Disk::take_dirty` + `data()`).

### In-crate tests (synthetic, like the 1541)
Memory-map decode, CIA/WD177x register round-trips, IEC fold bits. **Real-ROM
boot-to-idle is a slice-3 test** (needs the staged DOS ROM, can't be in CI).

## Slice 3 — runtime + catalogue

- **Firmware id** `commodore-1581-dos-rom` (32 KB), optional like the 1541 id.
  Stage `1581 318045-02 [!]` → `~/.emu198x/roms/commodore-c64/1581.rom`.
- **Drive slot**: give the 1581 its own slot/device so it can coexist with the
  1541. Default decision: distinct drive (IecBus supports 8-11). A catalogue
  entry selects the D81 loader by slot value or a drive-type field.
- **Runtime**: `drive_1581: Option<Drive1581>` (or a drive-kind enum), a
  `load_media` arm calling `load_d81_bytes_writable`, the tick-loop interleave
  (reuse the 1541 cross-multiply cadence with `DRIVE1581_CPU_HZ = 2_000_000`),
  snapshot envelope entry, and generalise `autoload_basic_disk`'s `drive-8`
  guard.
- **Boot proof**: real DOS ROM reaches idle loop `$B158` (VICE
  `driverom.c:264`); then a catalogue D81 game entry (e.g. from TOSEC
  `Commodore/C64/Games/.../[D81]/`) LOADs and runs — screenshot-verified
  before blessing, per the freeze-cart discipline.

## Status (2026-07-04)

Slices 1 & 2 DONE + committed. Slice 3 (runtime integration) DONE + committed
as WIP: the 1581 attaches, coexists with the 1541 (device 9 / slot `drive-9`),
is wired into a generalised 3-way tick interleave (C64 + 1541 + 1581, scheduled
by virtual time), snapshots, and has a `LOAD"*",9,1` autoload path. **No 1541
regression** — Bruce Lee D64 still reaches LOADING with the new scheduler.

**Remaining: the C64→1581 serial LOAD command transfer.** A real `LOAD"*",9,1`
now reaches `SEARCHING FOR *` and stays there (no longer `?DEVICE NOT PRESENT`).
Two bugs fixed en route:
1. **Device-number jumper**: VICE `read_ciapa` puts the jumpers at `8*(device-8)`
   = **bits 3-4**, not bits 0-1. Fixed in `apply_drive_inputs`.
2. **ATN-acknowledge DATA fold**: the 1541 (`via1d1541`) folds the drive's DATA
   contribution as `~data ^ cpu_bus`, but the 1581 (`cia1581d`) folds it as
   `data | cpu_bus` — a genuinely different per-drive hardware fold. Our shared
   `IecBus` only did the 1541 form. Added `write_drive_port_b_1581` +
   `drive_data_or_fold` so each drive picks its fold. This got "device present"
   working (the drive now auto-ACKs ATN by pulling DATA low).

Still open — localised to the FLAG-interrupt path:
- **ATN reaches the drive** (confirmed with a temporary latch: PB7 goes low
  during the LOAD). So bus propagation + `apply_drive_inputs` ATN fold are fine.
- **ATN gets STUCK low** (~60M ticks = permanent after the command) — a
  handshake deadlock: the C64 asserts ATN and holds it waiting for the drive to
  receive the command; the drive never does, so neither side releases.
- The drive's idle loop ($B105) is interrupt-driven for ATN: `$B108 BIT $76;
  $B10C JMP $FF30` — it jumps to the ATN handler when `$76` bit 0 is set, which
  the IRQ handler sets on a serviced FLAG interrupt. `$76` bit 0 never sets, so
  **the FLAG (ATN) interrupt is not being serviced**, even though ATN reaches
  PB7 and the DOS enabled FLAG in the ICR mask ($9A).

Deeper trace (temp ring buffer of pc + CIA Port B in/out, gated to fire after
boot) shows the FLAG interrupt DOES fire — the drive actively runs its ATN
handler ($AC5A), the $FF00 vector table, and the IRQ handler ($DAFD). So the
FLAG path works; the deadlock is one layer down, in the CLK/DATA/ATNA handshake:

1. **1541 coexistence interference (both-drives case).** With a 1541 (dev 8) +
   1581 (dev 9) on the bus, during a `,9` LOAD the trace shows `DATA in` stuck
   LOW. With the 1581 alone (`local_rom_firmware_with_1581_only`), `DATA in`
   reads HIGH. So the **1541 holds DATA low** during the device-9 command
   instead of un-listening — a real coexistence bug. (The 1541 loads fine
   standalone.)

2. **ATNA / DATA-ACK handshake (1581-only case).** Solo, the drive deadlocks in
   a wait loop ($AE58) with ATN low, CLK low, DATA high. The ATN acknowledge is
   software: the DOS sets ATNA (CIA PB4=1) to make the fold pull DATA low; at
   the deadlock PB4=0, so the drive has RELEASED DATA while the C64 still waits
   for the DATA-low ACK. CLK never toggles (C64 stuck before it clocks bits).
   So the drive's ATNA/DATA-ACK sequencing (or its timing vs the C64's wait) is
   off.

Fresh-pass findings (2026-07-04, session 2) — narrowed further, both sides
captured:
- At the deadlock the **C64 has returned to the BASIC READY loop** ($E5CD) with
  all lines released — it timed out (`?DEVICE NOT PRESENT`) and gave up. The
  1581 is left stuck at $AE58 (`BIT $4001; BNE` = wait for DATA low) as a
  would-be talker.
- $AE53 `JSR $ACE8` = "clear PB1 (release software DATA), then wait for DATA
  low" — the drive relies on the hardware ATN-ACK (ATNA/PB4 + ATN) to hold DATA
  low after it releases the software assert.
- During the ATN window the drive sees CLK toggle and ATN pulse, but **DATA
  never goes low** — the drive never acknowledges ATN.
- **1581-only: the drive makes ZERO writes to `$4001` (its serial output) during
  the whole LOAD** — its ATN handler does not execute at all. At idle its Port B
  output latch is `$80` → PB1/PB3/PB4/PB5 all 0, so ATNA=0 (no auto-ACK) and
  DATA released. (With a 1541 present the handler DID run — the cases differ
  because the 1541's DATA-hold changes what the 1581's idle loop sees. Tangled.)

The two cases diverging + zero serial writes in the solo case say this is a
subtle CIA/timing/state bug that ad-hoc probing keeps peeling without landing.
**Recommended fix approach: get a cycle-exact reference.** Build VICE with a
1581 + the same Batman D81, enable IEC/drive tracing (`-trace`/monitor), and
capture the drive's `$4001` write sequence + ATN/CLK/DATA line states through a
working LOAD command. Then match our drive's Port B writes and CIA FLAG/timer
interrupt timing to that reference, cycle by cycle. Confirmed-good so far:
device number, ATN-present DATA fold, and the FLAG interrupt (fires; drive
reaches its ATN handler in the both-drives case). The gap is why the ATN
handler doesn't drive the serial lines to complete the acknowledge/receive.
Also still open: the 1541 un-listen (release DATA when not addressed) for
coexistence.

## Reference anchors
- VICE 1581: `src/drive/iec/{cia1581d,wd1770,memiec}.c`, `src/drive/iec/fdd.c` (geometry).
- D81 offset: `((track-1)*40 + sector)*256`, track 1-80, sector 0-39.
- WD1770-vs-1772: only the step-rate table differs; VICE uses 1770 rates. Ignore for boot.
- Our reuse: `western-digital-wd1770::{Wd1770, Disk}`, `mos-cia-6526::Cia6526`, `common-commodore-iec::IecBus`, the 1541 tick-interleave in `runtime-commodore-c64/src/runtime.rs:639`.

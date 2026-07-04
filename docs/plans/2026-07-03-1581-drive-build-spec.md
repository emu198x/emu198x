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

**Remaining: the C64→1581 serial LOAD handshake.** A real `LOAD"*",9,1` returns
`?DEVICE NOT PRESENT` — the 1581 boots to its DOS idle loop ($B158 with the C64
present; $ABF8 standalone) but does not acknowledge the C64's ATN by pulling
DATA low. Diagnostic findings:
- Device-number jumper WAS a bug, now fixed: VICE `read_ciapa` puts the jumpers
  at `8 * (device-8)` = **bits 3-4**, not bits 0-1. Without this the drive read
  itself as device 8 and ignored device-9 ATN. Fixed in `apply_drive_inputs`.
- The bus fold matches VICE's `store_ciapb` exactly (same formula as our
  `IecBus::write_drive_port_b`), so the DATA/CLK output path is right.
- The drive spends ~79% of cycles in its timer IRQ handler ($DB08) — a possibly
  suspicious rate; check whether an interrupt storm (Timer A = $0006 underflows
  every 7 cycles) is starving the ATN poll, and whether the FLAG (ATN-edge)
  interrupt path + the ATN-ACK (idle-loop poll at $B158 → handler pulling DATA)
  actually fire. Next: a drive-level test that boots to idle, asserts ATN on a
  bare IecBus, and checks the drive pulls DATA low — isolating the ACK from all
  C64/timing variables.

## Reference anchors
- VICE 1581: `src/drive/iec/{cia1581d,wd1770,memiec}.c`, `src/drive/iec/fdd.c` (geometry).
- D81 offset: `((track-1)*40 + sector)*256`, track 1-80, sector 0-39.
- WD1770-vs-1772: only the step-rate table differs; VICE uses 1770 rates. Ignore for boot.
- Our reuse: `western-digital-wd1770::{Wd1770, Disk}`, `mos-cia-6526::Cia6526`, `common-commodore-iec::IecBus`, the 1541 tick-interleave in `runtime-commodore-c64/src/runtime.rs:639`.

# Atari 2600 — Starpath Supercharger (AR) support (#546)

> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.


**Status:** Fast-load **built and booting** (2026-06-19) on branch
`atari-2600-supercharger-546`. Phaser Patrol fast-loads and renders its ARCADIA
title screen (pixel-verified headless). M1–M3 below are done; the tape-accurate
path remains future work. Approach decided 2026-06-19: **fast-load now,
tape-accurate later.**

## As built (vs plan)

- The plan's design transcribed faithfully; `supercharger.rs` matches Stella
  `CartAR` (dummy BIOS verbatim, bank table, `load_into_ram`, hotspots). The
  distinct-access counter ticks on the **full 16-bit** CPU address (not the
  masked 13-bit), matching Stella `M6502::peek` — the AR RAM-write idiom's "+5"
  timing depends on it.
- **Surprise blocker (not in plan): a RIOT bug.** Phaser Patrol polls
  `LDA $0285 (INSTAT); AND #$FE; BEQ` for free-running frame sync. Our 6532
  wrongly cleared the timer-underflow flag on every INSTAT read, so the poll
  destroyed the flag it waited on → one frame then hang. Real silicon (and
  Stella `M6532::peek` case $05) clears only the PA7 flag on INSTAT; the timer
  flag clears on an **INTIM ($0284)** read. Fixed in a separate commit; the
  INTIM-polling games (Combat, Berzerk, H.E.R.O.) are unaffected.
- **Multi-load (M3):** verified two ways. (1) A cart-level unit test: slot
  selected by `header[5]`, pages re-mapped on switch. (2) A **machine-level**
  test (`tests/supercharger_multiload.rs`) that hand-authors a faithful two-load
  image and drives the full handshake — load 0 asks the BIOS for load 1
  (`STA $FA; JMP $F800`), the BIOS re-enters via the `$1850` hotspot, load 1 is
  selected and runs, painting a colour matched against a plain-4K reference (so
  it's palette-agnostic and runs in CI with no media). A negative control
  (load 0 paints red and never advances) confirms the advance is real, not an
  artifact. No real-*binary* multi-load **game** exists locally: the MAME
  softlist ships single-load protos + FLAC tapes, and no TOSEC 2600 ROM tree is
  staged here. A real multi-load game still needs either a sourced `8448×N`
  image or the tape path.

The Supercharger (Starpath/Arcadia, 1982) is not a bankswitch cartridge — it's a
RAM-expansion peripheral that loads game code from cassette into 6 KB of RAM
behind a small BIOS, with a control register that pages the RAM and switches
RAM/ROM/write modes. ~50 ROMs in the TOSEC 2600 set are Supercharger images and
currently fail `from_rom` with "Unsupported ROM size" (8448, and ×N multi-loads:
25344 = 3×8448, 33792 = 4×8448).

Reference: Stella `CartAR.cxx` / `CartAR.hxx` — local at
`198x/emulators/atari/stella/src/emucore/`. **This plan transcribes the parts we
need so the build needs no re-reading.**

## Approach: fast-load (now) vs tape-accurate (later)

The 8448-byte `.a26` file is the **already-decoded** tape. Two load paths exist:

1. **Fast-load (this plan).** Stella *replaces* the real BIOS with a 294-byte
   dummy stub (`ourDummyROMCode`). It boots, hits a hotspot at `$1850`, and copies
   the load straight into the 6 KB RAM via the header's page table. The real BIOS
   is **not used**. Standard for every emulator's `.a26`/`.bin` handling. Meets
   the issue's acceptance: single-load boots & runs; multi-load advances.
2. **Tape-accurate (future, separate plan).** The real 2 KB BIOS runs and reads
   the tape one PCM bit at a time via `$1FF9`, with sync timing + a ~30000-cycle
   play delay. Needs the raw tape audio (we have **FLAC** for the main releases,
   not the `.a26` protos) plus FLAC decoding + PCM streaming. The game plays
   identically once loaded — only the loading differs.

The game runs at full accuracy after load either way; fast-load is the correct,
proven first deliverable.

## Assets (staged in `~/.emu198x/media/atari-2600/`)

- `Supercharger BIOS.bin` (2048 B) — MAME `a2600/scharger.zip`
  (`starpath supercharger.bin`). **md5 `4565c1a7…` ≠ Stella's canonical
  `0c7926d6…`.** Only the *tape-accurate* path uses a real BIOS, so this
  discrepancy does not affect fast-load. Re-verify before the tape path.
- `Phaser Patrol.a26` (8448 B, single-load proto) — `a2600_cass/ppatrol.zip`
  (`ppatrolp/…(prototype).a26`). **Primary boot target.**
- `Excalibur.a26` (8448 B, Dragonstomper single-load proto) — `a2600_cass/dstomper.zip`.
- Full MAME cassette softlist: `/Volumes/Data/ToSort/MAME 0.288 Software List
  ROMs (merged)/a2600_cass/` (commumut, dstomper, fireball, killsat, mindmstr,
  offifrog, partymix, ppatrol, rabbit, saros, suicidem, survival, sweat). Main
  releases are FLAC (tape path); `xcalibur*`/`ppatrolp` are the binaries.
  Sandbox gotcha: copy off `/Volumes` to `/tmp` in one command (no loops).

## Memory model

`image` = 8 KB working set = three 2 KB RAM banks (6 KB) + one 2 KB ROM region:

```
const BANK_SIZE: usize = 2048;
const RAM_SIZE:  usize = 3 * BANK_SIZE; // 6144 — RAM banks 0,1,2
const ROM_SIZE:  usize = BANK_SIZE;     // 2048 — dummy BIOS, at offset RAM_SIZE
const IMAGE_SIZE: usize = RAM_SIZE + ROM_SIZE; // 8192
const LOAD_SIZE: usize = 8448;          // one tape "load" in the file
```

The 4 KB cart window `$1000-$1FFF` is two 2 KB slots:
- lower `$1000-$17FF` (addr `& 0x0800 == 0`) → `image_offset[0]`
- upper `$1800-$1FFF` (addr `& 0x0800 != 0`) → `image_offset[1]`

```
fn image_index(addr: u16) -> usize {
    (addr as usize & 0x07FF) + image_offset[if addr & 0x0800 != 0 { 1 } else { 0 }]
}
```

A `.a26` file is `num_loads` slots of LOAD_SIZE each. Within a slot:
`[0..6144]` = RAM page data, `[6144..8192]` = ROM placeholder (unused by
fast-load), `[8192..8448]` = the 256-byte **header**.

## The dummy BIOS (`ourDummyROMCode`, 294 bytes — transcribe verbatim)

Copy into `image[RAM_SIZE..]`; fill the rest of the 2 KB ROM with `0x02` (jam)
first. GPL-2.0-or-later → consuming Stella's code verbatim (with attribution) is
fine.

```rust
// Adapted verbatim from Stella CartAR.cxx `ourDummyROMCode` (GPL-2.0-or-later).
const DUMMY_ROM: [u8; 294] = [
    0xa5,0xfa,0x85,0x80,0x4c,0x18,0xf8,0xff, 0xff,0xff,0x78,0xd8,0xa0,0x00,0xa2,0x00,
    0x94,0x00,0xe8,0xd0,0xfb,0x4c,0x50,0xf8, 0xa2,0x00,0xbd,0x06,0xf0,0xad,0xf8,0xff,
    0xa2,0x00,0xad,0x00,0xf0,0xea,0xbd,0x00, 0xf7,0xca,0xd0,0xf6,0x4c,0x50,0xf8,0xff,
    0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff, 0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,
    0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff, 0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,
    0xa2,0x03,0xbc,0x22,0xf9,0x94,0xfa,0xca, 0x10,0xf8,0xa0,0x00,0xa2,0x28,0x94,0x04,
    0xca,0x10,0xfb,0xa2,0x1c,0x94,0x81,0xca, 0x10,0xfb,0xa9,0xff,0xc9,0x00,0xd0,0x03,
    0x4c,0x13,0xf9,0xa9,0x00,0x85,0x1b,0x85, 0x1c,0x85,0x1d,0x85,0x1e,0x85,0x1f,0x85,
    0x19,0x85,0x1a,0x85,0x08,0x85,0x01,0xa9, 0x10,0x85,0x21,0x85,0x02,0xa2,0x07,0xca,
    0xca,0xd0,0xfd,0xa9,0x00,0x85,0x20,0x85, 0x10,0x85,0x11,0x85,0x02,0x85,0x2a,0xa9,
    0x05,0x85,0x0a,0xa9,0xff,0x85,0x0d,0x85, 0x0e,0x85,0x0f,0x85,0x84,0x85,0x85,0xa9,
    0xf0,0x85,0x83,0xa9,0x74,0x85,0x09,0xa9, 0x0c,0x85,0x15,0xa9,0x1f,0x85,0x17,0x85,
    0x82,0xa9,0x07,0x85,0x19,0xa2,0x08,0xa0, 0x00,0x85,0x02,0x88,0xd0,0xfb,0x85,0x02,
    0x85,0x02,0xa9,0x02,0x85,0x02,0x85,0x00, 0x85,0x02,0x85,0x02,0x85,0x02,0xa9,0x00,
    0x85,0x00,0xca,0x10,0xe4,0x06,0x83,0x66, 0x84,0x26,0x85,0xa5,0x83,0x85,0x0d,0xa5,
    0x84,0x85,0x0e,0xa5,0x85,0x85,0x0f,0xa6, 0x82,0xca,0x86,0x82,0x86,0x17,0xe0,0x0a,
    0xd0,0xc3,0xa9,0x02,0x85,0x01,0xa2,0x1c, 0xa0,0x00,0x84,0x19,0x84,0x09,0x94,0x81,
    0xca,0x10,0xfb,0xa6,0x80,0xdd,0x00,0xf0, 0xa9,0x9a,0xa2,0xff,0xa0,0x00,0x9a,0x4c,
    0xfa,0x00,0xcd,0xf8,0xff,0x4c,
];
```

`initialize_rom()` (run once at construction):
1. `image[RAM_SIZE..IMAGE_SIZE].fill(0x02);`
2. copy `DUMMY_ROM` to `image[RAM_SIZE..RAM_SIZE+294]`.
3. `image[RAM_SIZE + 109] = 0xFF;` (fast SC BIOS — skip the progress-bar code).
4. `image[RAM_SIZE + 281] = <power-up random>;` (lands in A on BIOS exit). Reuse
   the deterministic power-up approach from [[project_2600_riot_timer_powerup]]
   (a fixed-seed LCG) so it's reproducible.
5. set vectors `image[IMAGE_SIZE-4..] = [0x0A, 0xF8, 0x0A, 0xF8]` (entry `$F80A`).

## bankConfiguration(cfg: u8)

```
bank_cfg = (cfg & 0b11100) >> 2;     // 0..7
OFFSET_0 = [2,0,2,0,2,1,2,1] * BANK_SIZE
OFFSET_1 = [3,3,0,2,3,3,1,2] * BANK_SIZE   // 3 = ROM (== RAM_SIZE)
image_offset[0] = OFFSET_0[bank_cfg];
image_offset[1] = OFFSET_1[bank_cfg];
power         = (cfg & 0b00001) == 0;       // ROM power (0 = ROM on)
write_enabled = (cfg & 0b00010) != 0;
current_bank  = cfg & 0b11111;              // for debug/scheme()
```

## handleHotspot(addr, distinct_accesses) — control + write mechanism

Called on every cart `read` *and* `write` to `$1000-$1FFF`. Returns whether it
mutated (so the machine can mark the page dirty if needed).

```
// cancel a stale pending write
if write_pending && distinct_accesses > num_distinct_at_hold + 5 {
    write_pending = false;
}
// (1) load the data-hold register: any access to $1000-$10FF ($F000-$F0FF)
if addr & 0x0F00 == 0 && (!write_enabled || !write_pending) {
    data_hold = addr as u8;            // value = low byte of the address
    num_distinct_at_hold = distinct_accesses;
    write_pending = true;
}
// (2) commit a bank configuration: access to $1FF8
else if addr & 0x1FFF == 0x1FF8 {
    write_pending = false;
    bank_configuration(data_hold);
}
// (3) commit a RAM write: exactly 5 distinct accesses after the hold
else if write_enabled && write_pending
        && distinct_accesses == num_distinct_at_hold + 5 {
    let slot = if addr & 0x0800 == 0 { 0 } else { 1 };
    let off  = (addr as usize & 0x07FF) + image_offset[slot];
    if slot == 0 || image_offset[1] != RAM_SIZE { // can't write the ROM slot
        image[off] = data_hold;
    }
    write_pending = false;
}
```

`distinct_accesses` semantics (Stella M6502.cxx:112-116): a global counter
incremented on each CPU memory access **when `addr != last_address`**. See
integration below.

## loadIntoRAM(load: u8) — the fast-load

Triggered by the `$1850` peek hotspot (below). Reads the header of the slot whose
`header[5] == load`, validates, copies pages into RAM, and stages 3 RIOT-RAM
pokes for the dummy BIOS.

```
for each slot s in 0..num_loads:
  base = s * LOAD_SIZE
  header = file[base + 8192 .. base + 8448]    // 256 bytes
  if header[5] != load { continue }
  // verify: 8-bit sum of header[0..8] == 0x55 (warn, don't abort, on mismatch)
  for j in 0..header[3]:                        // header[3] = page count
    bank = header[16 + j] & 0b011
    page = (header[16 + j] & 0b11100) >> 2
    src  = file[base + j*256 .. base + j*256 + 256]
    // page checksum: (sum(src) + header[16+j] + header[64+j]) == 0x55 (warn only)
    if bank < 3 { image[bank*2048 + page*256 .. +256].copy_from_slice(src) }
  // stage RIOT-RAM pokes for the BIOS:  $fe = header[0], $ff = header[1], $80 = header[2]
  return ArEffect::RamPokes([(0xfe, header[0]), (0xff, header[1]), (0x80, header[2])])
return ArEffect::None   // "load not found"
```

Header bytes that matter: `[0]` = bank-switch/control byte (→ `$fe`), `[1]` =
start address (→ `$ff`), `[2]` = next-load number (→ `$80`), `[3]` = page count,
`[5]` = this slot's load number, `[16+j]` = page j's bank(0-1)/page(2-4)
descriptor, `[64+j]` = page j's checksum byte.

## read(addr, distinct_accesses, ram_80) → (byte, ArEffect)

```
// fast-load hotspot: BIOS reaches $1850 with the ROM slot mapped in the upper window
if addr & 0x1FFF == 0x1850 && image_offset[1] == RAM_SIZE {
    let effect = load_into_ram(ram_80);     // ram_80 = system RIOT $80
    return (image[(addr as usize & 0x07FF) + image_offset[1]], effect);
}
let mutated = handle_hotspot(addr, distinct_accesses);
(image[image_index(addr)], if mutated { ArEffect::Dirty } else { ArEffect::None })
```

`write(addr, _value, distinct_accesses)` → just `handle_hotspot(addr, distinct_accesses)`
(AR ignores the data bus value; the value comes from the address).

## reset / construction

Stella `reset()` does `initialize_rom()` then `bank_configuration(0)` (slot0 =
RAM bank 2, slot1 = ROM, write disabled, ROM powered) and `loadIntoRAM(0xFF)`?
No — it sets the **dummy** state so the BIOS runs first. Concretely at power-on:
`image_offset = [2*BANK_SIZE, RAM_SIZE]` (RAM bank 2 low, ROM high), so the CPU
resets through the `$F80A` vector into the dummy BIOS, which then drives the
`$1850` load of load #0. Verify against `CartAR.cxx::reset()` during the build.

## Cross-component integration (the non-trivial part)

This is what makes AR different from every existing scheme. Touch points:

1. **Distinct-access counter** — `machine-atari-2600/src/lib.rs`. Add fields
   `last_address: u16`, `distinct_accesses: u32`. In **both** `mem_read`
   (line 169) and `mem_write` (line 190), at the top:
   `if addr != self.last_address { self.distinct_accesses = self.distinct_accesses.wrapping_add(1); self.last_address = addr; }`
   Only the AR scheme consumes it; everything else is unaffected.

2. **Cart sees `distinct_accesses` + RIOT `$80`, returns effects.** Keep
   `Cartridge::read/write` signatures stable for the 13 other schemes by adding a
   pre-call context + post-call effect, OR add the two scalars as params (small,
   explicit — preferred). The AR `read` needs `ram_80 = self.riot.ram()[0]`
   (RIOT RAM `$80` → `ram[0x80 & 0x7F] = ram[0]`).

3. **Machine applies `ArEffect::RamPokes`** — after `self.cart.read(...)` in
   `mem_read`, if it returns pokes, write them through `self.riot.ram_mut()`
   (accessor at `mos-riot-6532` line 292): for `(a, v)` → `ram_mut()[(a & 0x7F) as usize] = v`.

`$80 → ram[0]`, `$fe → ram[0x7e]`, `$ff → ram[0x7f]`.

## Phase plan (this feature does NOT split into tiny independent merges)

The fast-load does nothing useful until the integration lands, so develop on one
branch `atari-2600-supercharger-546` and **keep it unmerged until Phaser Patrol
boots and pixel-verifies.** Land it as one reviewed PR (or 2 if it gets large:
core+detection, then integration+boot). Internal milestones:

- **M1 — AR data model + detection + load parsing.** Add
  `BankingScheme::Supercharger`; detect `len >= 8448 && len % 8448 == 0` in
  `from_rom` (before the `match data.len()`, alongside the DPC/E0/3E pre-checks).
  Implement `image`, `initialize_rom`, `bank_configuration`, `load_into_ram`.
  **Unit-test against the real `Phaser Patrol.a26`:** header[5]==0, header[0..8]
  sum == 0x55, and after `load_into_ram(0)` the RAM banks equal the file pages
  per the descriptor table. No machine wiring yet — test the struct directly.
- **M2 — integration + boot.** Distinct-access counter, the read/write context +
  `ArEffect` plumbing, RIOT-RAM pokes, the `$1850` hotspot. Drive the machine
  headless with `Phaser Patrol.a26` for ~200 frames; **pixel-verify** the title
  screen renders (non-blank, recognisable — capture a PNG and eyeball vs a Stella
  screenshot). Add a `cart_boot`-style integration test gated on the media file.
- **M3 — multi-load.** Fast-load already handles it: the running game writes the
  next load number to `$80` and re-triggers `$1850`; `load_into_ram` finds the
  matching slot by `header[5]`. Verify with a multi-load binary (extract a
  25344-byte image, or convert a FLAC release — TBD). If no binary multi-load is
  available, document the gap and lean on the tape path for those titles.

## Verification

- `cargo test -p machine-atari-2600` + `cargo test -p atari-tia` green.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Phaser Patrol boots to its title/attract screen (PNG eyeballed vs Stella).
- No regression in the 34-game render sweep (the AR path is reached only by
  8448×N ROMs; all other schemes unchanged).
- Small PRs, `--merge` (never squash), branch from `main`.

## Risks / open questions

- **Reset state.** Confirm `CartAR.cxx::reset()` exactly — the initial
  `image_offset` and whether the first load is BIOS-driven vs forced. Get this
  wrong and the CPU resets into RAM/garbage.
- **`distinct_accesses` == "address changed".** It is *not* a raw cycle count —
  consecutive same-address accesses don't tick it. The "+5" write timing depends
  on this; model it exactly.
- **Header checksum failures** should warn (via the existing logging path), not
  panic — some dumps have soft checksum errors but still run.
- **md5-mismatched BIOS** is irrelevant to fast-load (dummy BIOS) but blocks the
  future tape path until a canonical dump is sourced.

## Reference

- Stella `CartAR.cxx` / `CartAR.hxx` (`198x/emulators/atari/stella/src/emucore/`).
- Memory: [[project_2600_supercharger_546]], [[project_2600_riot_timer_powerup]].
- Issue #546. Cartridge integration points: `machine-atari-2600/src/cartridge.rs`
  (`from_rom` :144, `read` :257, `write` :470, `scheme` :681) and
  `machine-atari-2600/src/lib.rs` (`mem_read` :169, `mem_write` :190).

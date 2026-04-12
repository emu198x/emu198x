# Commodore 64

The C64 is an 8-bit home computer from Commodore, released 1982. Roughly:

- **CPU**: MOS 6510 at 985 248 Hz PAL / 1 022 727 Hz NTSC — a 6502 with an integrated 6-bit I/O port at `$00`/`$01` for bank switching.
- **Memory**: 64 KiB RAM + 8 KiB BASIC ROM + 8 KiB KERNAL ROM + 4 KiB character ROM + 1 KiB colour RAM.
- **Video**: MOS 6569 (PAL) / 6567 (NTSC) VIC-II — 40×25 text, 320×200 bitmap, 8 hardware sprites, 16 fixed colours.
- **Audio**: MOS 6581 (early) / 8580 (late) SID — three voices, ADSR, state-variable filter.
- **I/O**: 2× MOS 6526 CIA — CIA1 for the keyboard matrix and joystick port 2; CIA2 for the serial (IEC) bus, VIC bank select, user port, and NMI source.
- **Peripherals** (not yet wired in this port): 1541 disk drive, Datasette, REU RAM expansion, cartridges.

See the [C64 per-subsystem source map](../decisions/archives-as-source.md#c64) for port sourcing and the per-chip wiki pages for details of each chip's public contract.

## Implementation status

**Phase 2 chip wave: complete.** All four core chips are ported with their own test suites:

| Crate | Tests | Status |
|---|---|---|
| [mos-6502](../chips/mos-6502.md) | 16 | Pipelined pin bus, foundation commit `25cd870`, tick commit `2d42f8b` |
| [mos-cia-6526](../chips/mos-cia-6526.md) | 23 | Commit `cf7d0e7` |
| [mos-sid-6581](../chips/mos-sid-6581.md) | 9 | Commit `49128bf` |
| [mos-vic-ii](../chips/mos-vic-ii.md) | 23 | Commit `7ac5a65` |

**Phase 3 machine wiring: complete and end-to-end validated.** `machine-commodore-c64` ties the four chips together (commit `a398d4c`). 24 unit + integration tests passing.

**🎯 2026-04-09: KERNAL boots to `READY.` prompt on first try.** The `#[ignore]`'d boot integration test runs a full frame loop against real Commodore ROMs and finds the PETSCII string `READY.` in screen memory at frame **108** (~2.16 seconds of emulated C64 time, matching real hardware's ~2.5 second cold-boot timing). Every chip worked correctly end-to-end: the 6502 executed ~2 M real KERNAL opcodes without an illegal-instruction trap, CIA1 Timer A drove the ~60 Hz IRQ loop, the VIC-II's bad-line BA assertion stalled the CPU without deadlocking, memory banking put BASIC and KERNAL in the right places, and the pin-level bus loop routed every read and write correctly.

This is the single biggest validation milestone of the whole C64 port. Every architectural decision — pin-level CPU bus (`RULES.md` item 6), `VicMemory` trait, one-op-per-tick discipline, tick ordering, IRQ routing, RDY-only-gates-reads semantics — held up under the hardest possible test: running real Commodore ROM code through to the operating-system prompt.

**Phase 4 runtime + CLI: complete.** `runtime-commodore-c64` implements the cross-family [`System`](../../crates/emu198x-shell/src/system.rs) trait on top of `machine-commodore-c64`. `emu198x-script-c64` is a headless CLI binary that boots the C64 from ROM files, advances N frames, and writes a PNG screenshot. Uses the shell's `encode_png_to_file` capture pipeline end-to-end.

**🎯 2026-04-09: C64 `READY.` screenshot rendered as PNG.** Running:

```sh
emu198x-script-c64 --roms ~/Projects/Emu198x-archive-april2026/roms/c64 \
                   --frames 120 --screenshot ready.png
```

produces a 416×312 8-bit RGBA PNG showing the classic `**** COMMODORE 64 BASIC V2 ****` / `64K RAM SYSTEM  38911 BASIC BYTES FREE` / `READY.` banner in the light-blue-on-blue C64 palette. This is the first *visible* proof the port chain works: VIC-II framebuffer → runtime RGBA re-pack → shell `encode_png` → PNG file on disk.

## Crate

`machine-commodore-c64` — owns the four chips + memory + keyboard matrix and exposes a tick loop that routes the pin-level bus between them. See `wiki/chips/mos-*.md` for each chip's individual pin contract; this page is about how the pins connect together.

### Modules

- `config.rs` — `C64Model` (PAL/NTSC) with clock rates, TOD dividers, frame sizing. `C64Config` with ROM bytes + audio sample rate.
- `keyboard.rs` — 8×8 `KeyboardMatrix`. Pure state + pure function.
- `memory.rs` — `C64Memory`: 64 KiB RAM + KERNAL + BASIC + character ROM + colour RAM + `$00`/`$01` port banking. Implements `VicMemory` (for the VIC-II's VRAM access) and `format_commodore_c64_prg::RamAccess` (for PRG loading that bypasses ROM overlays).
- `machine.rs` — `C64` struct with all four chips, the memory, the keyboard matrix, the master clock counter, and the tick loop.

## Pin routing

The machine layer's tick loop is where the chip pin fields actually get connected to each other — the topology described in `wiki/decisions/cpu-bus-interface.md` becomes concrete code here for the first time.

```
VIC-II ──irq──────┐
                  ├── OR ──→ CPU.irq
CIA1   ──irq──────┘

CIA2   ──irq──────────────→ CPU.nmi

VIC-II ──ba_low──┬── AND ──→ CPU.rdy
CPU    ──rw────────┘            (NMOS 6502: RDY only gates reads)

VIC-II ──framebuffer──→ take via C64::framebuffer()
SID    ──audio buffer──→ drain via C64::take_audio_buffer()
```

The keyboard matrix is polled between CIA1 ticks:

```
KeyboardMatrix ──scan(cia1.pa)──→ cia1.pb_in
```

The VIC-II bank follows CIA2 port A bits 0-1 inverted:

```
(!cia2.pa) & 0x03 ──→ vic.set_bank()
```

## CPU bus routing

The 6510 is a pin-level CPU (`addr`, `data`, `rw`, `sync`, `data_in`). The machine's tick loop reads the CPU's pins between ticks and routes the operation:

```rust
if self.cpu.rdy {
    if self.cpu.rw {
        self.cpu.data_in = self.cpu_read(self.cpu.addr);
    } else {
        self.cpu_write(self.cpu.addr, self.cpu.data);
    }
    self.cpu.tick();
}
```

Both `cpu_read` and `cpu_write` go through the `$01` banking decoder:

- `$0000`, `$0001` → 6510 port DDR / data register (handled inside `C64Memory`).
- `$A000-$BFFF` → BASIC ROM if HIRAM && LORAM, else RAM (writes always land in underlying RAM).
- `$D000-$DFFF` → I/O routing if CHAREN && (HIRAM || LORAM), char ROM if !CHAREN && HIRAM && LORAM, else RAM.
- `$E000-$FFFF` → KERNAL ROM if HIRAM, else RAM.
- Everything else → plain RAM.

When I/O is visible, `$D000-$DFFF` routes to a specific chip:

- `$D000-$D3FF` → VIC-II
- `$D400-$D7FF` → SID
- `$D800-$DBFF` → colour RAM (inside `C64Memory`)
- `$DC00-$DCFF` → CIA1 (before reading `$DC01`, the machine scans the keyboard matrix into `cia1.pb_in`)
- `$DD00-$DDFF` → CIA2 (on writes to `$DD00` or `$DD02`, the machine refreshes the VIC bank from `cia2.pa`)
- `$DE00-$DFFF` → I/O expansion (cartridge area, unmapped in this port)

## Deferred (follow-up work)

Known gaps from the archive that were deliberately not ported in this first pass:

- **Cartridge support** (CRT files, EXROM/GAME lines, ROML/ROMH overlays, Ultimax mode). The memory decoder's PLA variants are wired in the archive via `cart.exrom` / `cart.game`; ours is the EXROM=1, GAME=1 case only.
- **1541 disk drive** — full second 6502 + 6522 VIAs + GCR encoder + head-tracking model.
- **IEC serial bus** — three-wire protocol between C64 and drive. Not needed without a drive.
- **Datasette** — pulse-level model, TAP loading, tape trap in the KERNAL.
- **REU** — RAM Expansion Unit with DMA.
- **Symbolic keyboard mapping** — host `KeyboardEvent.code` strings → C64 key matrix positions + shift overrides.
- **Timed input queue** — scheduled keystrokes for the automated boot-and-type workflow.
- **`System` / `EmulatedSystem` trait implementations** — those live in `runtime-commodore-c64` (not yet built).
- **MCP introspection query paths** — `cpu.*`, `vic.*`, `cia1.*`, `sid.*`, `memory.*`.
- **Save state round-trip testing** — serde derives are in place on every type but the snapshot/restore helpers + a round-trip test are a follow-up.

## Boot-to-READY test

An `#[ignore]`'d integration test at `tests::boots_kernal_to_ready_prompt` runs a full frame loop against real ROMs and searches screen memory at `$0400-$07E7` for the PETSCII string `READY.` (character codes 18, 5, 1, 4, 25, 46). The test is marked `#[ignore]` because (a) Commodore's ROMs are preservation-grade artifacts we don't check into the repo, and (b) the test takes ~2.35 seconds and shouldn't run on every `cargo test` invocation.

**Known-good ROM location on this development machine**: `~/Projects/Emu198x-archive-april2026/roms/c64/{basic,chargen,kernal}.rom` (sizes 8192 / 4096 / 8192 bytes respectively). To enable the test:

```sh
mkdir -p crates/machine-commodore-c64/test-roms
cp ~/Projects/Emu198x-archive-april2026/roms/c64/basic.rom   crates/machine-commodore-c64/test-roms/c64-basic.rom
cp ~/Projects/Emu198x-archive-april2026/roms/c64/chargen.rom crates/machine-commodore-c64/test-roms/c64-chargen.rom
cp ~/Projects/Emu198x-archive-april2026/roms/c64/kernal.rom  crates/machine-commodore-c64/test-roms/c64-kernal.rom
cargo test -p machine-commodore-c64 -- --ignored --nocapture
```

The `test-roms/` directory has a `.gitignore` excluding `*.rom` so accidental `git add` doesn't track the preservation-grade bytes. Expected output: `Found READY. at frame 108, offset $00C8`.

## Related

- [Archives as source](../decisions/archives-as-source.md#c64) — port provenance.
- [CPU bus interface](../decisions/cpu-bus-interface.md) — the pin-level rule the machine loop enforces.
- [MOS 6502](../chips/mos-6502.md), [CIA](../chips/mos-cia-6526.md), [SID](../chips/mos-sid-6581.md), [VIC-II](../chips/mos-vic-ii.md) — per-chip contracts.

# Dragon 32/64

## Status: Fully functional — cassette loading, joystick, all graphics modes

The Dragon 32 boots the BASIC ROM, loads software from .CAS cassette files (via PIA at the correct baud rate) and .BIN binary files, accepts keyboard and joystick input, and plays sound via the SOUND command. SG4 semigraphics and all 8 PMODE graphics modes working.

## What works

- **CPU:** Motorola 6809 — full instruction set (3 opcode pages), all indexed addressing modes. 2,550,000 custom single-step tests passing (10,000 per opcode).
- **Video:** MC6847 VDG — text mode (32×16) with real character ROM, SG4 semigraphics (SET/RESET/POINT), all 8 graphics modes (CG1-CG6, RG1-RG6), border rendering, CSS colour set switching via PIA1.
- **Cassette:** Bus-level tape loading via emu-tape. CAS bytes converted to nanosecond-accurate pulse durations (1200/2400 Hz FSK). Signal fed through PIA0 port A bit 0 per CPU cycle. ROM's CLOAD/CLOADM reads tape naturally. Motor control via tape transport.
- **I/O:** MC6821 PIA x 2 — DDR/data/control registers, IRQ flags, keyboard matrix (PIA0), VDG mode control (PIA1), cassette data input.
- **Memory:** MC6883 SAM — video offset, VDG mode bits, CPU rate, memory size, all-RAM mode (P1 register rebuilds page table). 32KB RAM + 16KB ROM at $8000-$BFFF with vector mirroring.
- **Audio:** 6-bit DAC from PIA1 port A (single-bit + DAC blend), 48kHz output via cpal. SOUND command works with correct duration timing via PIA0 CB1 frame IRQ.
- **Keyboard:** Full matrix with shift-aware PC-to-Dragon mapping. BREAK (Escape) via matrix polling.
- **Joystick:** Analogue joystick via DAC/comparator on PIA0 port A bit 7. Digital press/release API with axis mapping (0-63, centre 31). Numpad 8/2/4/6 + Numpad0/RAlt.
- **Dragon 64:** SAM all-RAM mode implemented — page table rebuilt when P1 register changes, ROM area becomes writable RAM.
- **Shell:** Save states (F3/F4), rewind (Tab), PNG screenshots (F5), auto-CLOAD/CLOADM for .cas files, auto-EXEC for .bin files.

## Remaining

### Nice to have
- **Floppy controller** (WD2797) — for DragonDOS disk images
- **SG6/SG8/SG12/SG24** — higher semigraphics modes (require external A/S signal control not wired on Dragon hardware — only SG4 is accessible)
- **Cartridge port** — auto-start ROMs
- **Per-cycle VDG rendering** — current renderer is per-scanline
- **Sound MUX** — PIA0 CA2/CB2 select between DAC, cassette, and cartridge audio sources

## Test coverage

| Component | Tests |
|-----------|-------|
| Machine | 7 (create, ROM mapping, keyboard, ROM readonly, all-RAM mode) |
| PIA | 5 (DDR, control, IRQ, input pins, mixed I/O) |
| SAM | 4 (defaults, set/clear, video offset, all-RAM) |
| VDG | 4 (modes, rendering, text mode) |
| Cassette | 3 (parsing, bitstream encoding, transport playback) |
| Variants | 3 (Dragon 32, Dragon 64, registration) |
| **Total** | **25** |

## ROMs

Place in `roms/dragon/`:

| File | Size | Description |
|------|------|-------------|
| `dragon32.rom` | 16KB | Dragon 32 BASIC ROM (required) |

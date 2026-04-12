# Spectrum Variants

Eleven variants implemented: 16K, 48K, 128K, +2, +2A, +2B, +3, Pentagon 128, Scorpion ZS-256, Timex TC2048, Timex TS2068. Hardware differences concentrate in memory paging, contention, and I/O port decoding.

The 16K is the same hardware as the 48K with the upper 32K of DRAM physically absent — it shares the machine and memory implementation, just constructed with `Memory48K::new_16k()` so reads above $7FFF return $FF and writes are dropped.

## Memory maps

### 16K

| Range | Contents |
|-------|----------|
| $0000-$3FFF | ROM (16K, fixed) |
| $4000-$7FFF | RAM bank 5 (screen, contended) |
| $8000-$FFFF | Unpopulated — reads $FF, writes dropped |

### 48K

| Range | Contents |
|-------|----------|
| $0000-$3FFF | ROM (16K, fixed) |
| $4000-$7FFF | RAM bank 5 (screen, contended) |
| $8000-$BFFF | RAM bank 2 |
| $C000-$FFFF | RAM bank 0 |

### 128K / +2

Port **$7FFD** (A15=0, A1=0):

| Bit | Function |
|-----|----------|
| 0-2 | RAM bank at $C000 (0-7) |
| 3 | Screen bank (0=bank 5, 1=bank 7) |
| 4 | ROM (0=128K editor, 1=48K BASIC) |
| 5 | Paging lock (irreversible until reset) |

2 ROMs (32K total), 8 RAM banks (128K).

### +2A / +2B / +3

Port **$7FFD** — same as 128K.

Port **$1FFD** (A15=0, A14=0, A12=1, A1=0):

| Bit | Function |
|-----|----------|
| 0 | Paging mode (0=normal, 1=special) |
| 1-2 | Special mode select (0-3) |
| 2 | ROM select high bit |
| 3 | Disk motor |
| 4 | Printer strobe |

ROM select = `($1FFD bit 2) << 1 | ($7FFD bit 4)`. Four ROMs: 0=128K editor, 1=syntax checker, 2=+3DOS, 3=48K BASIC.

**Special paging modes** (all RAM, no ROM):

| Mode | $0000 | $4000 | $8000 | $C000 |
|------|-------|-------|-------|-------|
| 0 | Bank 0 | Bank 1 | Bank 2 | Bank 3 |
| 1 | Bank 4 | Bank 5 | Bank 6 | Bank 7 |
| 2 | Bank 4 | Bank 5 | Bank 6 | Bank 3 |
| 3 | Bank 4 | Bank 7 | Bank 6 | Bank 3 |

## I/O ports

| Port | Decode | Device | Present on |
|------|--------|--------|------------|
| $xxFE | A0=0 | ULA (border, beeper, keyboard) | All |
| $7FFD | A15=0, A1=0 (128K) / A15=0, A14=1, A1=0 (+3) | Memory paging | 128K+ |
| $1FFD | A15=0, A14=0, A12=1, A1=0 | Extended paging | +2A/+3 |
| $FFFD | A15=1, A14=1, A1=0 | AY register select / read | 128K+ |
| $BFFD | A15=1, A14=0, A1=0 | AY data write | 128K+ |
| $1F | Low byte match | Kempston joystick | Optional |

## EAR/MIC (port $FE bit 6)

- Tape connected: tape signal drives bit 6 directly (high=0, low=1)
- No tape: beeper/MIC output feeds back to bit 6 via hardware
- Must suppress feedback when tape is connected, or tape loading fails

## Floating bus

| Variant | Behaviour |
|---------|-----------|
| 48K / 128K / +2 | Returns ULA data bus during screen fetch, $FF during border |
| +2A / +2B / +3 | Always $FF (Amstrad gate array killed floating bus) |

## Interrupt

All variants: INT asserted at vertical blank start for 32 T-states. IM 1 vector: $0038. IM 2 vector: `(I << 8) | $FF` on 48K (bus floats high, no device responds).

## ROMs

| Variant | Files | Distributable |
|---------|-------|---------------|
| 48K | 48.rom | Yes (Amstrad permission) |
| 128K | 128-0.rom, 128-1.rom | Yes |
| +2 | plus2-0.rom, plus2-1.rom | Yes |
| +2A/+3 | plus3-0.rom .. plus3-3.rom | Yes |

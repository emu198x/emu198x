# Commodore Gary — port-gap analysis (2026-04-21)

Phase 1 gap list for tasks #176–#178, following the archive-port
methodology. Gary is the smallest Amiga chip port — a pure address
decoder with no runtime state transitions.

## What Gary is

Gary is the A500/A1000/A2000 address-decode chip. It maps a 24-bit
CPU address to one of a dozen chip-select lines:

```
$00_0000 .. $1F_FFFF  →  chip RAM
$20_0000 .. $5F_FFFF  →  unmapped (expansion)
$60_0000 .. $9F_FFFF  →  PCMCIA common (A600/A1200)
$A0_0000 .. $A5_FFFF  →  PCMCIA attr (A600/A1200)
$BF_D000 .. $BF_DFFF  →  CIA-B
$BF_E000 .. $BF_EFFF  →  CIA-A
$C0_0000 .. $D7_FFFF  →  slow RAM (ranger / trapdoor)
$DC_0000 .. $DC_003F  →  RTC (A2000/A3000/A4000)
$DD_0000 .. $DD_FFFF  →  DMAC (A3000)
$DE_0000 .. $DE_FFFF  →  resource registers (A3000/A4000)
$D8_0000 .. $DE_FFFF  →  Gayle (A600/A1200, overrides slow RAM)
$DF_F000 .. $DF_FFFF  →  custom chip registers
$E8_0000 .. $EF_FFFF  →  Zorro autoconfig
$F8_0000 .. $FF_FFFF  →  Kickstart ROM
```

Later models (A600, A1200, A3000, A4000) replace Gary with Gayle or
Fat Gary, but the decode priorities are the same — the archive
encodes the full family.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| CIA-A decode (`$BFExxx`) | ✅ `cia::decode_cia_a` in machine |
| CIA-B decode (`$BFDxxx`) | ✅ `cia::decode_cia_b` in machine |
| Custom register decode (`$DFFxxx`) | ✅ inline `CUSTOM_BASE..CUSTOM_TOP` |
| Chip RAM / ROM / slow RAM routing | ✅ inside `Memory::read_word` / `write_word` |
| Unified `ChipSelect` enum | ❌ scattered bit tests |
| A1200 / A3000 decode variants | ❌ A500 hard-coded only |

## Archive coverage (`crates/commodore-gary-archive/`)

| Area | Archive state |
| --- | --- |
| `pub enum ChipSelect` — 14 variants | ✅ |
| `pub struct Gary` with model flags | ✅ slow_ram / gayle / pcmcia / dmac / resource_regs / rtc |
| `Gary::decode(addr) -> ChipSelect` const fn | ✅ full priority chain |
| 24-bit address masking | ✅ `addr & 0xFF_FFFF` |
| A500 / A1200 / A3000 truth tables | ✅ 3 full-matrix tests |
| Shadow / priority tests (CIA over slow RAM, custom over Gayle, DMAC over Gayle) | ✅ |
| Unmapped gap tests (expansion, ranger, diagnostics) | ✅ |
| In-crate tests | ✅ 30 decode tests |

## HRM cross-check

**Priority order** matches HRM Appendix A "Memory Map": CIAs win
over slow RAM (both use `$BFxxxx` / `$BxxxxF` adjacency); custom
registers shadow Gayle at `$DFFxxx`; DMAC at `$DDxxxx` overrides
Gayle's `$D80000-$DFFFFF` block on A3000. The archive encodes all
three cases.

**24-bit truncation** is Gary's responsibility on 68000 systems;
68020+ with 32-bit address buses shift the responsibility to Fat
Gary / Bridgette (via a separate 24-bit transparent-gate chip).
Archive's `addr & 0xFF_FFFF` at the top of `decode` matches the
68000 Amiga behaviour.

## Known divergences / simplifications

1. **Gary exposes `Default::default()` as "no optional peripherals"** —
   this is the minimal A1000 config. The machine currently needs
   A500 with slow RAM, so the Phase 2 wiring configures
   `set_slow_ram_present(true)`.

2. **RTC decode present but unused** — the machine has no RTC;
   setting `set_rtc_present(false)` (default) returns `Unmapped`
   for `$DC0000-$DC003F`.

3. **Gary is stateless** — decode is a `const fn` with no side
   effects. No "tick at E-clock rate" wiring needed. Makes this
   the simplest Amiga port by a wide margin.

## Per-phase plan

### Phase 1 — characterisation tests (#176)

- **Decode truth tables** for A500 with slow RAM (the live machine's
  config): all 14 variants + shadow priorities + unmapped gaps + 24-
  bit masking. Already covered by the archive's in-crate tests;
  Phase 1 promotes the critical subset into `tests/` so the spec is
  frozen against Phase 2 wiring.

### Phase 2 — port (#177)

- Machine depends on `commodore-gary` (archive path until Phase 3).
- `AmigaOcs` gains a `gary: Gary` field configured for A500 + slow
  RAM at construction.
- Existing address-dispatch sites (`poke_word`, `poke_byte`,
  `read_word`, `read_long`, CPU bus servicer) use `gary.decode(addr)`
  as the source of truth; inline bit tests become exhaustive match
  arms on `ChipSelect`. CIA / custom / memory handlers stay in
  their existing modules.

### Phase 3 — integrate + retire (#178)

- Rename `crates/commodore-gary-archive/` → `crates/commodore-gary/`.
- Update workspace + machine `Cargo.toml` path refs.
- Add machine integration test asserting a handful of representative
  addresses route to the expected handler (CIA-A read, custom
  register write, chip RAM access, ROM fetch).

## Conclusion

Smallest Amiga port on the menu — a single pure function + a config
struct. Risk is low because the function has no runtime state and
the archive's 30 tests already cover the decode table exhaustively.
Main payoff is documentation clarity: replacing scattered `if (addr
& 0xFFF000) == 0xBFE000` bit tests with an exhaustive `match
ChipSelect` pattern.

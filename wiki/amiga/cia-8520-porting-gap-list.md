# MOS 8520 CIA — porting gap list

**Task:** #105 (CIA-8520 Phase 1: characterize archive and build gap list)
**Sources:**
- Archive: `crates/mos-cia-8520-archive/src/lib.rs` (773 lines)
- Current in-tree: `crates/machine-commodore-amiga-ocs/src/cia.rs`
- HRM: *Amiga Hardware Reference Manual, 3rd ed.*, Appendix F (pp. 340-352)
- WinUAE: `~/Projects/Emu198x-Unclean/fs-uae/cia.cpp` + `~/.../WinUAE/cia.cpp`
- vAmiga: `~/Projects/Emu198x-Unclean/vAmiga/Core/Components/CIA/CIARegs.cpp`

This document is purely descriptive — a read-only survey. No code
changes in Phase 1.

## Legend

| Symbol | Meaning |
|---|---|
| ✅ covered | present in current in-tree `cia.rs` and behaves correctly |
| 🟡 partial | present but missing a side effect / quirk |
| ❌ missing | absent from current in-tree; needs port |
| 📖 HRM-match | archive behaviour matches Amiga HRM Appendix F |
| ⚠️ HRM-deviation | archive deviates from HRM (or HRM silent; archive matches
  another emulator / datasheet detail) |

---

## 1. Register map

Per HRM Appendix F Table F-1. Sixteen addressable 8-bit registers.

| Addr | Name | R/W | In archive | In current | Notes |
|---|---|---|---|---|---|
| $0 | PRA | R/W | ✅ | ✅ | Port A data register. Effective value = `(pra & ddra) \| (external & !ddra)`. |
| $1 | PRB | R/W | ✅ | ✅ | Port B; same shape. |
| $2 | DDRA | R/W | ✅ | ✅ | Bit = 1 → output. |
| $3 | DDRB | R/W | ✅ | ✅ | Same for Port B. |
| $4 | TALO | R/W | ✅ | ✅ | Timer A low. READ uses latched value (archive). WRITE updates latch only. |
| $5 | TAHI | R/W | ✅ | ✅ | Timer A high. READ releases latch. WRITE also transfers latch → counter **when stopped** (and auto-starts in one-shot mode — 8520 quirk, both have). |
| $6 | TBLO | R/W | ✅ | ✅ | Timer B low. |
| $7 | TBHI | R/W | ✅ | ✅ | Timer B high. |
| $8 | TODLO | R/W | ✅ | 🟡 | Event counter byte 0 (bits 7-0). Archive implements read-latch + write-halt; current is plain storage. |
| $9 | TODMID | R/W | ✅ | 🟡 | Byte 1. Same gap as $8. |
| $A | TODHI | R/W | ✅ | 🟡 | Byte 2. Read LATCHES the full 24-bit snapshot. |
| $B | — | — | — | — | Not used. Reads return $FF. |
| $C | SDR | R/W | ✅ | ❌ | Serial Data Register. Shift register for keyboard byte stream (CIA-A). |
| $D | ICR | R/W | ✅ | ✅ | Interrupt Control. See §5. |
| $E | CRA | R/W | ✅ | 🟡 | Control Register A. All 8 bits archived; current only honours START/ONESHOT/LOAD bits. |
| $F | CRB | R/W | ✅ | 🟡 | Control Register B. Same shape as CRA plus ALARM bit (7) and extended INMODE (6:5). |

---

## 2. Timer A / Timer B

### 2.1 Count sources

| Source | CRA | CRB (TB) | In archive | In current |
|---|---|---|---|---|
| PHI2 (E-clock at master/10) | bit 5 = 0 | bits 6:5 = 00 | ✅ | ✅ |
| CNT external pulses | bit 5 = 1 | bits 6:5 = 01 | ✅ | ❌ |
| Timer A underflow | n/a | bits 6:5 = 10 | ✅ | ❌ |
| Timer A underflow gated by CNT high | n/a | bits 6:5 = 11 | 🟡 (handled same as 10) | ❌ |

Current in-tree only ticks timers on its own `tick_e_clock()` call — no
CNT pin, no cascading. The archive handles all four source modes via
`tick()` (PHI2) and `cnt_pulse()` (CNT edge) helpers.

### 2.2 Modes and control bits

| CRx bit | Name | Effect | In archive | In current |
|---|---|---|---|---|
| 0 | START | 1 = counter counts | ✅ | ✅ |
| 1 | PBON | 1 = timer output drives PB6 (TA) or PB7 (TB) | ❌ | ❌ |
| 2 | OUTMODE | 0 = pulse on underflow, 1 = toggle | ❌ | ❌ |
| 3 | RUNMODE | 0 = continuous, 1 = one-shot | ✅ | ✅ |
| 4 | LOAD | Strobe: force counter = latch. Reads back as 0. | ✅ | ✅ |
| 5 (CRA) | INMODE | 0 = PHI2, 1 = CNT | ✅ | ❌ |
| 5-6 (CRB) | INMODE | Four combinations (see §2.1) | ✅ | ❌ |
| 6 (CRA) | SPMODE | 0 = SDR input, 1 = SDR output | 🟡 (state tracked, not driven) | ❌ |
| 7 (CRA) | TODIN | 0 = 60 Hz TOD, 1 = 50 Hz TOD — unused on Amiga (TOD is VSYNC/HSYNC) | ❌ | ❌ |
| 7 (CRB) | ALARM | 0 = TOD writes go to counter, 1 = alarm | ✅ | ❌ |

### 2.3 Quirks

| Quirk | HRM? | Archive | Current | Notes |
|---|---|---|---|---|
| $0000 visible for one cycle before underflow flag | datasheet (8520 §2.3) | ✅ | 🟡 (flag raised on the zero-tick, not the tick after) | Archive models "count through zero, reload next tick". Current raises on same tick as the wraparound. |
| 8520-only: TxHI write in one-shot mode auto-starts | Amiga HRM Ch. 8; ⚠️ HRM-deviation from 6526 | ✅ | ✅ (added session 2026-04-20) | Source of the KS1.3 MICROHZ fix. |
| Timer read low-byte latches high byte until high read | HRM §F | ✅ | ❌ | Matches TOD-style atomic read. |
| LOAD strobe reads back as 0 | HRM §F | ✅ | 🟡 (cleared on write, but no read-side test) | |
| One-shot's START auto-clears on underflow | 8520 datasheet | ✅ | ✅ | |

---

## 3. Time Of Day (TOD)

24-bit binary counter. CIA-A pin = /VSYNC (50/60 Hz); CIA-B pin =
/HSYNC (~15.6 kHz PAL).

| Feature | HRM | Archive | Current |
|---|---|---|---|
| Binary count (not BCD) | §F | ✅ | ✅ |
| Wrap at $1000000 | §F | ✅ | ✅ |
| External pulse pin | §F | ✅ (`tod_pulse`) | ✅ (`tick_tod`) |
| Read MSB ($A) latches 24-bit snapshot | §F | ✅ | ❌ |
| Read LSB ($8) releases latch | §F | ✅ | ❌ |
| **Write-halt**: any TOD write stops the counter | §F ("TOD is automatically stopped whenever a write to the register occurs") | ✅ | ❌ |
| LSB ($8) write restarts the counter | §F ("will not start again until after a write to the LSB event register") | ✅ | ❌ |
| CRB bit 7 = ALARM routes TOD writes to alarm | §F | ✅ | ❌ |
| Alarm equality → ICR bit 2 (ALARM) | §F | ✅ | ❌ |
| Counter + alarm survive hardware reset | §F explicit | ✅ | ✅ (default 0, no reset hook exists yet) |

**Cross-emulator discrepancy:** vAmiga CIARegs.cpp stops the counter
only on TODHI ($A) write, not on TODMID ($9). This is a 6526-style
behaviour. The 8520 HRM Appendix F is unambiguous: *any* TOD clock
register write halts. Archive matches HRM (and the 8520 datasheet).

---

## 4. Serial Shift Register (SDR)

| Feature | HRM | Archive | Current |
|---|---|---|---|
| SDR register at $C stores pending byte | §F | ✅ | ❌ |
| Output mode (CRA bit 6 = 1): TA underflow generates CNT pulses, shifts byte out on SP | §F | ❌ (state tracked, no shift) | ❌ |
| Input mode (CRA bit 6 = 0): external CNT pulses shift bits in from SP | §F | 🟡 (has `receive_serial_byte` whole-byte helper) | ❌ |
| Byte-complete latches ICR bit 3 (SP) | §F | ✅ | ❌ |
| On Amiga CIA-A: SP connects to keyboard 6500/1 | HRM Ch. 8 | ✅ (via helper) | ❌ |
| On Amiga CIA-B: SP unused | HRM Ch. 8 | n/a | n/a |

The archive fakes SP input by injecting completed bytes rather than
bit-streaming; sufficient for keyboard-scancode delivery but not for
timing-accurate emulation of the keyboard handshake ACK pulse. If we
later want keyboard-glitch fidelity we'll need true bit-level input.

---

## 5. Interrupt Control Register (ICR)

| Feature | HRM | Archive | Current |
|---|---|---|---|
| 5 source bits: TA(0), TB(1), ALARM(2), SP(3), FLAG(4) | §F | ✅ | 🟡 (TA, TB only) |
| Write with bit 7 = 1 SETs mask bits in low 5 | §F | ✅ | ✅ |
| Write with bit 7 = 0 CLEARs mask bits in low 5 | §F | ✅ | ✅ |
| Read returns `flags \| (any masked → $80)`, then CLEARS flags | §F | ✅ | ✅ |
| `/IRQ` level output = `(flags & mask) != 0` | §F | ✅ | ✅ |
| FLAG pin falling-edge latches ICR bit 4 | §F | ✅ (`flag_falling_edge`) | ❌ |

The Amiga wires CIA-B FLAG to /INDEX from floppy — used for
one-per-revolution detection. CIA-A FLAG is unused in standard A500.

---

## 6. Control registers — Port-output side effects

| Feature | HRM | Archive | Current |
|---|---|---|---|
| CRA/CRB bit 1 (PBON) routes timer output to PB6/PB7 | §F | ❌ | ❌ |
| CRA/CRB bit 2 (OUTMODE) selects pulse or toggle | §F | ❌ | ❌ |

Neither archive nor current drives the port pins from timer output.
Not exercised by KS1.3 boot. Can defer until a game actually does it.

---

## 7. Reset state

Per HRM §F reset table.

| Field | Reset value | In archive `.reset()` | In current `.default()` |
|---|---|---|---|
| PRA | 0 (driven), pull-ups make reads $FF | $FF (data = $FF) | 0 |
| PRB | same | $FF | 0 |
| DDRA/DDRB | 0 (all input) | 0 | 0 |
| Timer A counter / latch | $FFFF | $FFFF / $FFFF | 0 / 0 |
| Timer B counter / latch | $FFFF | $FFFF / $FFFF | 0 / 0 |
| CRA / CRB | 0 | 0 | 0 |
| SDR | 0 | 0 | (absent) |
| ICR mask / flags | 0 | 0 / 0 | 0 / 0 |
| TOD counter | not affected by hardware reset | preserved | n/a (starts 0) |
| TOD alarm | not affected | preserved | n/a (absent) |
| TOD latched | released | released | (absent) |
| Timer read-high latched | released | released | (absent) |
| TOD halted | false | false | (absent) |

Current impl has `Cia::new() = default()` with zero timers; HRM says
$FFFF. Boot doesn't care (ROM programs them before use) but ROM code
*could* read a fresh counter and get an unexpectedly low value.

---

## 8. Amiga-specific wiring

### 8.1 CIA-A (`$BFE001` odd-byte mapping)

| Pin | Connection | In archive | In current |
|---|---|---|---|
| PRA bit 0 | OVL (output, drives Gary overlay) | n/a (archive doesn't know OVL) | ✅ |
| PRA bit 1 | /LED (output, power LED brightness) | n/a | ❌ |
| PRA bit 2 | /CHNG (input, disk changed) | n/a | ✅ (defaults 0 as of session 2026-04-20) |
| PRA bit 3 | /WPRO (input, disk write-protected) | n/a | ✅ (defaults 1) |
| PRA bit 4 | /TK0 (input, head at track 0) | n/a | ✅ (defaults 0) |
| PRA bit 5 | /RDY (input, drive ready) | n/a | ✅ (defaults 1) |
| PRA bit 6 | /FIR1 (joystick 1 fire / mouse right button) | n/a | 🟡 (defaults 1) |
| PRA bit 7 | /FIR0 (joystick 0 fire / mouse left button) | n/a | 🟡 (defaults 1) |
| PRB | Parallel port data | n/a | ❌ |
| SP  | Keyboard data | n/a | ❌ |
| CNT | Keyboard clock | n/a | ❌ |
| FLAG | — | n/a | ❌ |
| TOD pin | /VSYNC | ✅ (via `tod_pulse`) | ✅ (via machine calling `tick_tod`) |

### 8.2 CIA-B (`$BFD000` even-byte mapping)

| Pin | Connection | In archive | In current |
|---|---|---|---|
| PRA | Parallel handshake + RS-232 control lines (/CD, /CTS, /RTS, /DSR, /DTR, /DCD, /PRTRSEL) | n/a | ❌ |
| PRB bit 0 | /DKWD (disk write data) | n/a | ❌ |
| PRB bit 1 | /DKWE (disk write enable) | n/a | ❌ |
| PRB bit 2 | /DKSTEP (disk step pulse) | n/a | ❌ |
| PRB bit 3 | /DKDIREC (disk step direction) | n/a | ❌ |
| PRB bit 4 | /DKSIDE (disk side select) | n/a | ❌ |
| PRB bit 5 | /DKSEL0 (drive 0 select) | n/a | ❌ |
| PRB bit 6 | /DKSEL1..3 banked | n/a | ❌ |
| PRB bit 7 | /DKMOTOR (motor on) | n/a | ❌ |
| SP  | — | n/a | ❌ |
| CNT | — | n/a | ❌ |
| FLAG | /DSKINDEX (floppy index pulse) | ✅ (via `flag_falling_edge`) | ❌ |
| TOD pin | /HSYNC | ✅ | 🟡 (machine wires VBL to both, not HSYNC — acceptable for now since KS doesn't check CIA-B TOD) |

---

## 9. /IRQ chain — CIA → Paula → CPU

| Step | Archive | Current | Notes |
|---|---|---|---|
| (flags & mask) != 0 drives `irq_active` / `irq_pending` | ✅ | ✅ | Level-sensitive. |
| Paula edge-latches rising edge into INTREQ.PORTS (CIA-A) / INTREQ.EXTER (CIA-B) | n/a (archive doesn't wire Paula) | ✅ | See `machine-commodore-amiga-ocs/src/lib.rs` — already handled at machine level. |
| INTREQ.x + INTENA.x triggers CPU IPL | n/a | ✅ | Existing and working. |

---

## 10. Summary — scope of work for Phase 2 port

Sorted by observable impact on KS 1.3 boot / WB boot:

### Must-port (blocks Workbench boot)

1. **SDR + CIA-A SP → keyboard** — required for the boot selector and
   any keystroke input.
2. **FLAG pin (CIA-B) ← /DSKINDEX** — required for trackdisk's
   per-revolution timing once real disk DMA is in. Without it,
   trackdisk timeouts on read.
3. **CIA-B PRB disk control outputs** — required to drive the floppy
   drive once we have one.
4. **TOD read-latch + write-halt** — KS uses this for atomic
   set-system-time. Boot survives without it but clock drifts.
5. **Timer count source CNT / cascade** — required if any game uses
   CIA Timer B chained to Timer A for fine timing (common).

### Nice-to-port (accuracy improvements, no known boot impact yet)

6. **PBON timer output on PB6/PB7** — games occasionally use this for
   audio-rate signals.
7. **OUTMODE pulse/toggle** — minor, ties with PBON.
8. **Reset-to-$FFFF timers** — HRM-correct default.

### Strictly required cross-cutting wiring

9. **Parallel port (CIA-B PRA + CIA-A PRB)** — needs actual host
   peripheral modelling if we ever want printer support. Defer.

---

## 11. Known discrepancies between archive and HRM

None found. The archive matches HRM Appendix F everywhere I checked,
including the tricky `/TOD write-halt — any register triggers, only
LSB restarts/` behaviour that vAmiga gets wrong.

## 12. Known discrepancies between archive and current

None where both have the feature — the current is a strict subset of
the archive.

---

## 13. Test surface for Phase 1 characterization

The archive has 13 inline tests (`#[cfg(test)] mod tests` in its
`lib.rs`). For Phase 1 characterization (tasks #106-#109), expand
these into a proper `tests/` directory organised by concern:

- `tests/timer_modes.rs` — every CRA/CRB bit combination that
  touches timer count / source / LOAD / auto-start.
- `tests/tod.rs` — TOD counter, alarm, read-latch, write-halt.
- `tests/sdr.rs` — input/output/byte-complete.
- `tests/icr.rs` — all 5 sources, SET/CLEAR, read-clears.
- `tests/ports.rs` — DDR-gated reads, pull-up behaviour, wiring-
  specific inputs (disk sense).
- `tests/reset.rs` — post-reset field values match HRM.

All these run against the **archive crate first** — if any fail,
that's a real archive bug we find before porting.

Then each Phase 2 task re-uses the same test files by including the
ported types, guaranteeing parity.

# MOS 6526 CIA

Complex Interface Adapter — two 8-bit I/O ports, two 16-bit timers, time-of-day clock, 8-bit serial shift register, interrupt control. The C64 uses two of them (CIA1 for keyboard + joystick + IRQ, CIA2 for serial bus + VIC-II bank select + NMI). Also used by the Commodore 128, 1541 disk drive, and various other Commodore and third-party hardware.

## Crate

`mos-cia-6526` — **ported.** Landed immediately after the 6502 (`2d42f8b`) to validate the "chip has pin fields, machine inspects them" pattern at low risk before the VIC-II's bus arbitration. See the [C64 per-subsystem source map](../decisions/archives-as-source.md#c64) for source provenance. 23 unit tests cover Timer A/B countdown, ICR read-clear + IRQ pin assertion, Timer B cascade, TOD 50 Hz counting + BCD rollover + latch freeze, serial shift output/input modes, port A/B DDR masking + external input merging, and FLAG falling-edge detection.

## Port sources

Per [Archives as source](../decisions/archives-as-source.md#c64):

- **Primary:** `~/Projects/Emu198x-archive/crates/mos-cia-6526/src/lib.rs` (960 L). Already separates output registers from external input pins (`external_a` / `external_b`) — the mental model the new pin port needs.
- **Cross-reference:** `~/Projects/Emu198x-backup/systems/c64/src/cia.rs` (570 L). Flatter, simpler, useful when the March archive's ICR / timer cascade logic gets confusing.
- **April 2026 archive:** has no CIA crate — the April refactor dropped it.

## Pin contract

The 6526's register bus (`$DCxx` / `$DDxx`) is an address-decoded peripheral mapping that only the CPU accesses, so register reads/writes use methods (`read(&mut self, reg) -> u8` / `write(&mut self, reg, val)`). Everything else is a public field.

**Output pins (CIA → machine):**
- `irq: bool` — true when `icr_status & icr_mask & 0x1F != 0`. The machine routes CIA1's `irq` to the 6510's `irq` input and CIA2's `irq` to the 6510's `nmi` input.
- `pa: u8` — effective Port A pin state: `(port_a_reg & ddr_a) | (pa_in & !ddr_a)`. Recomputed by `tick()`, `write()`, and `read()`.
- `pb: u8` — effective Port B pin state, same formula.

**Input pins (machine → CIA):**
- `pa_in: u8` — external drive on Port A (default `0xFF`). For CIA2, the machine sets bits 6-7 from the IEC bus.
- `pb_in: u8` — external drive on Port B (default `0xFF`). For CIA1 keyboard scanning, the machine resolves `(pa, keyboard_matrix) → pb_in` before each CPU read of `$DC01`.
- `flag: bool` — FLAG input, level, high by default. A falling edge sets ICR bit 4. CIA1's FLAG is wired to the cassette read line; CIA2's fires on IEC byte-ready.

**Machine-side clock:** the CIA is ticked by `φ2` (≈ 985 kHz PAL, 1.023 MHz NTSC). The machine calls `cia.tick()` once per CPU cycle.

### Why the register bus is methods, not pins

The [`cpu-bus-interface.md`](../decisions/cpu-bus-interface.md) decision says "every chip with cross-chip bus visibility" exposes its bus state as fields. The CIA's register bus has no cross-chip visibility — only the CPU talks to it through its address decoder. The CIA's IRQ output, port pins, and FLAG input *do* have cross-chip visibility (CPU, keyboard matrix, IEC bus, cassette), so those are fields. The rule is about *what other chips can observe*, not about method vs field as a uniform style choice.

## Subsystems

### Timer A / Timer B

Each a 16-bit down-counter with a reload latch. Per control register:
- **Input source:** φ2 (system clock), `cnt` pin, timer A underflow (timer B only), or `cnt`-gated timer A underflow (timer B only).
- **Mode:** one-shot (stops on underflow) or continuous (reloads and keeps running).
- **Output:** toggle or pulse on PB6 (timer A) / PB7 (timer B). Optional.
- **Underflow:** sets the ICR bit. If the ICR mask has the source enabled, `irq` asserts.

The timer A → timer B cascade is the trap most implementations get wrong — timer B counts timer A *underflows*, not ticks.

### Time of Day (TOD)

48-bit BCD clock: hours (0-12 + AM/PM bit), minutes, seconds, tenths. Counts from an external 50 Hz or 60 Hz tick (selected by CRA bit 7). An alarm register triggers an ICR bit when TOD matches. Reads latch the clock to a shadow register to avoid tearing; writes latch input to a shadow until hours is written.

### Interrupt Control Register (ICR)

8 sources: Timer A underflow, Timer B underflow, TOD alarm, SP shift-register full/empty, FLAG pin, (two unused bits), IR (any interrupt asserted). Read-clear semantics: reading the ICR returns the current state *and clears it*, so the machine side must not eagerly peek at the ICR without honouring the clear.

### Serial shift register (SP)

8-bit register shifted out on `sp` clocked by `cnt`. Can be master (timer A drives `cnt` out) or slave (external `cnt_in`). Rare in C64 usage — mostly used by peripherals like the 1351 mouse. Port it for completeness; don't bikeshed the timing.

## Test plan (for the port session)

1. Timer A countdown at φ2 with 1:1 reload → IRQ asserts on underflow, ICR bit set, reading ICR clears.
2. Timer A one-shot mode → counter stops after underflow, timer A running bit in CRA clears.
3. Timer B counts timer A underflows → cascade mode, verify B ticks once per A underflow.
4. ICR read-clear → set two sources, read ICR, verify both bits cleared and IRQ deasserts.
5. TOD counts from 50 Hz tick → 10 ticks = 1/10s, 100 ticks = 1 second, BCD boundaries at 9→0.
6. TOD alarm → set alarm to 1 second from current, tick until match, verify ICR bit.
7. Port A/B output vs DDR → writes to PRA only affect bits where DDRA = 1; reads combine output bits with external input bits.
8. Serial shift in master mode → timer A drives shifts, ICR bit sets on full/empty.

## Known gaps (deliberate — follow-up work)

These are real hardware behaviours the March archive omitted and this port inherits. Each will be added when a machine-level test fails because of it:

- **Timer A/B → PB6/PB7 output** (CRA/CRB bit 1 + bit 2). The backup's CIA implements both toggle and one-cycle pulse modes. Not ported yet because nothing on the roadmap currently needs it; the C64 sound is SID, not timer-driven pulse trains.
- **Timer A/B startup delay** — real hardware takes 2 cycles from the start strobe to the first decrement. Affects cycle-exact timer period calculations.
- **Force-load pipeline delay** — real hardware takes 2 cycles for the latch-to-counter copy to land.
- **TOD alarm match** — ICR bit 2 is defined but never gets set because the archive has no alarm-match check. Adding it means: storing the alarm registers (steered by CRB bit 7), comparing `tod == tod_alarm` on every TOD advance, and setting the ICR bit.
- **Timer B CNT-gated-by-Timer-A mode** (CRB bits 5-6 = `10` or `11`). The CNT pin isn't exposed yet.

## Port decisions captured in the commit

- **Single struct, not CIA1/CIA2 wrappers.** The chip is identical; the IRQ routing difference lives in the machine layer. Simpler.
- **Full serde derives, no `#[serde(skip)]`.** CIA state is small and fully transient-safe — unlike the 6502's cycle state machine, which has a `#[serde(skip)]` for the mid-instruction cycle counter.
- **`read(&mut self, reg)` handles all side effects inline.** The archive had three separate methods (`read_icr_and_clear`, `read_tod_10ths_and_release`, `read_tod_hours_and_latch`) to work around `read(&self)`. The new API folds everything into one `read(&mut self, reg)` call — the caller doesn't have to know which registers are side-effectful.

## Related

- [Archives as source](../decisions/archives-as-source.md) — port-source decisions and per-subsystem table.
- [CPU bus interface](../decisions/cpu-bus-interface.md) — the pin-level contract the CIA port has to satisfy.
- [RULES.md](../../RULES.md) items 5–6 — cycle accuracy and the no-Bus-trait rule that applies to every chip, not just CPUs.

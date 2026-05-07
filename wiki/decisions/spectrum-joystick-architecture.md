# Decision: Spectrum joystick architecture (deferred)

**Status:** Deferred. Logged 2026-05-06 during 128K-class extraction. Not blocking Phase 1A SOLID foundations or D6.

**Drift trigger:** if you find yourself adding `kempston:` fields, joystick state, or `port & 0x00E0 == 0x0000` decoders to a machine struct, **stop and re-read this entry first.** That's the wart this captures.

## What's wrong today

Three things, all hardware-inaccurate:

1. **Kempston is hardcoded into `Spectrum128kClassCore`.** The struct carries a `pub kempston: u8` field and `io_read` decodes Kempston ports unconditionally. Real hardware: the Kempston Interface was an add-on plugged into the rear edge connector. It worked on the 48K, 128K, and grey +2. Code-wise, 128K and +2 emulate a "permanently attached Kempston" and the 48K emulates "no Kempston" — which is exactly backwards from how the Kempston was actually distributed.

2. **`Spectrum48k` has no joystick handling at all.** Real 48Ks were the most common host for a Kempston (rubber-key Spectrum + add-on cartridge was the canonical 1983-85 setup). The current code doesn't model this.

3. **No Sinclair Interface 2 mapping for the +2 / +2A / +2B / +3.** Those machines shipped with two built-in 9-pin joystick ports. Hardware-wise the ports are wired to the keyboard matrix as Sinclair Interface 2 emulation: joystick 1 → keys 1-5 (left, right, down, up, fire), joystick 2 → keys 6-0. Today the code has neither the physical ports nor the keyboard mapping.

The +2A/+2B/+3 also broke the rear edge connector pinout, so traditional Kempston interfaces *can't* attach there without an adapter — but the current `SpectrumPlus` crate (handling +2A/+2B/+3 via Model enum) almost certainly carries a `kempston` field too.

## What the right architecture looks like

**Kempston as a peripheral.** Move it out of any machine-internal struct.

- New crate `peripheral-kempston-joystick` (the `Peripheral` trait already exists in `common-sinclair-zx-spectrum`).
- Hosts the I/O decode (`port & 0x00E0 == 0x0000 && port & 0x0001 != 0`) and the joystick state byte.
- Attaches to the runtime via the same plumbing other peripherals use.
- 48K, 16K, 128K, +2, Spectrum+ can host one.
- +2A / +2B / +3 cannot (rear edge connector incompatible).

**Sinclair Interface 2 keyboard mapping.** Lives at the runtime input layer.

- Joystick events come in as host inputs.
- Runtime maps joystick directions/fire to keyboard matrix rows (just like keyboard events do).
- Applies to grey +2, +2A, +2B, +3 (the variants with built-in joystick ports).
- No machine-side change required — the machines already see keyboard input.

After both: every machine crate's `kempston: u8` field goes away. The 128K-class core's `io_read` drops the Kempston branch. The +2A/+2B/+3 machines (whatever D6 leaves them as) similarly shed their kempston fields.

## Why deferred

- Pre-existing wart, not introduced by today's 128K-class extraction. The extraction preserved the current behaviour as a pure refactor.
- Not blocking SOLID criterion 2 (variants in scope) — that just needs the variant crates to exist.
- Not blocking the catalogue (Phase 2) — Manic Miner, Knight Lore, Jet Set Willy don't use joystick.
- Real fix touches every Spectrum machine crate plus the runtime input layer plus a new peripheral crate. Substantial scope.

## When to come back

After Phase 1A foundations land (D6 split + Spectrum+ wrapper + runtime aliases + boot tests). Before Phase 2 catalogue authoring widens — specifically before any catalogue entry that needs a joystick (Renegade, Target Renegade, Saboteur II, lots of arcade ports) gets authored. If the catalogue starts hitting joystick-using titles, this jumps the queue.

The 128K-class extraction landed with `kempston` preserved as a private field of the core specifically so this fix is local — when we lift it to a peripheral, only the layer crate's `Spectrum128kClassCore`, the 48K-class core, and the +2A/+2B/+3 machines need to lose the field. No callers to migrate.

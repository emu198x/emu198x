---
title: Fix Wave Channel Cycle-Accurate Access
type: fix
date: 2026-04-12
---

# Fix Wave Channel Cycle-Accurate Access

## Overview

Three Blargg dmg_sound tests fail because our wave channel (CH3) wave RAM access model is too simple. We always redirect reads/writes to `wave_ram[sample_position / 2]` when CH3 is enabled. Real DMG hardware has a 1-T-cycle access window and a trigger corruption bug.

## Problem Statement

| Test | Issue | Hash |
|------|-------|------|
| 09-wave read while on | Reads return wrong byte (timing) | 0B1186 |
| 10-wave trigger while on | No corruption modelled | 8130733A |
| 12-wave write while on | Writes land on wrong byte (timing) | B566A6E9 |

## Proposed Solution

Three changes to `src/apu.zig`, all confined to the `Wave` struct and `APU.read`/`APU.write`.

### Fix 1: 1-T-cycle access window (tests 09 and 12)

On DMG, the CPU can only access wave RAM during the **exact T-cycle** that CH3's frequency timer expires and fetches the next sample. On all other T-cycles, reads return 0xFF and writes are dropped.

**Implementation:**

Add a `wave_just_read: bool` flag to the `Wave` struct. Set it to `true` in `Wave.tick()` on the cycle the frequency timer expires (when `period_timer == 0`). Clear it at the **start** of the next `Wave.tick()` call.

```
// src/apu.zig — Wave.tick()

fn tick(self: *Wave, wave_ram: *const [16]u8) void {
    self.wave_just_read = false;          // ← clear from previous cycle

    if (self.period_timer == 0) {
        self.period_timer = (2048 - freq) * 2;
        self.sample_position +%= 1;
        // fetch nibble...
        self.wave_just_read = true;       // ← set for this cycle only
    } else {
        self.period_timer -= 1;
    }
}
```

Then update `APU.read` and `APU.write` for FF30-FF3F:

```
// Read
if (self.ch3.enabled) {
    if (self.ch3.wave_just_read) {
        return self.wave_ram[self.ch3.sample_position / 2];
    }
    return 0xFF;
}
return self.wave_ram[addr - 0xFF30];

// Write
if (self.ch3.enabled) {
    if (self.ch3.wave_just_read) {
        self.wave_ram[self.ch3.sample_position / 2] = value;
    }
    return;  // silently dropped if not on the access cycle
}
self.wave_ram[addr - 0xFF30] = value;
```

**Why this works:** The APU ticks before the CPU in `GameBoy.tick()`. When the frequency timer expires, `wave_just_read` is set. The CPU's read/write on the same T-cycle sees the flag. On the next T-cycle, `wave_just_read` is cleared before any new timer check.

### Fix 2: Trigger corruption (test 10)

When CH3 is re-triggered while already playing AND the frequency timer is exactly 0 (about to clock), wave RAM is corrupted.

**Implementation in `Wave.trigger()`:**

```
fn trigger(self: *Wave) void {
    // Corruption only when re-triggering while already active
    if (self.enabled and self.period_timer == 0) {
        const offset = ((self.sample_position +% 1) / 2) & 0xF;
        if (offset < 4) {
            // First byte gets the byte at the current position
            wave_ram_ref[0] = wave_ram_ref[offset];
        } else {
            // First 4 bytes get the aligned 4-byte block
            const src = offset & 0xFC;  // align to 4
            wave_ram_ref[0] = wave_ram_ref[src];
            wave_ram_ref[1] = wave_ram_ref[src + 1];
            wave_ram_ref[2] = wave_ram_ref[src + 2];
            wave_ram_ref[3] = wave_ram_ref[src + 3];
        }
    }

    self.enabled = self.dac_enabled;
    if (self.length_timer == 0) self.length_timer = 256;
    self.period_timer = (2048 - freq) * 2;
    self.sample_position = 0;
}
```

**Problem:** `trigger()` currently takes `self: *Wave` but needs mutable access to `wave_ram`. Options:

a) Pass `wave_ram: *[16]u8` to `trigger()` (cleanest — the APU calls `self.ch3.trigger(&self.wave_ram)` from `writeFreqHi`).

b) Store a `wave_ram` pointer in the Wave struct (risky — pointer invalidation).

**Recommendation:** Option (a). Update `writeFreqHi` and the APU's write routing to pass `&self.wave_ram`.

### Fix 3: Trigger sample position behaviour

On trigger, `sample_position` resets to 0 but the first actual wave RAM read happens after the initial period expires, reading **nibble 1** (not nibble 0). The sample buffer retains whatever was in it before trigger.

Our current code already sets `sample_position = 0` on trigger, and the first `tick()` after trigger will advance to position 1 and fetch from there. This should be correct, but verify by checking whether `current_sample` is cleared on trigger or left as-is.

**Current code sets no value for `current_sample` on trigger** — it keeps the old value. This matches hardware (the DAC outputs the stale sample until the first period expires). No change needed here.

## Acceptance Criteria

- [ ] `09-wave read while on` passes (exact hash match)
- [ ] `10-wave trigger while on` passes (exact hash match)
- [ ] `12-wave write while on` passes (exact hash match)
- [ ] Tests 01-08, 11 still pass (no regression)
- [ ] Tetris still plays correctly with sound
- [ ] 67 unit tests still pass

## Implementation Order

1. **Fix 1 first** (wave_just_read flag) — this is the simplest and fixes 2 of 3 tests
2. **Re-run 09 and 12** to verify
3. **Fix 2** (trigger corruption) — this is test-10 specific
4. **Re-run all 12** to confirm 12/12

## Technical Considerations

- The `wave_just_read` flag lifetime is exactly 1 T-cycle. Since APU ticks before CPU in `GameBoy.tick()`, the flag set during APU tick is visible to the CPU tick on the same T-cycle, then cleared at the start of the next APU tick.
- The trigger corruption check `period_timer == 0` might need to be `period_timer == 1` depending on whether we check before or after the tick decrements it. Test against the ROM to verify.
- The corruption uses `(sample_position + 1) / 2` — the **next** byte position, not the current one.

## References

- [SameBoy apu.c](https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c) — authoritative reference implementation
- [Pan Docs Audio Details](https://gbdev.io/pandocs/Audio_details.html)
- [gbdev wiki Sound Hardware](https://gbdev.gg8.se/wiki/articles/Gameboy_sound_hardware)
- Blargg test ROMs: `gb-test-roms-master/dmg_sound/rom_singles/`

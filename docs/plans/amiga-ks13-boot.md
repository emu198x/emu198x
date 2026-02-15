# Amiga Kickstart 1.3 Boot Plan

## Context

cpu-m68k passes 317,500/317,500 single-step tests. Two Amiga emulator crates exist (`emu-amiga` and `emu-amiga2`), both excluded from the workspace, both depending on deleted legacy CPU crates. Neither compiles.

**emu-amiga2** has the right architecture: word-level `M68kBus` trait, CPU always ticks, chip RAM contention via `wait_cycles`. It has variant config (A500/A1000/A1200 presets), modular Agnus/Denise, headless + windowed modes.

**emu-amiga** has the wrong bus architecture (byte-level, CPU gating) but has useful extras: MCP server, input scripting, comprehensive keyboard mapping, environment variable tracing.

**Goal:** KS 1.3 boots to the "insert disk" screen. Verified with `--headless --frames 300 --screenshot`.

---

## Phase 0: Get emu-amiga2 compiling with cpu-m68k

### 0a. Update emu-amiga2/Cargo.toml

Change `emu-m68k = { path = "../emu-m68k" }` to `cpu-m68k = { path = "../cpu-m68k" }`.

### 0b. Update imports

Two files reference `emu_m68k`:

- `src/amiga.rs:13` — `use emu_m68k::{Cpu68000, M68kBus, FunctionCode}` → `use cpu_m68k::{Cpu68000, FunctionCode}`
  (Remove `M68kBus` — it's unused in this file)
- `src/bus.rs:16` — `use emu_m68k::bus::{BusResult, FunctionCode, M68kBus}` → `use cpu_m68k::{BusResult, FunctionCode, M68kBus}`

### 0c. Add accessor methods to cpu-m68k

emu-amiga2 calls methods that don't exist on `Cpu68000`. The `regs` field is `pub` but `state` is `pub(crate)`. Add to `cpu.rs`:

```rust
pub fn registers(&self) -> &Registers { &self.regs }
pub fn registers_mut(&mut self) -> &mut Registers { &mut self.regs }
pub fn is_halted(&self) -> bool { matches!(self.state, State::Halted) }
pub fn is_stopped(&self) -> bool { matches!(self.state, State::Stopped) }
```

`total_cycles()` and `set_ipl()` already exist.

### 0d. Add emu-amiga2 to workspace

Remove `"crates/emu-amiga2"` from the `exclude` list in root `Cargo.toml`.

### 0e. Verify

- `cargo check -p emu-amiga2` compiles clean
- `cargo test -p emu-amiga2` passes unit tests
- `cargo test -p cpu-m68k` still passes (regression check)

### Files changed

- `/Users/stevehill/Projects/Emu198x/Cargo.toml` (exclude list)
- `/Users/stevehill/Projects/Emu198x/crates/emu-amiga2/Cargo.toml` (dependency)
- `/Users/stevehill/Projects/Emu198x/crates/emu-amiga2/src/amiga.rs` (import)
- `/Users/stevehill/Projects/Emu198x/crates/emu-amiga2/src/bus.rs` (import)
- `/Users/stevehill/Projects/Emu198x/crates/cpu-m68k/src/cpu.rs` (4 accessor methods)

---

## Phase 1: Boot investigation

Run KS 1.3 headless (`--headless --frames 300 --screenshot`) and observe what happens. The CPU should execute ROM code. We need to find where it gets stuck.

### 1a. Add a boot trace test

Create `crates/emu-amiga2/tests/boot_trace.rs` with an `#[ignore]` test that:
- Loads KS 1.3 ROM from a known path
- Runs N frames, logging PC/SR/key registers at each instruction boundary
- Detects stuck loops (PC not changing for >1M ticks)

This is the primary debugging tool for the rest of the plan. Pattern: run → stuck → trace → identify missing hardware → fix → repeat.

### 1b. Expected boot sequence and likely blockers

KS 1.3 does, in order:

1. **Reset vectors** — read SSP/PC from overlay at $000000. Already works.
2. **ROM checksum** — read all 256K, verify sum. Should work (bus reads from ROM are correct).
3. **Memory sizing** — write patterns, read back. Uses CIA timer interrupts for bus timeout detection. **Potential blocker: CIA timer → Paula INTREQ → IPL → CPU interrupt chain must work.**
4. **Overlay clear** — write CIA-A PRA bit 0. Already works.
5. **exec.library init** — set up ExecBase at $000004. CPU writes to chip RAM.
6. **Display setup** — write DMACON, BPLCON0, DIWSTRT/DIWSTOP, DDFSTRT/DDFSTOP, colour registers, bitplane pointers via Copper list.
7. **"Insert disk" screen** — Copper drives the colour gradient, bitplane DMA fetches the hand graphic.

---

## Phase 2: Fix boot blockers (iterative)

These are the known issues, roughly in order of when KS 1.3 hits them. We fix each one as the boot trace reveals it.

### 2a. Bitplane DMA — fetches all planes in one slot (KNOWN BUG)

**Problem:** `do_bitplane_dma()` in `amiga.rs:236-248` fetches ALL active bitplanes in a single CCK slot. Real hardware allocates one slot per bitplane.

**Fix:** The DMA slot allocator already computes `pos_in_group` and only returns `SlotOwner::Bitplane` when `pos_in_group < num_bitplanes`. Change `do_bitplane_dma()` to fetch only the plane matching `pos_in_group`.

Two options:
- **Option A:** Extend `SlotOwner::Bitplane` to `SlotOwner::Bitplane(u8)` carrying the plane index. Clean but touches more code.
- **Option B:** Compute plane index in `do_bitplane_dma()` from `(hpos - ddfstrt) % 8`. Simpler.

Recommend Option A — makes the contract explicit.

**Files:** `agnus/mod.rs` (SlotOwner enum), `agnus/dma.rs` (allocate_variable_region), `amiga.rs` (do_bitplane_dma, match arm)

### 2b. Blitter busy flag

**Problem:** KS 1.3 may poll DMACONR bit 14 (blitter busy) after starting a blit. Current blitter stub has `is_busy() → false`, and DMACONR reads `agnus.dmacon & 0x03FF` which doesn't include bit 14.

**Fix:** DMACONR read should include blitter busy status. Since the blitter stub says "not busy", KS will see immediate completion. This is fine for boot — no actual blitting needed.

**File:** `bus.rs` — `read_custom_reg` DMACONR case, OR in `self.blitter.is_busy()` as bit 14.

### 2c. CIA interrupt delivery

**Problem:** CIA ticks at E-clock rate (every 40 crystal ticks). `irq_active()` is checked once per E-clock tick. If the CIA fires an interrupt and it's cleared before the next E-clock check, it's lost.

**Likely not a problem** for boot (timers count down slowly), but watch for it. If KS hangs during memory sizing, trace CIA timer state.

### 2d. Copper beam comparison accuracy

**Problem:** The Copper WAIT compares beam positions at 2-CCK granularity. The current implementation looks correct but hasn't been verified against a real Copper list.

**Diagnostic:** When display is still blank after fixing 2a, trace Copper execution: log every MOVE/WAIT instruction, check that COLORxx writes and BPLxPT writes happen.

### 2e. Other potential issues (fix as discovered)

- VPOSR Agnus ID (bit 8-14): KS 1.3 uses this to detect chipset. OCS returns $00, which is correct for A500.
- SERDATR reads: KS checks serial status during keyboard init. Current stub returns 0. May need TBE (transmit buffer empty) bit set.
- POTGOR: currently returns $FF00 (all buttons released). Should be fine.

---

## Phase 3: Merge useful emu-amiga features

After boot works, bring over the valuable parts of emu-amiga. These are nice-to-have for debugging and the Code Like It's 198x pipeline, not boot-blocking.

### 3a. Keyboard mapping

Copy `emu-amiga/src/keyboard_map.rs` → `emu-amiga2/src/keyboard_map.rs`. This provides host-key-to-Amiga-keycode mapping for windowed mode. Wire into the windowed event handler.

### 3b. Enhanced input queue

Merge `enqueue_text()` and `enqueue_auto_boot()` from `emu-amiga/src/input.rs` into `emu-amiga2/src/input.rs`. Enables scripted input for headless testing.

### 3c. MCP server (deferred)

Copy `emu-amiga/src/mcp.rs`, add serde/base64 deps. Wire into main.rs with `--mcp` flag. Useful for the Code Like It's 198x pipeline but not needed for boot verification.

---

## Phase 4: Rename and clean up

### 4a. Rename emu-amiga2 → emu-amiga

1. Delete `crates/emu-amiga/` (the old crate with wrong architecture)
2. `mv crates/emu-amiga2 crates/emu-amiga`
3. Update `Cargo.toml`: package name `emu-amiga`, lib `emu_amiga`, bin `emu-amiga`
4. Update internal imports (`emu_amiga2` → `emu_amiga`)
5. Remove both from workspace exclude list

### 4b. Update plan docs

- `docs/cpu-m68k-plan.md` — add "Status: COMPLETE" header
- `docs/decode-rewrite-plan.md` — add "Status: SUPERSEDED by cpu-m68k" header
- `docs/amiga-variants-plan.md` — update Phase A status

### 4c. Delete emu-amiga-legacy references

Clean up `docs/solutions/` module references from `emu-68000` to `cpu-m68k` if they cause confusion. Low priority.

---

## What we will NOT do

- **Blitter logic** — stub is sufficient for boot screen
- **Sprite DMA** — no mouse pointer needed for static "insert disk" screen
- **Floppy/disk** — "insert disk" appears without any disk
- **Audio** — no sound needed
- **ECS/AGA** — OCS only (Phase A of amiga-variants-plan)
- **Fast RAM / Autoconfig** — not needed for A500 KS 1.3

---

## Verification

**Pass criterion:** `cargo run -p emu-amiga2 -- --kickstart <ks13.rom> --model a500 --headless --frames 300 --screenshot ks13.png` produces a screenshot showing the KS 1.3 colour gradient and "insert disk" hand graphic.

**Secondary:** `cargo test -p emu-amiga2` and `cargo test -p cpu-m68k` pass.

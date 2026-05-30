---
title: blargg_nes_cpu_test5/official.nes — investigation notes
type: handoff
date: 2026-05-30
---

# blargg_nes_cpu_test5/official.nes investigation

Recorded while exercising the newly-fleshed-out NES MCP tools (`7e68c83`). The findings are non-trivial and worth keeping for whoever picks this up next.

## What the sweep saw

`nes_sweep` reported `official.nes` as TIMEOUT for the entire session. The earlier diagnostic probe (`add18be`'s commit body, since-deleted dump file) showed the CPU spinning at PC=$8003-$8005 from ~60M master ticks onward, which I interpreted as "stuck in an infinite loop waiting for something."

## What was actually happening (revealed by MCP tools)

`emu198x-nes --mcp` driven via JSON-RPC stdio:

1. **memory_read $8000 len=32** → `4C 5B 84 4C 03 80 …`. The byte sequence at `$8003` is `4C 03 80` = `JMP $8003`. This is the test's normal "freeze after exit" state, not an emulator bug.

2. **dump_nametable which=0** at the frozen state shows the actual test progress:
   ```
   Running tests...
   -----------------------------
   01-implied                        ← NO 0x00 marker
   02-immediate                    .
   03-zero_page                    .
   04-zp_xy                        .
   05-absolute                     .
   06-abs_xy                       .
   07-ind_x                        .
   08-ind_y                        .
   09-branches                     .
   10-stack                        .
   11-special                      .
   -----------------------------
   All tests complete
   ```

   The trailing `.` on each line is the test framework's `0x00` "passed" marker. Tests 02-11 all have it. Test 01-implied **doesn't**.

3. **memory_read $00F0 len=16** → `00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 FF`. Address `$00FF = 0xFF` after the run — same value as `cpu.nes` (which is the failing-`cpu_test5`-family signature). So the test runner detected a failure somewhere and stored a sentinel.

## What test 01-implied covers

Source: `assets/test-suites/nes-test-roms/blargg_nes_cpu_test5/source/01-implied.a`. The official build (which is what `official.nes` compiles) tests 22 implied-addressing opcodes:

- `ROL A` / `ASL A` / `ROR A` / `LSR A` (accumulator shifts)
- `TXA` / `TYA` / `TAX` / `TAY` (transfers)
- `INX` / `INY` / `DEX` / `DEY` (inc/dec)
- `SEC` / `CLC` / `SED` / `CLD` / `SEI` / `CLI` / `CLV` (flag set/clear)
- `NOP`

Tom Harte's 6502 corpus passes all 22 of these in ~2.56M tests — so the failure mode blargg catches is something Tom Harte doesn't probe. Candidates:

- **NES-wrapper / 2A03-specific behaviour** — the 2A03 has BCD disabled (`SED`/`CLD` still update the flag but ADC/SBC ignore D). If the test toggles D and runs ADC, the result must differ from a stock 6502.
- **`PHA` / `PHP` / `PLA` / `PLP` are NOT in test 01** (they're in test 10-stack, which passes).
- **Flag interactions during transfer instructions** — TXA/TYA/TAX/TAY update N+Z; if our implementation set V or C incorrectly, the test catches it. Tom Harte would too, so this is unlikely.
- **CRC method differs** — `instr_test.inc` updates a running checksum across many runs and compares against one expected u32. A single off-by-one in any sub-case breaks the CRC for that opcode.

## Suggested next steps

If picked up:

1. **Run the test against a known-good emulator** (Mesen2, FCEUX) and capture its trace through test 01. Compare every observable side-effect cycle by cycle until divergence.
2. **Use the new MCP `run_until_pc` + `step`** primitives to drive the failing emulator through the test sequence. When the failure point is identified, capture the opcode + register state — that's the bug.
3. The 0x00-marker write happens after each test's `check_crc`. So the failure is `check_crc` for test 01 — the CRC of a specific opcode doesn't match. Finding which opcode means running the test ROM with single-test selection or instrumenting our emulator to dump CRCs.

## Why this is worth recording

The MCP tool surface fleshed out in `7e68c83` (`query_cpu`, `memory_read`, `dump_nametable`, `run_until_pc`, `step`) turned a hidden timeout into a concrete, actionable diagnostic in about 5 MCP calls. The infrastructure is now in place to investigate any sweep failure / timeout the same way.

This investigation also surfaces a wider sweep-harness improvement: `$00FF == 0xFF` after a run is a reliable indicator of a `blargg_nes_cpu_test5`-family fail. Adding that to the multi-protocol grader would flip `official.nes` (and possibly `cpu.nes` — which currently reports "Failed" via the nametable grader anyway) from TIMEOUT to a properly classified FAIL.

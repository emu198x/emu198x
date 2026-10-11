# Game Boy Blargg OAM and interrupt-time verification

The DMG runtime passes three of eight OAM singles and fails five, plus the
aggregate. `interrupt_time` passes its normal-speed measurements but the full
ROM fails because it requires CGB speed switching. These are recorded failures,
not a claim that the newly wired suites pass.

## Corpus identity and rights

Source: [retrio/gb-test-roms](https://github.com/retrio/gb-test-roms/tree/c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15),
revision `c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15`. Each local ROM was
compared byte-for-byte with the pinned upstream file on 2026-10-11.
No ROM or third-party source is bundled. Use external storage under the
[test-ROM policy](../knowledge/decisions/test-rom-policy.md).

| Relative path | Bytes | SHA-256 |
| --- | ---: | --- |
| `oam_bug/oam_bug.gb` | 65536 | `21f0e7af73e525d14ca242ba28f97d4d381a40a61b3466dd6b4cae3e502e1ea1` |
| `oam_bug/rom_singles/1-lcd_sync.gb` | 32768 | `abc303a2c644528cbcf36e83f8020cad17fdb6abdf42940533c4f6c8830c530a` |
| `oam_bug/rom_singles/2-causes.gb` | 32768 | `3f9c0db75222aae7c47382fe814b3806735b9ef76c791c6c26985b55c82235b9` |
| `oam_bug/rom_singles/3-non_causes.gb` | 32768 | `33e474075def3c025ff7990af82f1effb31e83617fb88aa1ee401976c35b8978` |
| `oam_bug/rom_singles/4-scanline_timing.gb` | 32768 | `5527c97665c3ad5474e5bda53a7e8e629bb1d0cb8ec54e6a1b42946dc9835ec7` |
| `oam_bug/rom_singles/5-timing_bug.gb` | 32768 | `ca3500d92e77883a09e5bc560046ed3471ab362eca550689cd38a944dc558e4f` |
| `oam_bug/rom_singles/6-timing_no_bug.gb` | 32768 | `e5ea594ddc7ad9a0e3063c3d6b050a2a10239cd43a23012551d4985c3712ea70` |
| `oam_bug/rom_singles/7-timing_effect.gb` | 32768 | `b4bb86462a5701155d92ea7dafe826cbffae6faec2c48c10feef0a5f78154d6c` |
| `oam_bug/rom_singles/8-instr_effect.gb` | 32768 | `11b372a91f65dd3bd2570aafc6f26e201fc0f05955ba3bb57f4c17cd642f7b2d` |
| `interrupt_time/interrupt_time.gb` | 32768 | `1a8f8f3bc609a92a5838214c00906c81e7cc77b3b9a63be5a1ec6d513f234929` |

## Reproduce

Set `EMU198X_GB_BLARGG_ROOT` to the directory containing `oam_bug/` and
`interrupt_time/`. Each explicit run rejects an unset root, missing ROMs,
missing verdicts and timeouts. The OAM gate attempts all nine ROMs before
failing. Known failures remain ignored in ordinary CI with issue anchors;
explicit accuracy runs expose them as failures.

```sh
export EMU198X_GB_BLARGG_ROOT=/path/to/blargg
cargo test -p runtime-nintendo-game-boy --test phase2_verification blargg_oam_bug_passes_aggregate_and_all_singles -- --ignored --exact --nocapture
cargo test -p runtime-nintendo-game-boy --test phase2_verification blargg_interrupt_time_normal_speed_passes -- --ignored --exact --nocapture
cargo test -p runtime-nintendo-game-boy --test phase2_verification blargg_interrupt_time_full_rom_passes -- --ignored --exact --nocapture
```

Expected current exit codes: **101, 0, 101**, respectively. The first and
third are accuracy failures, never successful verification of the whole suite.
The third currently loads the DMG runtime: the CGB implementation must route
that gate to CGB when #324 lands.

## Result ledger — 2026-10-11

Hardware code base: `5d61f256b4ab5da068abd5006eaeee106dee28f8`.
The harness changes observation only. Frame counts below are runtime advances
of 17,556 M-cycles, not a comparison with the reference's frame calls.

| OAM case | Frame | Status | Final report |
| --- | ---: | ---: | --- |
| aggregate | 561 | 2 | 01:ok 02:02 03:ok 04:03 05:02 06:ok 07:01 08:02 |
| 1-lcd_sync | 26 | 0 | Passed |
| 2-causes | 31 | 2 | LD DE,$FE00 : INC DE |
| 3-non_causes | 77 | 0 | Passed |
| 4-scanline_timing | 37 | 3 | INC DE at first corruption |
| 5-timing_bug | 32 | 2 | Should corrupt at beginning of first scanline |
| 6-timing_no_bug | 64 | 0 | Passed |
| 7-timing_effect | 255 | 1 | 00000000; Failed |
| 8-instr_effect | 44 | 2 | INC/DEC rp pattern is wrong |

OAM accuracy follow-up: [#1728](https://github.com/emu198x/emu198x/issues/1728).
The signed cartridge-RAM status takes precedence over text; the harness now
retains the final text rather than only the initially printed title.

`interrupt_time` finishes at frame 29. Its visible console reports:

```text
00 00 00
00 08 0D
00 00 00
00 08 0D
7F8F4AAF
Failed
```

The author's [source](https://github.com/retrio/gb-test-roms/blob/c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15/interrupt_time/interrupt_time.s)
requires CGB and measures speed, interrupt request, then elapsed M-cycles.
The first two rows prove the normal-speed zero/13-cycle pair, including
interrupt dispatch plus vector JP and RET. They do not prove the full ROM
passes. The last two rows remain at normal speed because the runtime lacks
CGB; this remains tracked by [#324](https://github.com/emu198x/emu198x/issues/324).
This binary emits no observed serial/cartridge-RAM report. The harness reads
its visible ASCII tile console, including vertical scroll, and requires a
title and terminal verdict. Guest ROM bytes and RAM are never patched.

## Reference and validation limits

SameBoy `208ba4afabffab9edde416f2dbb8ae459e34adb8`, built from an unchanged
source archive and booted using its own boot ROMs, passes the OAM aggregate
and standalone 1–6 and 8 on DMG-B. Standalone 7 does not reach a verdict within
3,600 frame calls; it is not counted as passed. DMG-B matches the interrupt
output above; CGB-E reports speed 01 in the last two rows and passes.

This is software/reference evidence, not a physical-hardware measurement.
The primary observation record is
`198x/reference/by-topic/cpu-sm83/2026-blargg-oam-interrupt-observations.md`.
Full logs and the reference host are retained with the
[docs plan](https://github.com/emu198x/docs/blob/main/plans/2026-10-11-game-boy-blargg-oam-interrupt.md).
The local knowledge overview also records these results.

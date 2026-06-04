# Outstanding Work — Cross-System Rollup

Status as of 2026-06-03. Companion to
[`current-system-usability.md`](current-system-usability.md). Each section is
the live list of open items per machine, ordered roughly by user impact
within that machine. Items are tagged:

- **L** — hard blocker: the system can't reach its basic boot/usability goal
  until this lands (awaiting ROM/firmware, or boot incomplete)
- **A** — accuracy / correctness debt that doesn't block usability
- **S** — scope expansion (broader software / new machines / new hardware
  paths)

The **Spectrum SOLID engineering bar was met (2026-06-03)**, ahead of the
October public launch. `L` previously meant "October Spectrum launch blocker";
with that gate cleared it now carries its plainer meaning above (a hard
per-system blocker), and the Spectrum's own former-`L` items are recast as
`A`/`S` below.
The donor / extended systems are no longer "deferred behind the launch" — they
are the active engineering frontier.

Resolved items are kept here briefly only when they unblock something else
listed below.

**Family-wide as of 2026-06-03** (see
[`current-system-usability.md`](current-system-usability.md) § Capability
surfaces for the vocabulary):

- **Operational parity (Capture + Script + MCP) is landed across the whole
  family** — the 6 primary systems plus every donor extraction (rollout
  `31b49271`→`5cf1a0e2`, 2026-06-02). "Full shell parity" items below have been
  recast as the one surface that actually remains: the **native `wgpu`
  window**, which only the 6 primary systems have.
- **Borders** — the TV-visible CRT frame is now rendered across the affected
  chips (TMS9918, Sega VDP, the Atari chips, the inline VIC/VDG/Ace renderers;
  Phase 1.1–1.4). No longer active-area-only.
- **`zilog-z80-ctc` landed 2026-06-03** and is wired into the **Sord M5**,
  which now boots through the CTC (BASIC-I `Ready`, Dig Dug). The crate is
  general (4-channel, daisy-chain, both modes); wiring it into Memotech MTX
  and Tatung Einstein is the remaining reuse (each is separate port work, not
  automatic — and Einstein is also blocked on `western-digital-wd1770`).
- **Highest-leverage unblock now:** `western-digital-wd1770` (Tatung Einstein
  disk boot).

## ZX Spectrum — `emu198x-spectrum`

**Spectrum SOLID — engineering bar met (2026-06-03), ahead of the October
launch.** CPU surface in genuinely
good shape: Tom Harte 100%, ZEXDOC/ZEXALL all checkpoints, FUSE 1,351/1,356
with 5 documented disagreements, Patrik Rak `z80test` 6/6 with zero allowlist.
262/262 runtime tests pass. 11 variants boot to a working screen. The items
below are residual accuracy/scope debt, not blockers.

- **A — Strict PNG comparison for the 5 ULA / contention smokes against
  Spectron references.** The smokes currently compare against self-locked
  goldens; spec'd target is byte-equal against Spectron's
  `tests/Results/<name>_{48,128}.png`. Spectron renders 1224×968 with
  border + scaling, so the comparator needs a downscale-and-crop step
  before equality. The smokes are green against self-locked goldens today;
  this tightens the oracle, it does not gate the launch. See
  [`knowledge/tests/spectrum.md`](../../knowledge/tests/spectrum.md).
- **A — 4 residual FUSE block-I/O AF disagreements** on `INIR`, `OTIR`,
  `CPDR`, `OTDR` (X/Y undocumented flag bits at the final repeat
  iteration). WZ matches, T-states match, memory effects match; just the
  undoc bits. Resolution needs silicon-level evidence; not a launch
  blocker.
- **S — Scorpion ZS-256 screen rendering.** CPU-liveness boot test
  passes; the Service ROM doesn't paint standard screen RAM yet. Three
  concrete bugs in `machine-scorpion-zs256/src/memory.rs` identified
  against FUSE's `machines/scorpion.c`: page-select bit (`$1FFD` bit 0
  vs bit 4), ROM-select logic, and ROM 3 should be Beta Disk overlay not
  bank-selectable. The fixes interact and need to land together —
  research recorded for a future one-session attempt.

## Commodore 64 — `emu198x-c64`

Headless boot to `READY.` verified live 2026-06-01; disk autoload walks an
Impossible Mission D64 end-to-end through the IEC bus and 1541. CPU
oracles: Tom Harte 100%, Dormann functional pass, Lorenz 250/265. 71/71
active runtime tests pass.

- **S — Real-software autoload tests are gated on archive paths.** 8
  D64 autoload tests + 5 TAP autoload tests (Impossible Mission,
  Ghostbusters, Thomas the Tank Engine, Thing on a Spring, Thinker) sit
  `ignored` waiting for the local archive root to land. Tests are
  written; wire them once the archive path is settled.
- **S — `--autoload-disk` only types `LOAD"*",8,1`.** For non-autostart
  binaries the load completes and drops back to `READY.` Adding an
  `--autoload-run` (or an `8,1` → `RUN` extension) would smooth game
  launches to one command.
- **A — 15 Lorenz `cpu` skips need full C64 machine model** (CIA timer
  interaction, 6510 banking, KERNAL tape traps, IRQ delivery,
  cycle-observable `cputiming`, `finish` screen-clear). The 6510 zero-page
  port `cpuport` already flipped to PASS once the three pin classes were
  modelled.
- **A — Drive/tape workflows are flag-heavy.** Discoverability gap, not
  correctness gap. Could be folded into a single `--smart-autoload` that
  picks disk vs tape vs PRG by file extension.

## Nintendo NES — `emu198x-nes`

Tom Harte 100% (with one allowlisted opcode — `$AB` LXA/ATX uses Mesen's
stable model per the NES test-oracle decision); nestest 8991/8991;
155-ROM sweep at **135 PASS / 5 FAIL / 0 TIMEOUT / 15 VISUAL**. APU
length-counter timing and LXA both closed 2026-06-01.

- **A — `blargg_nes_cpu_test5` test 01-implied (2 ROMs).** Both `cpu.nes`
  and `official.nes` now fail uniquely on test 01 after the LXA fix.
  Probe at
  [`crates/machine-nintendo-nes/tests/cpu_test5_probe.rs`](../../crates/machine-nintendo-nes/tests/cpu_test5_probe.rs)
  confirms sub-tests 02-11 all carry `[OK]` markers. Test 01 covers 22
  implied-mode opcodes (ROL/ASL/ROR/LSR A, T(A/X/Y), IN/DE X/Y, the
  seven flag set/clear ops, NOP). A Rust port of blargg's CRC-32
  framework lives at
  [`crates/mos-6502/tests/blargg_01_implied_crc.rs`](../../crates/mos-6502/tests/blargg_01_implied_crc.rs);
  2/20 OFFICIAL_ONLY opcodes match (TXA, TYA), confirming the
  CRC + iteration order are correct. The remaining 18 likely diverge in
  `set_paxyso`'s PLP behaviour or first-iteration CPU state.
- **A — OAMDMA + DMC DMA cycle accounting** (`sprdma_and_dmc_dma` 0/2).
  OAMDMA is fixed 514 cycles in the machine layer; DMC sample DMA steals
  individual CPU cycles but doesn't interleave with an in-progress
  OAMDMA. Need: 513/514 by even/odd alignment + DMC interleave.
- **A — `cpu_timing_test6` protocol** (0/1). Settles at
  `$00F0 = 0x98`; protocol not understood (the `0x98` byte is the TYA
  opcode, which may be a hint but is not confirmed).
- **S — More mapper coverage.** Memory mapping, expansion audio, and
  scanline IRQ are wired for MMC5; broader mapper coverage and
  hardware-test cross-checking remain useful.

## Commodore Amiga — `emu198x-amiga`

Full `--model` matrix reachable from script mode as of `bc23bc8`
(2026-06-01): A1000 / A500 / A500+A501 / A500-Plus / A500-Maxed / A600 /
A1200 / A2000. A1200 + Kickstart 3.1 boots clean through Insert-Workbench
to a clean Workbench 3.1 desktop — no palette or geometry artefacts. CPU
oracles: 68000 100% Tom Harte (1M tests); 68010/68020 100% against Musashi
via `m68k-test-gen`.

- **A — Promote AGA Workbench 3.1 boot to an automated screenshot smoke.**
  The boot was verified manually this session (`--model a1200
  --kickstart kick31a1200.rom --disk workbench31.adf --frames 1800
  --screenshot aga_wb.png`). Locking a golden would catch regressions in
  the FMODE bitplane wide-fetch (`d31e46a`) and 68020 full-format EA
  decode (`369d50b`) paths.
- **A — Gayle for A600 / A1200.** Current Gayle wiring covers what
  Kickstart 3.1 needs to boot. IDE and PCMCIA paths are stub-level;
  broader software (e.g. an A1200 with a hard drive image) will need
  them properly modelled.
- **S — Broader software validation across OCS / ECS / AGA.** Workbench
  1.3 / 2.x / 3.1 desktops verified, but game/application coverage is
  thin. Pick representative titles per chipset and wire as headless
  smokes with screenshot artefacts.
- **S — Long-term scope (recorded, not active).** Apollo Vampire FPGA +
  AC68080, PiStorm, RTG framebuffer expansions — the trait surface was
  designed to accommodate non-Commodore CPUs / chipsets / dual-display
  from day one, but no implementation work is scheduled.

## Nintendo Game Boy — `emu198x-game-boy`

CPU oracle: 49,600 Adam Tennant SM83 single-step tests pass + 92 lib unit
tests. DMG-family verifier window works with `wgpu` `raw`/`lcd`/`crt`,
keyboard/gamepad joypad, scripts, snapshots, `.sav` battery-RAM sidecars.

- **A — Tune `lcd` filter against hardware references.** The LCD preset
  is wired but not calibrated against side-by-side photos. Game Boy is
  the obvious case for taking the LCD preset seriously.
- **S — Broader real-game smoke coverage.** Boot through known-good
  titles and lock screenshots so regressions get caught.

## Dragon 32 — `emu198x-dragon`

Native verifier window, real Dragon 32 BASIC ROM boot, mono audio pinned
to XRoar's level model, CAS / DragonDOS `.BIN` / PAK / VDK media paths,
PAK snapshot smokes, optional patched-XRoar screenshot comparisons.

- **A — DragonDOS VDK exact controller timing/status/write.** Initial
  P2 controller reads work; exact timing and write paths need filling
  in from observed real-software failures.
- **S — Real DragonDOS ROM + VDK software smokes** at the same bar as
  the CAS / PAK paths.

## Oric-1 / Atmos — `emu198x-oric-atmos` (new, 2026-06-01)

Tenth donor-codebase extraction. **Zero new chip crates** — reuses
`mos-6502` ✓, `mos-via-6522` (we already had a more complete impl
than the donor — 898 vs 839 lines, plus serde + detailed SR mode
types), and `gi-ay-3-8912` ✓. The Oric's custom video ULA ports
inline into the machine layer like the Electron's.

Fresh-write machine layer with the distinctive **AY-via-VIA wiring**
(VIA port A = AY data bus, CA2 = AY BDIR, CB2 = AY BC1; software
puts PCR into one of four `(BDIR, BC1)` modes per AY operation),
8×8 keyboard via VIA port B column select + port A row read,
TEXT and HIRES display modes with serial-attribute rendering, full
BBC-Micro-compatible 8-colour 3-bit RGB palette.

Strong cultural anchor in **France** — Loriciels and ESAT made the
Atmos a de-facto French home computer in the mid-1980s.

**Boots to BASIC (live, 2026-06-04).** With the ROM in place the
Atmos cold-starts cleanly — first try, no code changes — to its
canonical screen:

```
ORIC EXTENDED BASIC V1.1uk
1983 TANGERINE
  37631 BYTES FREE
Ready
```

The 16 KB BASIC 1.1 ROM is `orica/bas11_uk.rom` from MAME's `oric1`
romset (md5 `32026ca4edccecfd91f88b923a5ab629`), installed at
`~/.emu198x/roms/oric/oric.rom`. The boot test now asserts the banner
in TEXT screen RAM (`$BB80`).
- **A — Atmos RAM-under-ROM not fully wired.** 64 KB is allocated
  and writes go to RAM even at ROM addresses, but ROM still wins
  on reads (matching standard reset state). Bank-switching to
  expose the RAM at `$C000-$FFFF` for advanced software is not
  modelled.
- **A — TAP cassette loader.** Donor has Oric `.tap` parser
  ($16 sync / $24 marker / type + autorun + end + start + name
  + payload); not yet wired into our binary.
- **A — Display rendering** runs end-of-frame; mid-frame palette
  changes via serial attributes work within a line but not across
  scanlines mid-render.
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Atari 2600 — `emu198x-atari-2600` (new, 2026-06-01)

Twelfth donor-codebase extraction. **Two new chip crates** —
`mos-riot-6532` (477 LoC, 11/11 tests) and `atari-tia` (1204 LoC +
inline NTSC/PAL palette module, 13/13 tests). Reuses our `mos-6502`
(running as the 6507 — the pin-limited 13-address-line 6502).

Fresh-write machine layer wiring the 6507 to TIA + RIOT through
the 2600's distinctive **A12/A7/A9 address decode**: A12 = cart at
`$1000-$1FFF`; A12 = 0, A7 = 0 = TIA registers; A12 = 0, A7 = 1 =
RIOT (A9 picks RAM vs I/O+timer). Master clock = TIA colour clock
at 3.58 MHz NTSC / 3.55 MHz PAL; 6507 and RIOT tick every 3rd
colour clock; TIA WSYNC line halts the CPU until next hblank.

Cartridge bank-switching support: 2 KB / 4 KB (no banking), F8
(8 KB / 2 banks at `$1FF8/$1FF9`), F6 (16 KB / 4 banks at
`$1FF6-$1FF9`), F4 (32 KB / 8 banks at `$1FF4-$1FFB`). Reads or
writes to the hotspot addresses trigger bank switches.

**Live boot verified 2026-06-01** with the 1977 Atari Combat
cart (2 KB, NTSC). Renders the canonical olive-on-peach
two-tank playfield: red tank left, blue tank right, missile
indicator at top, peach borders. Gated smoke at
`crates/machine-atari-2600/tests/cart_boot.rs` (picks first
`.a26` / `.bin` from `~/.emu198x/media/atari-2600/`) passes (1/1).

Bug fixed during boot bring-up: the donor's `mos-riot-6532`
reset the timer prescaler + cleared `post_underflow` state on
**every read of INTIM** (`$0284`). Real 6532 silicon doesn't
touch the prescaler from a read; the timer free-runs
independently. Combat (and many 2600 games) polls INTIM in a
tight loop expecting the prescaler to keep decrementing — with
the donor's behaviour the prescaler was reset every read and the
game wait-loop never exited. Fix: reading INTIM clears only the
underflow flag; prescaler and post-underflow state are
free-running.

- **A — TIA cycle-perfect timing.** The TIA is famously hard.
  Pixel-level rendering works for normal cart code paths, but
  HMOVE quirks, RESP starfield edge cases, and audio mixing
  refinements are in the accuracy backlog. Bigger games (Pitfall,
  Adventure, Star Raiders) may surface issues this initial port
  doesn't catch.
- **A — Audio output unwired.** TIA AUDx registers latch but
  the binary doesn't drain or write a WAV.
- **A — Joystick / console-switch input** exposed on the
  machine via `set_joystick_input(byte)` and `set_switch_input
  (byte)` but the binary doesn't have a runtime interactive
  surface yet.
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Atari 5200 SuperSystem — `emu198x-atari-5200` (new, 2026-06-01)

Thirteenth donor-codebase extraction. **Three new chip crates** —
`atari-antic` (14/14 tests, display-list processor + DMA
controller), `atari-gtia` (9/9 tests, video output + player /
missile graphics + collision), `atari-pokey` (13/13 tests,
4-channel audio + paddle pot scanner + serial I/O). Same ANTIC
+ GTIA + POKEY chip family the 800XL / 130XE share — so this
extraction is the foundation for the rest of the 8-bit Atari
home-computer line.

Fresh-write machine layer (`machine-atari-5200`, 14/14 tests)
wiring the 6502 "Sally" to ANTIC + GTIA + POKEY through the
5200 memory map: 16 KB RAM at `$0000-$3FFF`, cart at
`$4000-$BFFF` (size-mirrored 4 KB / 8 KB / 16 KB / 32 KB), GTIA
`$C000-$CFFF`, ANTIC `$D400-$D5FF`, POKEY `$E800-$E9FF`, 2 KB
BIOS at `$F800-$FFFF` (cart's `$FFFC/$FFFD` mirror falls
through when BIOS is absent). Master clock = colour clock;
CPU + POKEY tick every 2nd colour clock = 1.79 MHz NTSC; ANTIC
processes one scan line at every 228-clock boundary and stalls
the CPU for its DMA budget at the start of each line.

Joystick wired through POKEY pots (0-228 each, 114 = centre)
via `set_joystick(x, y)`; fire button wired through GTIA TRIG0
via `set_fire(pressed)`.

**Boots end-to-end to the Pac-Man menu (2026-06-04)** with the
1982 Atari Pac-Man cart (16 KB, NTSC) + 5200 BIOS: ATARI logo →
`JMP ($BFFE)` handoff → the cart's title/menu screen ("1UP /
HIGH SCORE", the maze row, "PRESS START TO PLAY GAME") in the
correct colours. Gated smoke at
`crates/machine-atari-5200/tests/cart_boot.rs` runs 320 frames
(past the ~255-frame logo) and, when a BIOS is present, asserts
a real rendered frame (≥ 4 colours, ≥ 1000 non-background px);
without a BIOS it falls back to the looser cart-only check.

Three bugs fixed to get there:

- **16 KB carts now use the "two chip" (EE_16) decode.** The
  fresh-write (and the donor) mapped 16 KB carts linearly to
  `$8000-$BFFF`, so Pac-Man's entry vector (`$BFFE` → `$8386`)
  landed in the lower chip's `$FF` padding and the CPU executed
  garbage until it hit an unbalanced `RTS` (empty stack → `$0001`
  → `$FCA2` NMI storm). Real 16 KB 5200 carts are two 8 KB chips
  selected by CPU A15 (A13/A14 don't-care): lower 8 KB →
  `$4000-$7FFF`, upper 8 KB → `$8000-$BFFF`. Entry now lands on
  the upper chip's `SEI`. Guarded by `cartridge::sixteen_kb_two_chip_decode`.
- **ANTIC now DMAs from the full bus, not just RAM.** ANTIC was
  handed only the 16 KB RAM (`process_line(&self.ram)`, masked to
  `& $3FFF`), so a display list in cart ROM (`$9EDF`) or glyphs
  from the BIOS character set (`$F800`) were unreachable —
  uniform black. The machine now keeps a 64 KB `dma_mem` image
  (RAM mirrored live + cart + BIOS baked) and passes that, so
  ANTIC reads the DL, screen data, and char sets from wherever
  they live. ANTIC itself is unchanged (the 800XL already passes
  a 64 KB view).
- **ANTIC text modes 4–7 decode per the hardware.** The char
  renderer applied the mode-2 convention to every text mode, so
  the 5-colour modes (6/7) lost their glyphs and colour: the
  code's top two bits — which are the *colour* in modes 6/7 —
  leaked into the glyph index, turning coloured uppercase into
  lowercase ("PLAYER" → "player") and digits into wrong glyphs
  ("1" → garbage), while the 1bpp font was wrongly read as 2bpp.
  Now each text family decodes correctly: modes 2/3 hi-res 2-colour
  (high bit = inverse video), modes 4/5 4-colour (2bpp font, high
  bit picks COLPF3), modes 6/7 5-colour (6-bit glyph, top two bits
  pick COLPF0–3 for an 8-pixel 1bpp font). Double-height modes
  (5/7) now address the 8-byte font with the row halved. The
  Pac-Man menu — "PAC-MAN", "1 PLAYER GAME", "PRESS START TO PLAY
  GAME" — reads correctly, and the right-edge artifact (mis-placed
  narrow mode-6 text) is gone. Guarded by
  `mode_6_five_colour_text_uses_colour_bits` and
  `mode_4_four_colour_text_high_bit_selects_pf3` in `atari-antic`.

Remaining polish (display works; these are fidelity, not blockers):

- **A — Cycle-accurate WSYNC + DMA stealing.** Current model
  treats the DMA budget as a fixed CPU-cycle stall at the start
  of the line; real ANTIC interleaves DMA cycles through the
  scanline.
- **A — Audio output unwired.** POKEY buffer drained via
  `take_audio_buffer()` but the binary doesn't write a WAV.
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.
- **S — Atari 800XL.** Reuses ANTIC + GTIA + POKEY (now
  unblocked); adds MOS PIA-6520.

## Atari 7800 ProSystem — `emu198x-atari-7800` (new, 2026-06-01)

Fourteenth donor-codebase extraction. **One new chip crate** —
`atari-maria` (15/15 tests, 1009 LoC), the zone-based display
processor that replaces the 2600's race-the-beam model: MARIA
walks a Display List List (DLL) at every scanline, DMAs sprite /
tile data from RAM, and stalls the 6502C "Sally" for the DMA
budget. Reuses our `mos-6502` as the 6502C "Sally" (stock 6502
with Atari's HALT pin) and `mos-riot-6532` for joystick / console
switches / timer.

Fresh-write machine layer (`machine-atari-7800`, 18/18 tests)
wiring CPU + MARIA + RIOT + a thin TIA-audio register stub through
the 7800 memory map: TIA at `$0000-$001F` (audio only — MARIA
handles video), MARIA at `$0020-$003F`, zero-page RAM
`$0040-$00FF`, stack RAM `$0140-$01FF`, RIOT `$0280-$02FF`, main
RAM 4 KB at `$1800-$27FF` (mirrored to `$3FFF`), cart
`$4000-$FFFF`. Cartridge handling: flat 16 KB / 32 KB / 48 KB
mapping or 8 × 16 KB SuperGame banking with bank 7 fixed at
`$C000` and writes to `$8000-$BFFF` switching the middle window.
A78 header auto-stripped.

Joystick wired via RIOT port A bits 4-7 (active-low) through
`set_joystick(up, down, left, right)`; console switches (Reset /
Select / Pause) via RIOT port B through `set_console(...)`.

Gated cart-boot smoke at
`crates/machine-atari-7800/tests/cart_boot.rs` passes with TOSEC
Asteroids (1987)(Atari)(NTSC).a78 — cart loads, machine ticks
300 frames without panic, framebuffer correctly sized.

- **✅ Black screen fixed (2026-06-04) — it was a MARIA CTRL bit
  bug, not a missing BIOS.** Tracing Asteroids disproved the BIOS
  theory: the cart runs its own reset code at `$D000`, then spins at
  `$D06F-$D076` waiting for zero-page `$76`, which only its NMI
  handler (`$D2FC`) advances — and that NMI is MARIA's DLI. Root
  cause: the MARIA CTRL (`$3C`) bit map was wrong on three bits —
  DMA-enable read as bit 7 instead of `DM` bits 6:5, colour-kill as
  bit 6 instead of 7, Kangaroo as bit 1 instead of 2. So a game
  enabling DMA (`DM=10`, bit 6) read to us as "DMA off + colour-kill
  on", and MARIA never walked the display list → no DLI → no NMI →
  `$76` frozen → black. Corrected the bit positions against the
  MiSTer RTL; `$76` now advances and the frame renders (Asteroids:
  ~18 colours, ~5k non-background px, was a uniform black frame).
  The `cart_boot` smoke now asserts a rendered frame.
- **A — MARIA display-list fidelity.** Enabling DMA exposed that the
  renderer had never run on real content. The DL-entry parser was
  rewritten to the hardware format (2026-06-04), cross-checked against
  the MiSTer `DMA.sv`: 4- vs 5-byte header chosen *per entry* by `b1`
  (`& 0x5F == 0` ends the list, `& 0x1F != 0` is 4-byte else 5-byte),
  correct byte roles (`b0`/`b2` = addr low/high, palette + width in
  `b1` or `b3`, HPOS in `b3`/`b4`), and two's-complement width
  (`((!W) & 0x1F) + 1`, 1–32). The list now terminates naturally
  (`dma_cycles` returns to 0 on idle lines — the runaway is gone, not
  just capped) and the transparency unit test validates exact pixels.
  Remaining fidelity is graphics-mode coverage (160B / 320B/C/D,
  Kangaroo transparency) and a visual diff against a reference —
  sibling of the Atari 5200 ANTIC/GTIA work.
- **A — BIOS overlay (authenticity, not a blocker).** The 7800 BIOS
  (`$8000-$FFFF` overlay, INPTCTRL bit 2 disables it) is a separate,
  lower-priority feature — games boot straight from their own reset
  vector without it.
- **A — TIA audio synthesis.** The 7800 uses TIA only for sound;
  six registers are stored but no synthesis path is wired.
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Atari 800XL — `emu198x-atari-800xl` (new, 2026-06-01)

Fifteenth donor-codebase extraction — the last 8-bit Atari in
the donor. **One new chip crate** — `mos-pia-6520` (14/14 tests),
the MOS 6520 / Motorola 6821 Peripheral Interface Adapter used in
many early-1980s machines (here for joystick + console-key input
+ PORTB-controlled ROM banking). Reuses ANTIC, GTIA, POKEY now
that the 5200 has landed them, and our `mos-6502` as the 6502C
"Sally".

Fresh-write machine layer (`machine-atari-800xl`, 15/15 tests)
wiring CPU + ANTIC + GTIA + POKEY + PIA through the 800XL memory
map: 64 KB RAM, ROM overlays via PIA PORTB. Bit 0 = 1 enables OS
ROM at `$C000-$FFFF` (with the `$D000-$D7FF` I/O gap); bit 1 = 0
enables BASIC ROM at `$A000-$BFFF`; bit 7 = 0 enables a self-test
ROM at `$5000-$57FF`. Cartridges in `$A000-$BFFF` shadow BASIC,
16 KB carts cover `$8000-$BFFF`. Without OS ROM, the reset vector
is fetched from the cart entry point (cart-only boot).

I/O area at `$D000-$D7FF`: GTIA `$D000-$D0FF`, POKEY
`$D200-$D2FF`, PIA `$D300-$D3FF`, ANTIC `$D400-$D4FF`. Joystick
wired through PIA PORTA (active-low bits 0-3) via
`set_joystick(up, down, left, right)`; fire button through GTIA
TRIG0; console keys (START / SELECT / OPTION) through GTIA
CONSOL.

Scope of this slice is the **800XL model**: 64 KB RAM with
XL-style PORTB banking. The 400 / 800 (no XL banking) and 130XE
(extended bank-switched RAM) variants are deliberately deferred.

**Boots to BASIC `READY` — the furthest-advanced donor extraction
(2026-06-02 → 2026-06-03).** The full OS → SIO-timeout → BASIC
cartridge chain runs, the GR.0 text screen renders correctly, and
keys typed through POKEY reach the BASIC line editor. Seven boot
bugs were resolved end-to-end (PIA A0/A1 address cross-wire, ANTIC
NMIEN VBI/DLI bit swap, DL LMS/DLI decode, CHACTL cursor inversion,
GTIA hi-res text colour, GTIA CONSOL read/write split, POKEY
edge-latched serial transmit). Solution record:
[`docs/solutions/atari-800xl-sio-disk-boot-to-basic.md`](../solutions/atari-800xl-sio-disk-boot-to-basic.md).

Gated regression tests (all green, ROM-bundle-gated):
`os_boot.rs` (frame budget), `basic_boot_probe.rs`
(`basic_boot_programs_antic_and_gtia`, `boots_to_basic_ready` —
asserts LOMEM/VNTP set + `READY` screen codes, `keyboard_types_into_basic`
— types `PRINT 6*7` → `42`), and the binary-level
`keyboard_into_basic.rs` (end-to-end by key name through
`HeadlessSession`). Verified against the MiSTer Atari800 OS+BASIC
bundle at `~/.emu198x/roms/atari-800xl/`.

**MCP debug surface** (2026-06-02): `query_cpu` (+ halted),
`memory_read` (banked), `poke_byte`/`poke_word`,
`query_antic`/`query_gtia`/`query_pokey`/`query_pia`, `run_until_pc`,
`press_key`/`type_string`. Matches the Spectrum/Amiga "an agent can
drive the machine" bar. The one gap is `disasm`, parked on the
Asm198x session promoting its spec-driven 6502 disassembler into a
shared dependency-free crate (per
[`asm198x-and-shared-isa-spec.md`](../../../decisions/asm198x-and-shared-isa-spec.md)).

- ~~**A — BASIC ROM not bundled.**~~ **Closed 2026-06-02.** 8 KB
  `ataribas.rom` from the Atari800 MiSTer core (SHA-256
  `4988cb41121921f997ab17a59dd1909fece9273699eba0c6fbaafae104aa27b0`)
  at `~/.emu198x/roms/atari-800xl/ataribas.rom`; default-path
  resolver picks it up and the runtime boots through to `READY`.
- ~~**A — `READY` visual confirmation gated on render pipeline.**~~
  **Closed 2026-06-03.** The 800XL GR.0 text path renders correctly
  (DL LMS/DLI, hi-res text colour, CHACTL cursor). Note this is the
  GR.0 text mode; the **Atari 5200's partial-render gap is a
  different ANTIC mode path** (task #76) and is not closed by this.
- **A — POKEY audio synthesis unwired** in the binary.
- **A — XEX / disk loading not implemented.** Cart-only and
  cart-with-OS for now. SIO + ATR disk loading is the next Tier-1
  feature (the MCP `memory_read`/`run_until_pc`/chip queries now make
  the SIO protocol far more tractable to debug).
- **A — Snapshot deferred** (shared family pattern).
- **S — 130XE 128 KB extended-RAM banking.** PORTB bits 2-5
  drive the 4 × 16 KB extended banks; not modelled in this slice.
- **S — Atari 400 / 800 variants.** Same chip family, different
  RAM size, no XL banking; would only need a model-selector flag.
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02); the native `wgpu` interactive window is the
  remaining surface. With boot + keyboard + render working, the
  800XL is the strongest donor candidate for the first native
  window outside the primary six.

## Jupiter Ace — `emu198x-jupiter-ace` (new, 2026-06-01)

Sixteenth donor-codebase extraction. The Forth-instead-of-BASIC
home machine from Steven Vickers + Richard Altwasser (the team
behind the ZX Spectrum's ROM). **No new chip crate needed** —
the Ace is a Z80A + 8 KB ROM + simple character display, with
the keyboard scanned by the same 8 × 5 matrix protocol as the
Spectrum.

**Boots to its cursor (live, 2026-06-04).** The Ace cold-starts to
the canonical blank screen with the cursor block at the bottom left,
and typing renders characters from the copied font — verified by
pressing a key and seeing a non-space cell appear.

Fresh-write machine layer (`machine-jupiter-ace`, 19/19 tests
including ported display + keyboard + input modules) wiring
[`zilog_z80::Z80`] through `bus_request()` to: ROM at `$0000-$1FFF`,
video RAM at `$2000-$23FF`, character RAM at `$2800-$2BFF` (128 ×
8-byte user-redefinable glyphs), and 1 KB user RAM mirrored across
`$3000-$3FFF`. Each of video and character RAM mirrors once into the
next 1 KB (`$2400` / `$2C00`) because the decode ignores A10. Port
`$FE` bit 0 clear drives keyboard read (row selector in high address
byte) on read and 1-bit beeper on bit 4 on write.

PAL display: 312 lines × 207 T-states/line = 64,584 T-states at
3.25 MHz, ~50.3 Hz. INT pulsed at the top of each frame for the
first 32 T-states.

The boot was unblocked by correcting the memory map. The crate had
video and character RAM both in the wrong place: it treated the
`$2400` video mirror as character RAM and routed real character RAM
(`$2800`) into general RAM. So the ROM's screen-clear (spaces) landed
in what the renderer read as the font — every cell showed glyph 0 as
a vertical line — and the font the ROM copies to `$2800` went into
dead RAM. The map now matches MAME's `cantab/jupace.cpp`, and the
font copy reaches the character generator. The 8 KB ROM (the standard
image, md5 `db6efdfd82cebdfbb493d85b1a5efc3c`) comes from the in-tree
`emulators/zx-spectrum/.../jupiter.rom`, installed at
`~/.emu198x/roms/jupiter-ace/ace.rom`.

- **A — Audio output unwired** in the binary (mono beeper
  buffer is taken via `take_audio_buffer()` but no WAV is
  written).
- **A — Snapshot deferred** (shared family pattern).
- **A — `.ace` snapshot load** — donor handled this; not yet
  ported (RAM dump at `$2000`).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Commodore PET — `emu198x-commodore-pet` (new, 2026-06-01)

Seventeenth donor-codebase extraction. The 1977/79 Commodore
business computer — one of the original "1977 trinity"
alongside the Apple II and TRS-80 Model I. **No new chip crate
needed** — reuses `mos-6502`, `mos-pia-6520`, `mos-via-6522`,
`motorola-6845` (all four already in the workspace).

Fresh-write machine layer (`machine-commodore-pet`, 7/7 tests)
wiring the 6502 through the public pin fields to: 32 KB RAM at
`$0000-$7FFF`, video RAM 2 KB at `$8000-$87FF`, BASIC ROM
`$C000-$DFFF`, Editor ROM `$E000-$E7FF`, Kernal ROM
`$F000-$FFFF`, PIA at `$E810`, VIA at `$E840`, CRTC at `$E880`.

PIA port A drives the keyboard column-select; row data is read
back on port B (10 × 8 matrix). The CRTC is pre-configured at
construction for 40-column or 80-column geometry; one master
tick = one 6502 cycle, CRTC ticks at the same rate (donor v1
simplification — real 80-column hardware clocks the CRTC at
2 MHz).

**Boots to BASIC `READY` (live, 2026-06-04).** The screen shows
the canonical `### COMMODORE BASIC ###` / `31743 BYTES FREE` /
`READY.` banner with the VICE 901465-* ROM set (BASIC 2 +
Kernal 2 + Editor 2N) and the 4 KB character ROM. Three bugs
stood between the "@" grid and a real boot, all fixed here:

1. **CPU never reset** — `Pet::new()` left the 6502 powered on
   at PC=`$0000`, so it ran the BRK there instead of cold-starting
   from the `$FFFC` reset vector. Added `cpu.reset()` at
   construction (the C64 / Atari 5200 do the same).
2. **Character ROM addressed with a 16-byte stride** — the PET
   glyph ROM is 8 bytes per character; `code * 16` made every
   glyph read its neighbour's bitmap and "spaces" fetch a
   non-blank glyph (the screen filled with horizontal-line
   noise). Now `code * 8 + scanline`, matching the VIC-II.
3. **CRTC pre-incremented its address counter** — `motorola-6845`
   advanced `ma` before the machine sampled it, dropping the
   first cell of every row (the banner lost its leading `*`). The
   CRTC now latches a separate `ma_output` for the character it
   is displaying and advances the counter behind it.

The editor's vertical-retrace spin-wait (`LDA $E840; AND #$20`)
is also wired now: VIA PB5 reflects the CRTC's `in_vertical_retrace`
state so the screen-write loop releases each frame.

- **A — Cassette / IEEE-488 unwired.** VIA exists but the
  external lines aren't connected.
- **A — Speaker unwired** (VIA CB2 piezo).
- **A — Snapshot deferred** (shared family pattern).
- **A — `.prg` / `.tap` load not implemented.**
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Commodore VIC-20 — `emu198x-commodore-vic-20` (new, 2026-06-01)

Twenty-second and final donor-codebase extraction. Commodore's
1981 mass-market home computer — first computer to sell over a
million units, designed to a $300 price point with Robert
Yannes' MOS 6560/6561 VIC handling video AND audio on a single
chip.

The donor shipped a VIC 6560/6561 inline (187 LoC, text-mode-only
rendering with 16-colour ARGB palette and PAL/NTSC timing).
**Promoted 2026-06-02** to the shared `mos-vic-i` crate (commit
`82685f5d`); the machine now imports `mos_vic_i::Vic6560`.

Fresh-write machine layer (`machine-commodore-vic-20`, 9/9 tests
+ inline VIC + keyboard + input modules) wiring the 6502 through
public pin fields to the standard VIC-20 memory map: 1 KB low
RAM `$0000-$03FF`, 3 KB low expansion `$0400-$0FFF`, 4 KB main
RAM `$1000-$1FFF`, 24 KB high expansion `$2000-$7FFF`, 4 KB
character ROM `$8000-$8FFF`, VIC registers `$9000-$93FF`, 1 KB
colour RAM `$9400-$97FF`, cartridge block 5 `$A000-$BFFF`, 8 KB
BASIC `$C000-$DFFF`, 8 KB Kernal `$E000-$FFFF`.

**Boots to BASIC `READY` (2026-06-04).** The `**** CBM BASIC V2
****` / `3583 BYTES FREE` / `READY.` screen renders in the
canonical cyan-border / white-screen / blue-text colours. Gated
smoke at `tests/rom_boot.rs` (run with `--ignored`) asserts the
screen-colour register and the banner text in screen RAM.

Two bugs fixed to get there:

- **The CPU was never reset.** `Vic20::new` built the 6502 but
  never ran `cpu.reset()`, so it powered on at PC=`$0000`,
  executed the `BRK` there, and stormed in the KERNAL IRQ/BRK
  handler — the "display black" symptom. (The C64/5200 always
  reset their CPU in `new`.)
- **The memory map mirrored the C64's, not the VIC-20's.** BASIC
  was at `$A000-$BFFF` and the KERNAL was mirrored into
  `$C000-$DFFF`. The VIC-20 puts BASIC at `$C000-$DFFF` and the
  KERNAL at `$E000-$FFFF`, with `$A000-$BFFF` as cartridge space.
  So `JMP ($C000)` (start BASIC) read the wrong cold-start vector
  and derailed into an IRQ-return with an empty stack. Corrected
  the map; the earlier "live boot verified" was only
  runs-without-panic, never an actual boot.
- **A — Audio unwired** (VIC's 3 tone generators + noise).
- **A — Keyboard scan unwired.** VIA 6522 × 2 not implemented;
  donor stubbed too.
- **A — Joystick unwired.**
- **A — Cassette / IEEE-488 unwired.**
- **A — Snapshot deferred** (shared family pattern).
- **A — `.prg` / `.tap` load not implemented.**
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.
- ~~**S — Promote inline VIC to a chip crate.**~~ **Done
  2026-06-02** — `mos-vic-i` (`82685f5d`).

## Acorn Atom — `emu198x-acorn-atom` (new, 2026-06-01)

Twenty-first donor-codebase extraction. Acorn's £120 self-build
(1980) designed by Sophie Wilson and Steve Furber — the team
that would design the BBC Micro the following year.

The donor shipped an Atom-specific MC6847 VDG (265 LoC, embedded
64-glyph character set, text mode only) inline. **Migrated
2026-06-02** to the shared `motorola-vdg-6847` crate (commit
`a82fe9d7`), which now carries the Atom text model alongside its
Dragon/CoCo model. Graphics modes 1-5 are a known follow-up.

Fresh-write machine layer (`machine-acorn-atom`, 8/8 tests +
inline VDG/keyboard) wiring the 6502 through public pin fields
to: 2.5 KB base RAM (expandable to 12 KB), 1 KB video RAM at
`$8000-$83FF` mirrored to `$9FFF`, 24 KB combined ROM (BASIC1
`$A000`, FP `$B004`, BASIC2 `$C000`, OS `$D000`), VDG control
register at `$B000`, PIA 6520 at `$B001-$B003`. PIA port A
column-select; port B row data.

**Boots to its prompt (live, 2026-06-04).** The Atom cold-starts to
the `ACORN ATOM` banner with the `>` prompt and cursor. Two things
were needed:

1. **CPU reset.** Like the VIC-20 and PET, `AcornAtom::new()` never
   ran the 6502 reset sequence, so the CPU powered on at PC=$0000
   and never cold-started — the screen stuck on the uninitialised
   character grid. Added `cpu.reset()`.
2. **The combined ROM.** Assembled the crate's 24 KB blob from MAME's
   `atom` romset: `abasic.ic20`'s low 4 KB → BASIC (`$C000`), its
   high 4 KB → MOS (`$F000`), and `afloat.ic21` → floating point
   (`$D000`); the `$A000` utility slot is left empty. Installed at
   `~/.emu198x/roms/acorn-atom/atom.rom`. Verified the reset vector
   resolves into the MOS ($FF3F).
- **A — Graphics modes 1-5 not implemented.** VDG renders text
  mode only; graphics modes show solid green (donor stub).
- **A — Cassette / printer unwired.**
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Memotech MTX500 / MTX512 — `emu198x-memotech-mtx` (new, 2026-06-01)

Twentieth donor-codebase extraction. UK-built Z80A home
computer from Memotech (1983) — aluminium case, pro-grade
keyboard, MTX BASIC + Noddy + SuperPascal ROMs. Critically
respected; commercially overshadowed. **No new chip crate
needed** — uses `zilog-z80`, `ti-tms9918`, `ti-sn76489` (all
already in the workspace).

Fresh-write machine layer (`machine-memotech-mtx`) wiring CPU + VDP + PSG
through the MTX I/O ports and paging — the authoritative port map and `$00`
paging byte are recorded in
[`knowledge/systems/memotech-mtx.md`](../../knowledge/systems/memotech-mtx.md).

Clock model: CPU at 4 MHz; VDP at 5.37 MHz via Bresenham counter against the
CPU clock; PSG at 4 MHz with internal ÷16.

**Boots to BASIC `Ready` (2026-06-03).** Three findings, all from MEMU
(`github.com/Memotech-Bill/MEMU`), took the MTX from blank screen to the BASIC
prompt:

1. **Paging was wrong.** The donor read port `$00` bit 0 as "page 0 → RAM",
   swapping the executing OS ROM out the instant the power-on RAM-sizing loop
   wrote `1` — derailing into zeroed RAM (PC ≈ `$1A65`). Rewrote `Mtx::resolve`
   after MEMU `mem.c`: OS fixed at `$0000`, 16 KB RAM blocks paging the upper
   windows, `RELCPMH` CP/M mode.
2. **I/O map was wrong.** After `memu.c` `OutZ80`/`InZ80`: SN76489 is `$06`
   (donor had `$03`); the keyboard reads from **both** `$05` (sense low) and
   `$06` (sense high + country code) on the drive/sense model (`kbd2.c`).
3. **The ROM image was incomplete.** A stock MTX motherboard carries OS +
   BASIC + **ASSEM**; the cold-start `RST $28 #$50` system call runs from the
   ASSEM ROM (paged subpage 1). With OS+BASIC only it landed on `$FF` and
   reset-looped. The machine now takes an OS + paged-ROM image (8 KB OS + N×8 KB
   subpages); with OS+BASIC+ASSEM (24 KB) the boot completes, programs the VDP
   and CTC, and renders `Ready`. Gated smoke `tests/boot_trace.rs`
   (`boots_to_basic_ready`). Full map in
   [`knowledge/systems/memotech-mtx.md`](../../knowledge/systems/memotech-mtx.md).

- **✅ VDP interrupt via the Z80 CTC (2026-06-04).** `zilog-z80-ctc` (the crate
  proven on the Sord M5) is wired at ports `$08-$0B`, and the VDP `/INT` now
  feeds **CTC channel 0**'s `CLK/TRG`; the CTC's own INT output drives the Z80
  IRQ, replacing the direct VDP→IRQ line (`memu.c` `LoopZ80` → `ctc_trigger(0)`).
  IntAck vectors via `ctc.acknowledge()` (IM 2) and RETI (`ED 4D`) releases the
  daisy chain. The boot still reaches `Ready`, and the gated `boots_to_basic_ready`
  test now asserts the OS programs `$08-$0B` and channel 0 is running with
  interrupts enabled — positive proof the CTC is the live timebase, not inert.
- **A — Keyboard matrix not aligned to MEMU's grid.** The drive/sense *model* is
  correct (no-key + country read verified); the physical key→(column, sense-bit)
  mapping still needs aligning to `kbd2.c` for accurate typing.
- **A — Cassette in/out unwired** (`$03` out / `$03` in returns `0x03`).
- **A — Centronics printer not implemented** (`$00` in / `$04`).
- **A — Snapshot deferred** (shared family pattern); `.mtx` / `.run` load not done.
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Sinclair ZX80 + ZX81 — `emu198x-sinclair-zx80` / `emu198x-sinclair-zx81` (new, 2026-06-01)

Eighteenth and nineteenth donor-codebase extractions. Both
share **one new chip crate** — `sinclair-zx81-ula` (6/6 tests):
the ZX81 family ULA handling display generation (32 × 24 D_FILE
walk with character-ROM glyph decode), NMI timing, and the
keyboard half-row read protocol. The ZX80 uses the same ULA
hardware silicon — only the NMI wire and ROM size differ
between the two systems.

**ZX80** (`machine-sinclair-zx80`, 11/11 tests). Sinclair's
£100 launch (1980), around 100,000 units. No NMI wired to the
ULA — display is blanked while the CPU runs (FAST mode), shown
only during HALT (SLOW mode). 4 KB ROM at `$0000-$0FFF`
mirrored to `$1000-$3FFF` and `$8000-$BFFF`; 1 KB or 16 KB RAM
at `$4000-$7FFF` mirrored to `$C000-$FFFF`.

**ZX81** (`machine-sinclair-zx81`, 9/9 tests). The £49.95
follow-up (1981), ~1.5 million units. Pioneers NMI-driven bus-
stealing display: the NMI handler at `$0066` executes HALT,
and the ULA puts character-ROM data on the data bus during the
Z80's refresh cycles. NMI generator gated by an enable bit
toggled via OUT($FE) (on) / OUT($FD) (off). 8 KB ROM at
`$0000-$1FFF` mirrored to `$2000-$3FFF` and `$8000-$BFFF`.

Both share an identical 8 × 5 Spectrum-style keyboard matrix
scanned through port `$FE` with the row selector in the high
address byte.

Live boot verified 2026-06-01 with the ZESARUX-bundled
`zx80.rom` (4 KB) and `zx81.rom` (8 KB). Both render real
character output from D_FILE through the ULA pipeline.

- **A — SLOW-mode rendering not yet correct.** ZX80 shows the
  canonical FAST-mode display with the "K" cursor visible —
  but SLOW mode (the v1 ULA model) blanks the screen during
  CPU execution and renders only during HALT, which doesn't
  yet match real hardware behaviour.
- **A — `.p` / `.p81` snapshot load not implemented** for the
  ZX81 (donor didn't have it either).
- **A — Cassette in/out unwired.**
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Acorn BBC Micro Model B — `emu198x-acorn-bbc-micro` (new, 2026-06-01)

Eleventh donor-codebase extraction. **One new chip crate** —
`motorola-6845` CRTC ported alongside (417 LoC, self-contained,
8/8 tests). Reuses our `mos-6502`, `mos-via-6522` (×2 — one
System VIA at `$FE40` for sound + keyboard + IC32 addressable
latch, one User VIA at `$FE60` for Centronics + user port),
`ti-sn76489` PSG, plus an inline Acorn Video ULA.

Fresh-write machine layer with the SHEILA I/O page at
`$FE00-$FEFF`, 16 sideways ROM banks at `$8000-$BFFF` switched
via `$FE30`, 32 KB RAM at `$0000-$7FFF`, 16 KB MOS at
`$C000-$FFFF`, and the BBC-specific **IC32 addressable latch** —
the SN76489 PSG isn't memory-mapped, it's written by putting the
PSG byte on System VIA port A and then **pulse-falling the latch
bit 0** through a port B write encoding `(addr=value&7,
data=value&8)`.

CRTC VSYNC drives System VIA CA1 (sets the line so the VIA edge
detector latches the interrupt). System VIA + User VIA IRQs OR
into the CPU `irq` pin.

**Live-tested with Acorn OS v1.2** (16 KB, SHA-256
`b0ad5c0b2e7d5776cc65d643989c02c66f7823df1f5c1c528833588c5a3e7a07`,
from TOSEC `Acorn/BBC/Operating Systems/`). The MOS executes
through to its sideways-ROM scan and ends up selecting bank 15
(the conventional BASIC slot — same scan logic as the Electron).
Framebuffer stays at MODE 7 backdrop (black) because the OS
defaults to MODE 7 teletext and the **SAA5050 teletext chip is
absent** from this port.

- **L — Acorn BASIC II ROM still missing.** Same blocker as the
  Electron — TOSEC has neither under Acorn/BBC nor anywhere
  searchable. Without BASIC the OS shows the canonical "Language?"
  error in MODE 7. Source from Stairway to Hell.
- **A — SAA5050 teletext chip not modelled.** MODE 7 stays blank.
  The OS uses MODE 7 by default (BASIC then usually moves to
  MODE 0/1 etc); without SAA5050 we can't see the boot screen
  text even with BASIC loaded.
- **A — Keyboard scan via System VIA + IC32 latch + addressable
  output not wired.** Keyboard matrix is allocated; scan path
  through the OS reads the column data which the System VIA
  pulls from IC32-controlled lines. Not yet implemented; affects
  any key-driven boot path.
- **A — CRTC bus contention timing.** Donor and this port both
  run CPU at flat 2 MHz; real BBC has the famous 1 MHz / 2 MHz
  alternating per-cycle scheme.
- **A — Snapshot deferred** (shared family pattern).
- **S — Floppy disk** (Intel 8271 or WD 1770 — different variants).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Acorn Electron — `emu198x-acorn-electron` (new, 2026-06-01)

Ninth donor-codebase extraction. **Zero new chip crates** — the
Electron's custom ULA is the only non-CPU chip, and donor's ULA is
~250 LoC of `match` statements that port cleanly inline into the
machine layer. Reuses our `mos-6502` (the 2A03 sibling chip-crate
from NES).

Fresh-write machine layer with the BBC-Micro-compatible 8-colour
palette, eight display modes (0-7 except MODE 7 teletext which the
Electron's ULA doesn't implement), 14×4 keyboard matrix scanned via
the address bus on `$FE00` reads, VBlank + 100 Hz RTC IRQ sources,
sound generation through the ULA's tone counter, and ROM-page
register at `$FE05`.

**Boots to BASIC (live, 2026-06-04).** The screen shows the
canonical `Acorn Electron` / `BASIC` / `>` cold-start — white text
on black. The OS ROM (16 KB, SHA-256
`b63f851d79498f598999d923b7c9f62e2525c34f0b9cd2d4b328b89d622dcda4`)
comes from TOSEC; the language slot uses **Acorn BASIC II**, which
is byte-identical to the BBC Model B BASIC II ROM (md5
`2cc67be4624df4dc66617742571a8e3d`) — sourced from the in-tree
`emulators/bbc-micro/BBCMicro_MiSTer/roms/bbcb/basic2.rom` and
installed at `~/.emu198x/roms/acorn-electron/basic.rom`.

Reaching that screen took three ULA fixes, all validated against
MAME's `electron_ula` device:

1. **Palette decode.** The register format is scrambled and
   inverted, not a simple nibble. Each register *pair* feeds four
   logical colours, with red/green/blue drawn from non-contiguous
   bits; the ULA stores `written ^ 0xFF`. The old `(value >> 4) & 7`
   stub painted the whole screen red.
2. **Screen-start address.** `$FE02`/`$FE03` pack address bits
   A14-A6 (64-byte granularity), not a raw high/low byte pair. The
   naive decode put the MODE 6 base at `$0030` instead of `$6000`,
   so the renderer scanned out RAM garbage.
3. **Display layout.** The Electron stores each 8×8 cell as eight
   consecutive bytes; columns step by 8, the scanline is the low
   offset. Text modes (3, 6, 7) also space character rows 10 lines
   apart (eight glyph + two blank), giving 250 displayed lines. The
   old renderer used a raster stride and an 8-line pitch.

- **A — Keyboard read path.** The matrix is currently scanned at
  `$FE00`; the real Electron reads it through the paged region
  (`$8000-$BFFF` with ROM slot 8/9 selected). Boot doesn't need it,
  but typing into BASIC will until the paged-keyboard read lands.
- **A — ULA bus contention not modelled.** Real Electron CPU
  halves to 1 MHz during ULA RAM-fetch windows; this initial port
  runs CPU at a flat 2 MHz. Significant cycle-accuracy gap on a
  machine whose software is heavily sensitive to it (Elite, many
  scrollers).
- **A — Sideways ROM paging at `$FE05`** stores the page register
  but doesn't yet swap a paged-ROM array into the
  `$8000-$BFFF` window — only the default BASIC ROM is visible.
- **A — Cassette I/O via `$FE04`** is a write-stub.
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.
- **S — BBC Micro** is the natural next 6502-family extraction
  from this base. It reuses 6502 ✓ and SN76489 ✓, adds VIA-6522
  (new) and Motorola 6845 CRTC (new).

## Tatung Einstein TC-01 — `emu198x-tatung-einstein` (new, 2026-06-01)

Eighth donor-codebase extraction. **Zero new chips** — reuses
TMS9918A and AY-3-8910 (via `gi-ay-3-8912`). Fresh-write machine
layer with Einstein-specific port-`$21` ROM page-out (any write
flips the 8 KB X-TAL MOS ROM out of `$0000-$1FFF` and exposes the
full 64 KB RAM), AY-driven keyboard (8×8 matrix, row select on
AY R14 / port A, column read on port `$20`), and Z80 CTC channel 0
stub at `$28`.

- **L — BIOS boot reaches VDP init but no text output.** With the
  1983 X-TAL MOS v1.2 ROM (8 KB, SHA-256
  `401d0e0bf6f64ba82e68137525749171fbcf9bd9055a49e5ac9a47941a6a0ae1`)
  the BIOS sets the TMS9918A backdrop to blue and then hangs
  waiting for the **WD1770 floppy controller** that we don't
  model. Real Einstein hardware shows a "DISK FAIL" message or
  loads CP/M from disk; without the WD1770 the boot can't proceed
  to text. Gated smoke at
  `crates/machine-tatung-einstein/tests/bios_boot.rs` asserts the
  VDP-init stage (1024+ non-black pixels = display enabled with
  backdrop) so we know the chips are reachable.
- **A — Z80 CTC may also be required.** Channel 0 is stubbed at
  port `$28`; like Sord M5, the Einstein wires VDP /INT through
  the CTC for IM-2 vectoring. The X-TAL MOS does set IM 1 early
  (visible in the disassembly), so the immediate boot path is
  unlikely to need CTC vectors, but later software almost
  certainly will.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Cassette / printer ports** unwired.
- **A — Snapshot deferred**.
- **S — Native verifier window.** Capture + script + MCP parity landed (operational-parity rollout, 2026-06-02); the native `wgpu` interactive window is the remaining surface.

## Mattel Aquarius — `emu198x-mattel-aquarius` (new, 2026-06-01)

Seventh donor-codebase extraction. **Zero new chips** — Z80-only
machine with custom character-display rendering (320×192 = 40×24
8×8 cells, TEA1002 16-colour palette, character generator at the
upper 2 KB of the 8 KB Microsoft BASIC ROM). 8-row keyboard read
through port `$FF` with the row select on address lines A8-A15
(active-low). VBlank drives Z80 NMI (50 Hz PAL pulse).

**Live boot verified 2026-06-01** with the 1982 Microsoft Aquarius
BASIC ROM (8 KB, SHA-256
`277b2655a0599861302e6ec86b027c65a29e284fe7aadc296c4570498ab0e249`).
BIOS reaches its idle loop (PC ≈ `$1EA7`) and paints the
characteristic magenta-and-black title screen — the cold-init fill
character produces the iconic repeated-tile background, with the
title text rendered as dark glyphs over the magenta backdrop. Gated
smoke at `crates/machine-mattel-aquarius/tests/bios_boot.rs` (1/1).

Bug fixed during boot bring-up: the donor's source comment on the
colour byte layout claimed `bits 0-3 = foreground`, but the BIOS
actually writes `high nibble = foreground, low nibble = background`.
Corrected with a note pointing at the donor's misleading comment.

- **A — Per-scanline display rendering.** The current port renders
  the whole framebuffer in one call at end-of-frame. Mid-frame
  changes to char / colour RAM won't be visible until the next
  frame.
- **A — TEA1002 palette tuning.** Donor palette is plausible but
  not calibrated against real-hardware photos.
- **A — 1-bit speaker downsample to 48 kHz** is exposed via
  `speaker_bit()` but the binary doesn't write a .wav yet.
- **A — Mini-Expander AY-3-8910 stub.** Port `$FC` writes are
  swallowed; some games rely on the optional Mini-Expander PSG.
- **A — Snapshot deferred** (shared family pattern).
- **S — Cassette I/O** via port `$FE` not yet wired.

## Spectravideo SVI-328 — `emu198x-spectravideo-svi-328` (new, 2026-06-01)

Sixth donor-codebase extraction. **Zero new chips** — reuses
TMS9918A, AY-3-8910 (via `gi-ay-3-8912`), and Intel 8255 PPI exactly
as MSX1 does. Fresh-write machine layer with Spectravideo-specific
simpler memory map: 32 KB system ROM at `$0000-$7FFF` overlaid with
RAM via port `$97` bit 0, 16 KB RAM at `$8000-$BFFF` overlaid with
cart via port `$97` bit 1, 16 KB RAM at `$C000-$FFFF` always. I/O
window tightened to `$80-$97` with 11×8 keyboard matrix via the
8255 (row select on port C, column read on port B).

- **L — BIOS not yet available.** The 32 KB SVI-318/328 system ROM
  (BASIC + OS) is not in the TOSEC dump and not in any standard
  emulator-bundle path on this machine. Gated smoke at
  `crates/machine-spectravideo-svi-328/tests/bios_boot.rs` waits
  for one to land at
  `~/.emu198x/roms/spectravideo-svi-328/svi-328.rom`. Real BIOS
  ships with openMSX/blueMSX firmware bundles; can also be
  extracted from a real SVI-328.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Centronics printer is a write-stub** (`$90-$91`).
- **A — Cassette I/O.** Real SVI-328 software loads from cassette
  through PPI port C bits 4-7; not yet wired (port C bits 0-3 do
  drive the keyboard row select correctly).
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02); the native `wgpu` window is the remaining
  surface (and is the more useful target once a BIOS lands).
- **S — SVI-318 sibling.** Same chip stack, 16 KB RAM instead of
  32 KB, slightly different keyboard matrix. Mostly the same
  binary with a variant flag.

## Sega Master System — `emu198x-sega-master-system` (new, 2026-06-01)

Fifth donor-codebase extraction. Adds the **Sega VDP** (315-5124 /
315-5246) as a new chip crate — TMS9918A derivative with Mode 4
(4bpp tiles, dual 16-colour palettes from 64-colour pool, 8 sprites
per line, scroll registers, line interrupt counter, H/V counter
readback). Reuses SN76489 from the Coleco family. Fresh-write
machine layer with the **Sega mapper** (`$FFFC-$FFFF` bank
registers + cart RAM control), 8 KB RAM mirrored across
`$C000-$FFFF`, GG-specific extensions (`$00` START button + `$06`
PSG stereo), Pause→NMI line, no BIOS required.

**Live boot verified 2026-06-01.** Alex Kidd in Miracle World
(1986, US, 128 KB) boots straight to the canonical title screen on
first try after the cart-bank-masking fix landed —
"ALEX KIDD / IN MIRACLE WORLD" full Mode 4 multi-colour title,
character vignettes, "PUSH START BUTTON / © SEGA 1986" footer.
Gated smoke at `crates/machine-sega-master-system/tests/cart_boot.rs`
(picks first `.sms` from `~/.emu198x/media/sega-master-system/`)
passes (1/1).

- **A — Sega VDP only exposes `tick_scanline()`** (no per-dot
  tick), so the machine accumulates 228 T-states per scanline and
  issues one batched scanline tick at the boundary. More
  accuracy-relaxed than `ti-tms9918`'s per-dot tick. Refining
  `sega-vdp` to a per-dot model is the obvious next step.
- **A — Cart RAM at `$8000-$BFFF`** (when mapper control bit 3 is
  set) reads as `$FF` in this initial port; full SRAM
  write/read/persistence path needed for Phantasy Star, Wonder Boy
  III, Golvellius etc.
- **A — Sega mapper bank masking.** Real-hardware bug behaviour
  around non-power-of-two cart sizes not yet modelled; current
  impl uses `next_power_of_two() - 1` mask which is correct for
  the common power-of-two cart sizes (32 / 64 / 128 / 256 / 512 KB).
- **A — Line interrupt counter** wired through `vdp.interrupt` but
  programmer-side behaviour (R10 reload + status bit) needs
  validation against real software that scrolls split-screens.
- **A — YM2413 FM-PAC.** Mark III + some carts have an optional
  YM2413 FM synthesis chip mapped at `$F0-$F2`. Out of scope here;
  separate chip crate when needed.
- **A — Snapshot deferred** (shared family pattern).
- **S — Game Gear** is most of the way there — same chip stack,
  smaller 160×144 visible region inside the same VDP framebuffer.
  Stereo PSG via `$06` is wired; runtime exposes
  `SmsVariant::GameGear`. Lacks a `.gg` cart smoke test.
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02); the native `wgpu` window is the remaining
  surface.

## Sord M5 — `emu198x-sord-m5` (new, 2026-06-01)

Fourth donor-codebase extraction. Reuses TMS9918A + SN76489 from
ColecoVision / SG-1000; adds the `zilog-z80-ctc` chip crate.
Fresh-write machine layer with Sord-specific memory map (cart at
`$2000-$6FFF`, 4 KB RAM at `$7000-$7FFF`, optional cart RAM at
`$8000-$BFFF`) and the same correct 3:2 VDP-phase clock as
SG-1000 / MSX.

- ~~**L — BIOS boot does not complete.**~~ **Closed 2026-06-03.**
  The M5 now **boots through the CTC to a rendered screen** —
  BASIC-I reaches its `Ready` prompt and Dig Dug renders its title /
  play screen. Two faults were fixed together: (1) the `zilog-z80-ctc`
  chip crate now models the IM 2 vector path (the Monitor ROM arms
  CTC channel 3 as a counter off the TMS9918A `/INT` line and the CTC
  supplies the `$7006 -> $01DF` per-frame vector); (2) an **I/O port
  map error** — an I/O trace of the Monitor ROM showed the real
  assignments are CTC `$00-$03`, VDP `$10/$11`, PSG `$20`, where the
  donor map had VDP `$00`, PSG `$10`, CTC `$50`. Every CTC write had
  been landing on the VDP and every VDP write on the PSG, so the VDP
  never configured and the CTC never armed.
- **A — Keyboard ports provisional.** The keyboard strobe (`$30`) /
  column read (`$40`) are not yet trace-confirmed; the BIOS does no
  keyboard I/O on the boot path, so this didn't block boot. Confirm
  against the BIOS keyboard scan.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Snapshot deferred** (shared family pattern).
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02), now with `disasm` / `query_ctc` / `query_vdp` /
  `run_until_pc` / `io_trace` debug tools; the native `wgpu` window is
  the remaining surface.

## MSX1 — `emu198x-msx` (new, 2026-06-01)

Third donor-codebase extraction. Reuses TMS9918A from ColecoVision
and our existing `gi-ay-3-8912` PSG (software-equivalent to the
AY-3-8910 for MSX's joystick scheme); adds the **Intel 8255 PPI**
chip crate as the only new silicon. Fresh-write machine layer with
the **MSX-signature memory-slot system** (PPI port A → primary slot
per 16 KB page), an 11×8 keyboard matrix, two cart slots with
**MegaROM mapper support** (Plain / Konami / Konami SCC / ASCII 8 /
ASCII 16), and the **correct 3:2 VDP-dot-per-T-state phase clock**.
Headless binary `emu198x-msx` with `--bios`, `--cart`, `--mapper`,
`--region`, `--frames`, `--screenshot` flags. Gated BIOS-boot smoke
at `crates/machine-msx/tests/bios_boot.rs` waiting for a 32 KB
BIOS at `~/.emu198x/roms/microsoft-msx/msx.rom` (real BIOS from
TOSEC) or `cbios_main_msx1.rom` (free GPL C-BIOS replacement).

- **Live boot verified 2026-06-01.** The 1983 Microsoft US MSX
  System v1.0 + BASIC BIOS (32 KB, SHA-256
  `3b33130d959337be63182c4eae217797774b52322f8eb9e35ab20747412ed417`)
  boots cleanly to the canonical MSX BASIC prompt — `MSX BASIC
  version 1.0 / Copyright 1983 by Microsoft / 28815 Bytes free /
  Ok` plus the function-key strip — on a light-blue background.
  Slot 0 BIOS read, slot 3 RAM hydration, PPI port A slot select,
  TMS9918A text-mode render, and keyboard-matrix-quiescent BASIC
  init all verified through real BIOS code. Gated smoke at
  `crates/machine-msx/tests/bios_boot.rs` now passes (1/1).
- **A — TMS9918A scanline-batched render** (shared with Coleco +
  SG-1000; will resolve together).
- **A — Subslot expansion.** MSX1 doesn't need it; MSX2+ uses
  writes to `$FFFF` (when slot 3 is selected for page 3) to
  expand each primary slot into 4 subslots. Field recognised in
  the spec but disabled. Wire when targeting MSX2.
- **A — Joystick / cassette / printer ports.** PSG R15 selects
  joystick; PSG R14 reads joystick data. The hookup is in place
  on the chip side but no joystick input surface on the machine
  yet (host can poke registers via `psg_mut()` if needed). Cassette
  and printer through PPI port C bits 4-7 unwired.
- **A — Snapshot deferred** (shared pattern with Coleco + SG-1000).
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02); the native `wgpu` window is the remaining
  surface.
- **S — MSX2 / MSX2+ / TurboR.** V9938 / V9958 VDP, mapped RAM,
  YM2413 FM-PAC, subslots. Out of scope; current `machine-msx`
  is MSX1-only.
- **S — TMS9918 family expansion** continues to be cheap from
  here: Sord M5, Memotech MTX, Spectravideo SVI-328 all reuse
  TMS9918 + SN76489 (Sord/Memotech) or TMS9918 + AY-3-8910
  (SVI-328 same as MSX, basically).

## Sega SG-1000 / SC-3000 — `emu198x-sega-sg-1000` (new, 2026-06-01)

Second donor-codebase extraction landed: reuses the TMS9918A + SN76489A
chip pair from the ColecoVision extraction, no new chips. Fresh-write
machine layer with a **correct 3:2 VDP-dot-to-CPU-T-state phase
counter** (more accurate than ColecoVision's initial-port 3:1 ratio).
Headless binary `emu198x-sega-sg-1000` boots the canonical Tsukuda
Original "007 James Bond" Othello Multivision cart to its
level-select title screen. Gated cart-boot smoke at
`crates/machine-sega-sg-1000/tests/cart_boot.rs` (picks first `.sg`
file from `~/.emu198x/media/sega-sg-1000/` or `~/Downloads/`).

- **A — TMS9918A scanline-batched render** (shared with Coleco; will
  resolve together).
- **A — Upgrade ColecoVision to the 3:2 phase counter.** SG-1000 has
  it right; ColecoVision's initial port runs the VDP 3× too fast.
  Mechanical fix once the SG-1000 model is comfortable.
- **A — SC-3000 keyboard.** `set_pause_pressed` already drives the
  Z80 NMI line; full SC-3000 8255 keyboard matrix not yet modelled.
- **A — Snapshot deferred** (shared pattern with ColecoVision).
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02); the native `wgpu` window is the remaining
  surface.
- **S — Cart-mapper support.** SG-1000 ceiling is 48 KB; some Sega
  Mark III / late SG-1000 carts have bank-switching mappers (Sega,
  Codemasters, Korean variants). Out of scope for initial port; SMS
  will likely share the mapper layer when it lands.

## ColecoVision — `emu198x-colecovision` (new, 2026-06-01)

First donor-codebase extraction landed: TMS9918A + SN76489AN chip
crates ported from `Emu198x-Oldest`, machine wiring fresh-written
against the pin-driven bus pattern, headless binary boots the
canonical 1982 ColecoVision BIOS to its title screen
("COLECOVISION™ / TURN GAME OFF / © 1982 COLECO"). Gated BIOS-boot
smoke at `crates/machine-coleco-colecovision/tests/bios_boot.rs`
(loads BIOS from `~/.emu198x/roms/coleco-colecovision/`, runs 200
frames, asserts a non-trivial framebuffer).

- **A — Initial-port clock ratios.** Inherited from the donor:
  VDP runs 3 dots per CPU cycle with NTSC/PAL frame budgets of
  `342 × 262` and `342 × 313` CPU cycles. Real ColecoVision
  master crystal is 10.738635 MHz (CPU ÷ 3 = 3.579545 MHz; VDP
  dot ÷ 2 = 5.369 MHz), so the actual ratio is 1.5 dots per CPU
  cycle, not 3. Frame structure still completes correctly; real-time
  speed is off. Tracked here, fix when wall-clock matters.
- **A — TMS9918A scanline-batched render.** Donor renders the
  full scanline on dot-wrap-to-0 rather than incrementing pixels
  through the active display. Misses mid-scanline register writes
  and per-pixel effects. Refine when test ROMs (e.g. ColecoVision
  diagnostics, SCV graphics tests) point at visible defects.
- **A — Snapshot story.** Deferred from the machine layer. The
  current `ColecoVision` struct is unsynchronised; a runtime layer
  with proper `serde(skip)` design for chip framebuffer + audio
  buffer hydration is the natural home for save/restore.
- **A — IM 1 IntAck.** Returns `$FF` (floating bus) — matches BIOS
  expectation of `RST 38h` fetch. Real-hardware behaviour with a
  cartridge that drives the data bus during IntAck is unverified.
- **S — Native verifier window.** Capture + script + MCP parity
  landed (2026-06-02 — `--screenshot`/`--audio-capture`/`--script`/
  `--mcp`); the native `wgpu` window with keyboard/audio matching
  `emu198x-nes`/`emu198x-c64` is the remaining surface.
- **S — TMS9918 family expansion.** Same chip crate is the
  foundation for SG-1000, MSX-1, Sord M5, Memotech MTX, Spectravideo
  SV-328. Same SN76489 also feeds SG-1000, SMS (with Sega VDP),
  BBC Micro. Pick the next extraction by curriculum / scene value.

## Cross-system shared work

- **A — Shared `wgpu` filter preset calibration** against hardware
  references — LCD for Game Boy, CRT for the TV / monitor systems
  (Spectrum, C64, NES, Amiga, Dragon). The presets exist; the
  calibration step is the work.
- **S — `scripts/verify-current-systems.sh` as the single CI gate.**
  Already runs unit/integration tests + conditional local-asset
  smokes. Worth keeping it the entry point as smoke counts grow rather
  than fragmenting into per-system scripts.

## Roadmap-adjacent (not active)

Status of the Emu198x-Oldest donor codebase extraction.

**Twenty-two already extracted** in this run — the donor
codebase is now fully harvested. See dedicated sections above:

| # | System | Live boot status |
|---|--------|------------------|
| 1 | ColecoVision | BIOS to title (live) |
| 2 | Sega SG-1000 / SC-3000 | Othello Multivision cart (live) |
| 3 | MSX1 | Microsoft BASIC (live) |
| 4 | Sord M5 | **Boots through CTC** (live) — BASIC-I `Ready`, Dig Dug renders |
| 5 | Sega Master System | Alex Kidd in Miracle World (live) |
| 6 | Spectravideo SVI-328 | **Awaiting BIOS** (32 KB MSX-style system ROM) |
| 7 | Mattel Aquarius | Microsoft BASIC (live) |
| 8 | Tatung Einstein TC-01 | **VDP-init only** — needs WD1770 floppy |
| 9 | Acorn Electron | **Boots to BASIC `>`** (live, 2026-06-04) — MAME-accurate ULA palette + screen-start + character-block display |
| 10 | Oric-1 / Atmos | **Boots to BASIC** (live, 2026-06-04) — clean first-boot with BASIC 1.1; `Ready` prompt |
| 11 | Acorn BBC Micro Model B | OS bank-scan reaches BASIC slot (live) — needs SAA5050 + BASIC for full |
| 12 | Atari 2600 | Combat playfield (live) |
| 13 | Atari 5200 SuperSystem | **Boots Pac-Man to its menu** (live, 2026-06-04) — two-chip 16K cart decode + ANTIC full-bus DMA + text-mode colour fix |
| 14 | Atari 7800 ProSystem | **Renders** (live, 2026-06-04) — MARIA CTRL-bit fix lets the DLI→NMI fire; Asteroids draws |
| 15 | Atari 800XL | **Boots to BASIC `READY`** (live) — GR.0 renders, keyboard types, MCP debug surface |
| 16 | Jupiter Ace | **Boots to cursor** (live, 2026-06-04) — MAME-accurate video/char RAM map ($2000 video, $2800 char, A10 mirrors); typing renders |
| 17 | Commodore PET | **Boots to BASIC `READY`** (live, 2026-06-04) — CPU reset + 8-byte char-ROM stride + CRTC address-latch fix |
| 18 | Sinclair ZX80 | Boot screen renders (live) — SLOW mode pending |
| 19 | Sinclair ZX81 | Boot screen renders (live) |
| 20 | Memotech MTX500/512 | **Boots to BASIC `Ready`** (OS+BASIC+ASSEM); Z80 CTC wired at $08-$0B with VDP /INT → ch0 (2026-06-04) |
| 21 | Acorn Atom | **Boots to prompt** (live, 2026-06-04) — CPU reset + 24 KB combined ROM assembled from MAME `atom`; `ACORN ATOM >` |
| 22 | Commodore VIC-20 | **Boots to BASIC `READY`** (live, 2026-06-04) — CPU reset + correct VIC-20 ROM map ($C000 BASIC / $E000 KERNAL) |

**Fourteen chip crates ported** as foundation:
`ti-tms9918`, `ti-sn76489`, `intel-8255`, `sega-vdp`, `motorola-6845`,
`mos-riot-6532`, `atari-tia`, `atari-antic`, `atari-gtia`,
`atari-pokey`, `atari-maria`, `mos-pia-6520`, `sinclair-zx81-ula`,
plus our pre-existing `gi-ay-3-8912` and `mos-via-6522` reused
across the family. Two more inline-only chip implementations land
inside their machine crates (Atom's text-mode MC6847; VIC-20's
6560/6561 VIC) — promotion to standalone crates is deferred until
a second consumer surfaces.

**Donor codebase: fully harvested.** The Amiga **AGA chipset
scaffold** (Agnus AGA + Denise AGA in the donor) is structurally
identical to our forward-port; we consult it as a reference
snapshot only — see [decisions/aga-donor-reference-only.md].
Nothing else substantive remains.

External-blocker holds:
- ~~Sord M5 boot needs a `zilog-z80-ctc` chip crate.~~ **Closed
  2026-06-03** — crate landed and wired; the M5 boots through the CTC.
  The crate is available to wire into Memotech MTX and Tatung Einstein
  (separate port work).
- Tatung Einstein full boot needs a `western-digital-wd1770` floppy
  controller for the X-TAL MOS disk wait.

Extract on demand when expanding scope; do not rewrite from scratch.

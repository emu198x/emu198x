# Emu198x

[![CI](https://github.com/emu198x/emu198x/actions/workflows/ci.yml/badge.svg)](https://github.com/emu198x/emu198x/actions/workflows/ci.yml)

Emu198x is a Rust workspace for building cycle-accurate vintage computer and
console emulators without shortcutting major hardware behavior.

It now spans **6 primary systems** — the full desktop-app experience, real
software, CPU oracles green — and **22 extended systems** extracted from earlier
codebases, each driveable headlessly while their native windows are built out:

- **Primary:** Sinclair ZX Spectrum (11 variants), Commodore 64, Nintendo
  Entertainment System, Commodore Amiga (OCS/ECS/AGA), Nintendo Game Boy,
  Dragon 32
- **Extended:** Atari (800XL, 2600, 5200, 7800), Acorn (BBC Micro, Electron,
  Atom), Sega (Master System, SG-1000), MSX1, ColecoVision, Commodore (VIC-20,
  PET), Sinclair (ZX80, ZX81), Sord M5, Memotech MTX, Mattel Aquarius, Tatung
  Einstein, Jupiter Ace, Spectravideo SVI-328, Oric-1/Atmos

## What This Project Is Trying To Do

The project aims to build emulators that model the real machines directly:

- pin-level CPU interfaces
- system-specific timing loops
- direct modeling of video, audio, DMA, media, arbitration, and peripherals
- deterministic headless runtimes with thin host shells above them

If a hardware path is not modeled accurately enough yet, the system is meant to
stay incomplete rather than faking the missing behavior.

## Capability surfaces — the shared vocabulary

Every system is described against the same five surfaces. "Parity" claims mean a
specific subset of these — name the surface, not a vague level.

- **Window** — native `wgpu` interactive window (`raw`/`lcd`/`crt` presets),
  real-time keyboard/gamepad input, live audio. The full desktop-app experience.
- **Capture** — headless PNG screenshot, WAV audio capture, video record at any
  frame boundary.
- **Script** — `--script PATH` programmatic control: run-frames / run-until-pc /
  run-until-mem-change, key/joystick input, memory read/write, snapshot.
- **MCP** — `--mcp` stdio server exposing the script surface plus per-chip
  `query()` paths as tools, so an agent can drive and inspect the machine.
- **Boot** — how far real firmware/software actually gets today.

Two tiers follow from these surfaces:

- **Primary systems (6)** — Window + Capture + Script + MCP, real software, CPU
  oracles green. The launch and engineering-bar machines.
- **Extended systems (22, donor extractions)** — Capture + Script + MCP
  ("operational parity", landed 2026-06-02) but **headless — no native Window
  yet**. Boot status varies from full-software to awaiting-ROM.

The authoritative, continuously-updated state lives in
[`docs/status/current-system-usability.md`](docs/status/current-system-usability.md);
the cross-system rollup of remaining work is in
[`docs/status/outstanding-work.md`](docs/status/outstanding-work.md). The summary
below tracks them.

## Primary systems

The **ZX Spectrum SOLID engineering bar was met on 2026-06-03**, ahead of the
October public launch. The donor / extended systems are now the active
engineering frontier rather than post-launch side-work.

### Sinclair ZX Spectrum — `emu198x-spectrum`

11 variants boot: 16K, 48K, 48K+, 128K, +2, +2A, +2B, +3, Pentagon 128, Timex
TC2048, Timex TS2068. (Scorpion ZS-256 reaches CPU-liveness but not screen
output yet.) Shared `wgpu` native window with `raw`/`lcd`/`crt` modes, keyboard,
audio, TAP/TZX loading and autoload, and snapshots through the shared runtime.

CPU oracles green: Tom Harte 100%, ZEXDOC/ZEXALL pass, FUSE 1,351/1,356, Patrik
Rak `z80test` 6/6 zero-allowlist. 262/262 runtime tests pass; all 8 boot goldens
green; 6 ULA/contention TAPs wired as smokes across 48K and 128K. Residual
debt is accuracy/scope only (strict Spectron PNG comparison for 5 ULA smokes;
Scorpion ZS-256 screen rendering) and does not gate the launch.

### Commodore 64 — `emu198x-c64`

Interactive verifier window (shared `wgpu`, `raw`/`lcd`/`crt`), keyboard, audio,
PRG/BAS/T64 import, TAP autoload, an optional live 1541/`D64` path, physical
gamepad input, and a host-key joystick mode for port 2. Headless boot to
`READY.` is verified; disk autoload walks an Impossible Mission `D64`
end-to-end (`LOAD"*",8,1` → `SEARCHING` → `LOADING`) through the IEC bus and the
1541 drive.

CPU oracles: Tom Harte 100% (2.56M), Dormann functional pass, Lorenz 250/265 (15
hardware-dependent skips). 71/71 active runtime tests pass; 13 software-autoload
tests sit `ignored` pending external D64/TAP archive paths.

### Nintendo NES — `emu198x-nes`

Native verifier window (shared `wgpu`, `raw`/`lcd`/`crt`) plus a headless
cartridge runner with screenshots, live audio / audio capture, keyboard/gamepad
input, scripts, snapshots, and local smoke-matrix reporting. Mapper support:
NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, Color Dreams, VRC2a, Action 53,
BxROM, NINA-001, Sunsoft-4, Camerica.

155-ROM test sweep: **135 PASS / 5 FAIL / 0 TIMEOUT / 15 VISUAL**; nestest
8991/8991; Super Mario Bros. renders. Remaining hard items: `blargg_nes_cpu_test5`
01-implied, OAMDMA + DMC DMA cycle interleave, `cpu_timing_test6`.

### Commodore Amiga — `emu198x-amiga`

Native verifier window (shared `wgpu`, keyboard/mouse, port-1 joystick/gamepad,
live Paula audio) plus a headless Kickstart/Workbench runner with the full
A1000 / A500-family / A600 / A1200 (AGA) / A2000 model matrix reachable via
`--model`. A1200 + Kickstart 3.1 boots to the Insert-Workbench prompt, and
**Workbench 3.1 mounts to a clean desktop** with no palette or geometry
artefacts. DF0 `ADF`, screenshots, audio capture, and scripted input all work.

CPU oracles green: 68000 100% Tom Harte (1M tests); 68010/68020 100% against
Musashi (via `m68k-test-gen`).

### Nintendo Game Boy — `emu198x-game-boy`

Native DMG-family verifier window (shared `wgpu`, `raw`/`lcd`/`crt`) plus a
headless cartridge runner with screenshots, live audio / audio capture,
keyboard/gamepad joypad input, scripts, snapshots, and `.sav` battery-RAM
sidecars. CPU oracle: 49,600 Adam Tennant SM83 single-step tests pass + 92 lib
unit tests.

### Dragon 32 — `emu198x-dragon`

Early native verifier window (shared `wgpu`) with live PIA-derived mono audio
pinned to XRoar's DAC/tape/cartridge-SND model, real Dragon 32 BASIC ROM boot,
semantic keyboard input, gamepad-to-analogue-joystick input, CAS media,
DragonDOS `.BIN` loading, ROM/DGN cartridge mounting with GMC banking, initial
DragonDOS VDK sector reads via the P2 controller path, PC-Dragon PAK snapshot
mounting, native `--autoload` over the ROM `CLOAD`/`CLOADM` paths, a beam-updated
MC6847 framebuffer, and optional patched-XRoar screenshot comparison.

## Extended systems (donor extractions)

All have **Capture + Script + MCP** parity and are **headless — no native Window
yet** (the shared `wgpu` verifier window is the remaining surface for every row).
Boot status is the differentiator. Per-machine open items live in
[`docs/status/outstanding-work.md`](docs/status/outstanding-work.md).

| System | Binary | Boot status |
|--------|--------|-------------|
| **Atari 800XL** | `emu198x-atari-800xl` | **Boots to BASIC `READY`** — full GR.0 render, keyboard typing, MCP debug surface (memory + ANTIC/GTIA/POKEY/PIA queries + `run_until_pc`). Furthest-advanced extraction. |
| Sega Master System | `emu198x-sega-master-system` | Alex Kidd in Miracle World → title screen (Mode 4) |
| MSX1 | `emu198x-msx` | Microsoft MSX BASIC prompt, clean |
| ColecoVision | `emu198x-colecovision` | BIOS rainbow logo + "TURN GAME OFF" |
| Sega SG-1000 | `emu198x-sega-sg-1000` | Othello Multivision cart → title |
| Mattel Aquarius | `emu198x-mattel-aquarius` | Microsoft BASIC title (magenta/black) |
| Atari 2600 | `emu198x-atari-2600` | Combat two-tank playfield |
| Atari 5200 | `emu198x-atari-5200` | Pac-Man title (partial render — shared 8-bit Atari pipeline) |
| Commodore PET | `emu198x-commodore-pet` | Char grid renders; full BASIC banner pending |
| Sinclair ZX81 | `emu198x-sinclair-zx81` | Boot screen renders |
| Sinclair ZX80 | `emu198x-sinclair-zx80` | Boot screen (FAST); SLOW-mode render pending |
| Acorn BBC Micro | `emu198x-acorn-bbc-micro` | OS bank-scan reaches BASIC slot; needs SAA5050 + BASIC II |
| Acorn Electron | `emu198x-acorn-electron` | "Language?" red error (needs Acorn BASIC II) |
| Commodore VIC-20 | `emu198x-commodore-vic-20` | ROM boots; display black until KERNAL screen-init |
| **Sord M5** | `emu198x-sord-m5` | **Boots through CTC** — BASIC-I `Ready`, Dig Dug renders |
| Memotech MTX | `emu198x-memotech-mtx` | ROM boots; display blank (needs CTC wiring) |
| Atari 7800 | `emu198x-atari-7800` | Cart accepts; BIOS-driven boot pending |
| Tatung Einstein | `emu198x-tatung-einstein` | VDP-init only — needs `western-digital-wd1770` |
| Jupiter Ace | `emu198x-jupiter-ace` | Awaiting ROM (8 KB Forth) |
| Acorn Atom | `emu198x-acorn-atom` | Awaiting ROM (24 KB combined) |
| Spectravideo SVI-328 | `emu198x-spectravideo-svi-328` | Awaiting BIOS (32 KB) |
| Oric-1 / Atmos | `emu198x-oric-atmos` | Awaiting BIOS (16 KB Tangerine) |

The shared next step for the whole tier is the native `wgpu` verifier window.
`zilog-z80-ctc` (landed 2026-06-03) unblocked the Sord M5 and is available to
wire into Memotech MTX and Einstein; the remaining missing chip crate is
`western-digital-wd1770` (Tatung Einstein disk boot).

## Principles

- Accuracy is the release criterion, not a future polish pass.
- Shared infrastructure stays narrow; machine timing stays family-specific.
- Host conveniences are allowed only above the emulation boundary.

Examples:

- `--autoload-tape` for Spectrum is a host workflow over the real ROM editor and
  real tape transport, not an instant-load trap.
- `--disk game.d64` + `--autoload-disk` for the C64 mounts a `D64` into the live
  drive-8 path (when a 1541 ROM is present) and types `LOAD"*",8,1`, letting the
  real DOS/IEC path perform the load — real drive-owned media, not a fake trap.
- `--tape game.cas` for the Dragon runner mounts real CAS media; the ROM still
  performs the `CLOAD`/`CLOADM` load over the emulated cassette input.
- `--snapshot game.pak` for the Dragon runner restores a PC-Dragon PAK snapshot
  as machine state; it is not treated as cartridge media.

## Building

The workspace tracks the latest stable Rust toolchain.

Typical commands:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/coverage.sh
```

The shared verification gate for the primary systems is:

```bash
scripts/verify-current-systems.sh
```

It runs in-repository unit/integration checks, then conditionally runs local
ROM/media smoke checks when the configured assets exist. Missing local assets
are recorded as `skip`, not `fail`, so the same command is useful on a fresh
machine and on a full reference workstation.

## Getting ROMs

Emu198x does not ship ROMs. You provide them yourself, and the legal position
varies a lot by platform. The extended systems each need their own
firmware/BIOS where the boot-status table says "awaiting ROM"; sourcing for the
primary systems is below.

### Sinclair ZX Spectrum

Amstrad granted permission in 1999 for the Sinclair ROMs to be freely
redistributed for non-commercial use. The canonical distribution lives in
**World of Spectrum's Sinclair ROM set** (`48.rom`, `128.rom`, `plus2.rom`,
`plus3.rom`).

### Commodore 64

The C64 KERNAL, BASIC, and CHARGEN ROMs are held by **Cloanto** through
Commodore IP succession; no free legal redistribution exists. The licensed
source is **Cloanto's C64 Forever**, which also includes the 1541 drive ROM used
by the optional live-drive path.

### Commodore Amiga

Kickstart and Workbench are held by **Cloanto**; the licensed source is
**Cloanto's Amiga Forever**. The OCS/ECS A500-family path targets Kickstart 1.3
(`kick13.rom`) and 1.2 (`kick12.rom`); the AGA A1200 path uses Kickstart 3.1.

### Nintendo NES

The NES has no system ROM — boot logic lives in the cartridge. For verification,
**Blargg's NES test ROMs** and **`nestest.nes` by kevtris** are freely
redistributable. For commercial cartridges, dump your own.

### Nintendo Game Boy

The DMG boot ROM is optional; the runner boots cartridges without it. For
commercial cartridges, dump your own.

### Dragon 32

Dragon Data dissolved in 1984; no current rights-holder sells licensed copies of
the BASIC ROM (`dragon32.rom`). Community archives host it under the abandonware
umbrella; this README does not link them.

## Running

These commands are intentionally minimal — they boot or load representative
software. Use `--help` on each binary and the system status pages for deeper
accuracy notes and the full flag set. Add `--no-default-features` to drop the
native window and run a binary purely headless (for screenshots, scripts, CI).

### Sinclair ZX Spectrum

```bash
cargo run --release -p emu198x-spectrum -- --rom 48.rom --tape game.tzx --autoload-tape
```

### Commodore 64

```bash
cargo run --release -p emu198x-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64 --autoload-disk
```

### Nintendo NES

```bash
# windowed
cargo run --release -p emu198x-nes -- game.nes

# headless screenshot
cargo run --release -p emu198x-nes --no-default-features -- --rom game.nes --frames 300 --screenshot nes.png

# Blargg $6000 assertion run
cargo run --release -p emu198x-nes --no-default-features -- --rom apu_test.nes --frames 3000 --assert-blargg
```

### Commodore Amiga

```bash
# OCS/ECS A500 + Kickstart 1.3 + Workbench 1.3
cargo run --release -p emu198x-amiga -- --model a500-a501 --kickstart kick13.rom --disk workbench13.adf

# AGA A1200 + Kickstart 3.1 + Workbench 3.1
cargo run --release -p emu198x-amiga -- --model a1200 --kickstart kick31a1200.rom --disk workbench31.adf

# headless screenshot
cargo run --release -p emu198x-amiga --no-default-features -- --model a500-a501 --kickstart kick13.rom --disk workbench13.adf --frames 2500 --screenshot amiga.png
```

### Nintendo Game Boy

```bash
# windowed
cargo run --release -p emu198x-game-boy -- game.gb

# headless screenshot
cargo run --release -p emu198x-game-boy --no-default-features -- --rom game.gb --frames 300 --screenshot gameboy.png
```

### Dragon 32

```bash
cargo run --release -p emu198x-dragon -- --rom ~/.emu198x/roms/dragon/dragon32.rom --tape game.cas --autoload --video crt
```

### Extended systems

Extended binaries are headless by default (no native window yet); add `--mcp`
for the agent surface, or `--frames`/`--screenshot` for a capture. Flags vary by
machine — check each binary's `--help`. For example:

```bash
# MSX1 headless capture
cargo run --release -p emu198x-msx -- --bios ~/.emu198x/roms/microsoft-msx/msx.rom --frames 200 --screenshot msx-boot.png

# Atari 800XL agent/MCP surface
cargo run --release -p emu198x-atari-800xl -- --mcp
```

## Verification Strategy

This repo does not treat "boots one thing" as sufficient proof. The verification
approach is layered:

- chip- and format-level unit tests first
- machine wiring tests second
- ROM/software regressions above that
- external reference suites where appropriate

Current high-water marks:

- **Z80** — Tom Harte 100%, `zexdoc`/`zexall` pass, FUSE 1,351/1,356, Patrik Rak
  `z80test` 6/6; Spectrum machine/software regressions over real ROM and tape.
- **6502** — Tom Harte 100% on the C64 path (2.56M), Dormann functional pass,
  Lorenz 250/265; NES nestest 8991/8991 and a 155-ROM sweep (135 PASS).
- **68000/010/020** — 68000 Tom Harte 100% (1M), 68010/68020 100% vs Musashi via
  `m68k-test-gen`; Amiga boots Kickstart and mounts Workbench 3.1 cleanly.
- **SM83** — 49,600 Adam Tennant single-step tests on the Game Boy core.
- **Shared MCP debug surface** (`io_trace` / `disasm` / `run_until_pc`) now drives
  the donor frontier — it cracked the Sord M5 port-map bug and is the tool for
  the "boots but black screen" extended systems.

Coverage exists as an audit signal, not as the primary correctness gate.

## Repository Map

- [`crates/`](crates) — the Rust workspace (chip cores, format parsers, machine
  models, and the `emu198x-*` system binaries)
- [`docs/status/current-system-usability.md`](docs/status/current-system-usability.md) — the authoritative current-state matrix
- [`docs/status/outstanding-work.md`](docs/status/outstanding-work.md) — cross-system remaining work
- [`docs/testing-policy.md`](docs/testing-policy.md) — verification standard
- [`knowledge/decisions/`](knowledge/decisions) — binding architectural decisions
- [`docs/archive/`](docs/archive) — superseded/historical material

Source comments occasionally reference `knowledge/chips/`, `knowledge/systems/`,
`knowledge/concepts/`, and similar paths. Those are LLM-curated working notes
kept locally; only `knowledge/decisions/` ships publicly. Treat the other paths
as project-internal context.

## Notes On Documentation

Some older documents in this repository describe earlier workspaces, abandoned
implementation branches, or overstated completion claims. The active path is:

1. the Rust workspace in `crates/`
2. `docs/status/` (current state) and the dated plans under `docs/plans/`
3. `knowledge/decisions/`
4. `docs/testing-policy.md`

If those disagree with an older status or roadmap document, the older document
is historical.

## About versioning

The binary releases (`emu198x-spectrum`, `emu198x-c64`, …) stay at 0.x by
design. There is no planned "Emu198x 1.0" milestone — production-readiness per
system is signalled by the catalogue passing for that system, not by the version
label. See [`knowledge/decisions/versioning-milestones.md`](knowledge/decisions/versioning-milestones.md).

When individual library crates from this workspace start publishing to crates.io
(chip cores like `mos-6502` and `zilog-z80`, format parsers, …), each one carves
out to independent versioning at publish time and may hit its own 1.0 milestone
when its public API is judged stable. See
[`knowledge/decisions/versioning-strategy.md`](knowledge/decisions/versioning-strategy.md)
for the carve-out mechanics.

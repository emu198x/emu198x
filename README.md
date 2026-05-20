# Emu198x

Emu198x is a fresh Rust workspace for building cycle-accurate vintage computer
and console emulators without shortcutting major hardware behavior.

The current implementation focus is:

- Sinclair ZX Spectrum 48K
- Commodore 64
- Nintendo Entertainment System
- Commodore Amiga (A500 OCS PAL baseline)
- Nintendo Game Boy
- Dragon 32

This repository also contains older research and planning material from previous
attempts. Treat the fresh Rust workspace as the implementation truth, and treat
archived status/roadmap material as historical context only.

## What This Project Is Trying To Do

The project aims to build emulators that model the real machines directly:

- pin-level CPU interfaces
- system-specific timing loops
- direct modeling of video, audio, DMA, media, arbitration, and peripherals
- deterministic headless runtimes with thin host shells above them

If a hardware path is not modeled accurately enough yet, the system is meant to
stay incomplete rather than faking the missing behavior.

## Current Fresh-Workspace State

As of April 28, 2026, the fresh Rust workspace currently provides:

- **Spectrum 48K**
  - real Z80 + ULA-driven machine loop
  - TAP/TZX loading
  - tape autoload helper and cycle-faithful tape turbo
  - live beeper and tape audio in the native verifier shell
  - headless runner, native verifier UI, screenshots, snapshots, and queryable
    boot detection
  - real-software regressions including Manic Miner and Jet Set Willy
  - native shell input is usable for verification, but still feels softer than
    target and should not yet be treated as a polished frontend

- **Commodore 64**
  - live 6502/CIA/VIC-II/SID board loop
  - KERNAL boots to `READY.`
  - headless runner, native verifier UI, screenshots, snapshots, boot
    detection, decoded screen-text queries, and mono audio output
  - TAP-backed datasette media insertion plus real tape transport control on
    the C64 board path
  - host-side `SHIFT+RUN/STOP` tape autoload helper over the real KERNAL path
  - host-side `.prg` import
  - host-side plain-text `.bas` import via Commodore BASIC tokenisation
  - host-side `.d64` import by extracting the first PRG directory entry
  - host-side `.t64` import by extracting the first loadable archive entry
  - optional live `1541` drive-8 execution with real `D64` media insertion and
    real `LOAD"*",8,1` progress on plain disk titles
  - native shell input is usable for verification, but still feels softer than
    target and should not yet be treated as a polished frontend

- **Nintendo NES**
  - live 2A03/2C02/APU machine loop
  - iNES cartridge loading with NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM,
    Color Dreams, VRC2a, Action 53, BxROM, NINA-001, Sunsoft-4, and Camerica
    mapper support
  - native verifier UI, headless runner, screenshots, live audio/audio capture,
    scripts, snapshots, keyboard/gamepad controller input, and local smoke
    matrix reporting
  - `nestest.nes`, Blargg-style `$6000` assertions, and local real-ROM smoke
    coverage are used to steer mapper and timing work

- **Commodore Amiga**
  - live A500 OCS PAL board loop over `motorola-68000`, Agnus, Denise, Paula,
    Gary, dual `8520` CIAs, keyboard, and DF0 floppy
  - native verifier UI with shared `wgpu` video, keyboard/mouse input,
    port-1 joystick/gamepad input, and live Paula audio
  - headless runner, screenshots, audio capture, shared scripted keyboard
    input, A1000 and A500-family profiles, queryable boot/disk state, and DF0
    `ADF` media insertion
  - postcard-encoded snapshot envelope across the full chip stack (CPU, Agnus,
    Copper, Denise, Paula, both CIAs, Gary, autoconfig, Memory, Floppy,
    Keyboard, RTC) with version + model validation; disk media is referenced
    by inserted ADF bytes rather than embedded MFM
  - Kickstart and Workbench paths now have local headless smoke coverage

- **Nintendo Game Boy**
  - live DMG-family CPU/PPU/APU machine loop
  - native verifier UI using the shared `wgpu` video presenter with
    `raw`/`lcd`/`crt` modes
  - headless cartridge runner with screenshots, live audio/audio capture,
    scripts, snapshots, keyboard/gamepad joypad input, and `.sav`
    battery-RAM sidecars
  - Blargg and mooneye-style verification gates are used for CPU/timing work

- **Dragon 32**
  - real Dragon 32 BASIC ROM boot over `motorola-6809`, dual MC6821 PIAs,
    MC6883 SAM, and MC6847 VDG
  - native verifier UI using the shared `wgpu` presenter, semantic keyboard
    input, gamepad-to-keyboard controls, CAS media, ROM/DGN cartridge
    mounting, DragonDOS VDK disk mounting, PC-Dragon PAK snapshot mounting, native
    `--autoload`, and beam-updated MC6847 framebuffer
  - headless CAS smoke harness that loads BASIC and machine-code tapes through
    the real ROM `CLOAD`/`CLOADM` paths and can compare screenshots against a
    patched XRoar reference

Notably not claimed yet:

- no claim that DragonDOS disk support is complete beyond the initial VDK
  sector-read controller path
- no claim that disk/tape/cartridge support exists unless the corresponding
  hardware path is actually modeled

## Principles

- Accuracy is the release criterion, not a future polish pass.
- Shared infrastructure stays narrow; machine timing stays family-specific.
- Host conveniences are allowed only above the emulation boundary.

Examples:

- `--autoload-tape` for Spectrum is a host workflow over the real ROM editor and
  real tape transport, not an instant-load trap.
- `--load demo.bas` for the current C64 runner is a host-side program import
  path, not fake disk or tape emulation.
- `--load demo.d64` for the current C64 runner is a host-side container import
  path that extracts the first PRG directory entry; it is not full 1541
  emulation.
- `--disk game.d64` for the current C64 runners mounts a `D64` into the live
  drive-8 path when a 1541 ROM is present; this is real drive-owned media
  insertion on the shared IEC bus.
- `--autoload-disk` for the current C64 runners is a host workflow over the
  real BASIC editor: it types `LOAD"*",8,1` and waits for the live 1541 path
  to perform the real DOS/IEC disk load.
- an optional 1541 ROM in the current C64 runtime means a live drive board now
  executes on the shared IEC bus and now loads plain disk titles such as
  `Bruce Lee`, `Aztec Challenge`, and `Bomb Jack`; write/save paths and broader
  compatibility are still incomplete.
- `--tape game.tap` for the current C64 runner is real datasette media on the
  board path.
- `--load demo.t64` for the current C64 runner is a host-side container import
  path that extracts the first loadable entry; it is not pulse-timed datasette
  playback.
- `--tape game.cas` for the current Dragon runner mounts real CAS cassette
  media; the ROM still performs the `CLOAD`/`CLOADM` load over the emulated
  cassette input path.
- `--cart cartridge.dgn` for the current Dragon runner mounts ROM/DGN
  cartridge media through the emulated cartridge ROM/GMC banking path.
- `--disk game.vdk` for the current Dragon script runner mounts a DragonDOS VDK
  image through the emulated P2 disk-controller register path; this is not yet a
  complete WD2797 timing or write implementation.
- `--snapshot game.pak` for the current Dragon runner restores a PC-Dragon
  PAK snapshot as machine state; it is not treated as cartridge media.

## Building

The workspace tracks the latest stable Rust toolchain.

Typical commands:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/coverage.sh
```

## Running

### Spectrum 48K native verifier shell

If `--rom` is omitted, the runner looks for:

- `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`

Example:

```bash
cargo run -p emu198x-spectrum -- \
  --tape '/Users/stevehill/Projects/198x/assets/sinclair/spectrum/Games/[TZX]/Manic Miner (1983)(Bug-Byte).zip' \
  --autoload-tape \
  --turbo-tape
```

### C64 native verifier shell

The C64 native verifier resolves ROMs from:

1. `--rom-dir DIR`
2. `EMU198X_C64_ROM_DIR`
3. `~/.emu198x/roms/commodore-c64`
4. `~/.emu198x/roms/c64`

Example:

```bash
cargo run -p emu198x-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --tape game.tap \
  --autoload-tape \
  --turbo-tape

cargo run -p emu198x-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --disk game.d64 \
  --autoload-disk
```

Live controls:

- `Esc` quit
- `F9` start tape
- `F10` stop tape
- `F11` toggle cycle-faithful tape turbo
- `F12` hard reset

### Spectrum 48K headless runner

```bash
cargo run -p emu198x-script-spectrum -- \
  --rom ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom \
  --tape '/Users/stevehill/Projects/198x/assets/sinclair/spectrum/Games/[TZX]/Manic Miner (1983)(Bug-Byte).zip' \
  --autoload-tape \
  --wait-for-tape-stop 12000
```

### C64 headless runner

The C64 runner resolves ROMs from:

1. `--rom-dir DIR`
2. `EMU198X_C64_ROM_DIR`
3. `~/.emu198x/roms/commodore-c64`
4. `~/.emu198x/roms/c64`

Expected ROM names:

- `kernal.rom` or `c64-kernal.rom`
- `basic.rom` or `c64-basic.rom`
- `chargen.rom` or `c64-chargen.rom`

Examples:

```bash
cargo run -p emu198x-script-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --wait-for-boot 200 \
  --screenshot ready.png
```

```bash
cargo run -p emu198x-script-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --load demo.bas \
  --save-snapshot demo.c64.pst
```

```bash
cargo run -p emu198x-script-c64 -- \
  --rom-dir ~/.emu198x/roms/commodore-c64 \
  --tape game.tap \
  --autoload-tape \
  --wait-for-tape-stop 12000
```

### NES headless runner

```bash
cargo run -p emu198x-script-nes -- \
  --rom '/Users/stevehill/Projects/198x/assets/nintendo/nes/test-suites/other/nestest.nes' \
  --frames 60 \
  --screenshot nestest.png
```

### Amiga headless runner

The Amiga runner resolves Kickstart ROMs from:

1. `--rom-dir DIR`
2. `EMU198X_AMIGA_ROM_DIR`
3. `~/.emu198x/roms/commodore-amiga`
4. `~/.emu198x/roms/amiga`

Expected Kickstart names:

- `kick13.rom`
- `kick12.rom`
- `kick31.rom`
- `kickstart.rom`
- `kick.rom`

Examples:

```bash
cargo run -p emu198x-script-amiga -- \
  --rom-dir ~/.emu198x/roms/commodore-amiga \
  --wait-for-boot 300 \
  --screenshot amiga-kick13.png
```

```bash
cargo run -p emu198x-script-amiga -- \
  --rom-dir ~/.emu198x/roms/commodore-amiga \
  --disk '/Users/stevehill/Projects/198x/assets/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip' \
  --wait-for-boot 300 \
  --screenshot amiga-workbench.png
```

### Dragon 32 native verifier shell

```bash
cargo run -p emu198x-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --tape game.cas \
  --autoload \
  --video crt
```

### Dragon 32 headless smoke runner

```bash
cargo run -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --smoke-root '/path/to/Dragon/Applications/[CAS]' \
  --smoke-run-limit 12 \
  --smoke-report dragon-smoke.json
```

```bash
cargo run -q -p emu198x-script-dragon -- \
  --rom ~/.emu198x/roms/dragon/dragon32.rom \
  --snapshot-smoke-root '/path/to/Dragon/Games/[PAK]' \
  --smoke-run-limit 32 \
  --smoke-report dragon-pak-smoke.json \
  --smoke-screenshot-dir dragon-pak-screens \
  --smoke-screenshot-format xroar-zoomed \
  --xroar-bin ../Emu198x-Unclean/xroar/src/xroar \
  --xroar-reference-dir dragon-pak-xroar-reference
```

## Verification Strategy

This repo does not treat “boots one thing” as sufficient proof.

The current verification approach is:

- chip- and format-level unit tests first
- machine wiring tests second
- ROM/software regressions above that
- external reference suites where appropriate

Current examples include:

- Z80 verified against Tom Harte, `zexdoc`, `zexall`, and a tracked FUSE
  compatibility harness
- Spectrum machine and software regressions over real ROM and tape paths
- C64 ROM-backed `READY.` boot detection plus snapshot round-trip checks
- NES machine regressions over real `nestest.nes`, plus a fresh headless
  cartridge path that now runs `nestest.nes` and `Super Mario Bros.` through
  `emu198x-script-nes` and emits screenshots
- Amiga machine/runtime tests over the imported A500 OCS PAL chip stack, plus
  fresh headless smokes that boot Kickstart 1.3 to the insert-disk screen and
  accept DF0 `ADF` insertion through `emu198x-script-amiga`
- C64 datasette board/runtime tests for TAP pulse parsing, 6510 port sense,
  CIA1 FLAG delivery, plus ROM-backed `Thinker` and `Thomas` TAP paths that
  reach observable loader-banner states over the real datasette flow, and a
  `Ghostbusters` TAP regression that now reaches a later graphics/loader state
  after the first-stage `FOUND MAIN` banner, plus a `Thing on a Spring` TAP
  regression that reaches a stable post-load menu with readable controls and
  then enters a stable started state after `SPACE`
- C64 disk groundwork at two levels: host-side `D64` parsing/import for quick
  software triage, plus a new drive-side `mos-via-6522` / `machine-commodore-1541`
  substrate that now boots a real 1541 reset vector, mirrors RAM correctly,
  decodes both VIA windows, and now shares first-pass IEC line state with the
  C64 board through `common-commodore-iec`; the runtime can now optionally
  attach that live 1541 with queryable drive CPU/VIA state, snapshot coverage,
  real `D64` media insertion into `drive-8`, plus ROM-backed disk proofs:
  `Bruce Lee` now reaches `LOADING`, advances to a title after `RUN`, then
  responds to joystick input beyond that title; `Aztec Challenge` now returns
  to BASIC after load and reaches a readable instruction screen after `RUN`
  and `F1`; `Bomb Jack` now completes a multi-stage loader, reaches a readable
  title screen, and responds to joystick port-1 fire on the same live 1541
  path
- Dragon ROM/runtime tests boot to BASIC, verify keyboard echo, mount real CAS
  tapes, load Textstar with `CLOAD`/`RUN`, start machine-code CAS titles with
  `CLOADM`/`EXEC`, and compare a 12-title application smoke set against patched
  XRoar screenshots with 11/12 exact matches

Coverage exists as an audit signal, not as the primary correctness gate.

## Repository Map

- [`crates/`](crates) — fresh Rust workspace
- [`docs/plans/2026-04-12-emulator-suite-coherent-development-plan.md`](docs/plans/2026-04-12-emulator-suite-coherent-development-plan.md) — current high-level plan
- [`docs/testing-policy.md`](docs/testing-policy.md) — verification standard
- [`wiki/index.md`](wiki/index.md) — technical index
- [`wiki/log.md`](wiki/log.md) — append-only milestone log
- [`docs/archive/`](docs/archive) — superseded/historical material

## Notes On Documentation

Some older documents in this repository describe earlier workspaces, abandoned
implementation branches, or overstated completion claims. The active path is:

1. the fresh Rust workspace in `crates/`
2. the dated coherent plan
3. `wiki/decisions/`
4. `docs/testing-policy.md`

If those disagree with an older status or roadmap document, the older document
is historical.

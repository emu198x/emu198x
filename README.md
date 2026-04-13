# Emu198x

Emu198x is a fresh Rust workspace for building cycle-accurate vintage computer
and console emulators without shortcutting major hardware behavior.

The current implementation focus is:

- Sinclair ZX Spectrum 48K
- Commodore 64

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

As of April 13, 2026, the fresh Rust workspace currently provides:

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
  - host-side `.t64` import by extracting the first loadable archive entry
  - native shell input is usable for verification, but still feels softer than
    target and should not yet be treated as a polished frontend

Notably not claimed yet:

- no fresh-workspace NES product path
- no fresh-workspace Amiga product path
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
- `--tape game.tap` for the current C64 runner is real datasette media on the
  board path.
- `--load demo.t64` for the current C64 runner is a host-side container import
  path that extracts the first loadable entry; it is not pulse-timed datasette
  playback.

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
  --tape '/Users/stevehill/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]/Manic Miner (1983)(Bug-Byte).zip' \
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
  --load demo.bas
```

### Spectrum 48K headless runner

```bash
cargo run -p emu198x-script-spectrum -- \
  --rom ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom \
  --tape '/Users/stevehill/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[TZX]/Manic Miner (1983)(Bug-Byte).zip' \
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
- C64 datasette board/runtime tests for TAP pulse parsing, 6510 port sense,
  CIA1 FLAG delivery, and a ROM-backed `Thinker` TAP path that reaches KERNAL
  `FOUND` and `LOADING`

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

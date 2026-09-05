# Emu198x

[![CI](https://github.com/emu198x/emu198x/actions/workflows/ci.yml/badge.svg)](https://github.com/emu198x/emu198x/actions/workflows/ci.yml)

Emu198x is the Rust emulator workspace for the 198x family. It models vintage computers and consoles with machine-specific timing, reusable chip cores, deterministic headless runners, and thin host shells for windowed use.

The current source of truth for system usability is [`docs/status/current-system-usability.md`](docs/status/current-system-usability.md). The cross-system work queue is [`docs/status/outstanding-work.md`](docs/status/outstanding-work.md).

## Capability surfaces

Use these surface names when describing support. Avoid vague claims like “supported” without naming the surface.

| Surface | Meaning |
|---|---|
| **Window** | Native `wgpu` interactive window with video presets, keyboard/gamepad input, and live audio. |
| **Capture** | Headless screenshots, audio capture, and frame-boundary recording. |
| **Script** | Programmatic control through `--script`: run frames, stop on PC/memory changes, inject input, inspect or patch memory, snapshots. |
| **MCP** | Stdio agent surface exposing script controls plus per-chip `query()` paths. |
| **Boot** | How far real firmware or software gets today. |

Two broad tiers use those surfaces:

- **Primary systems** — full desktop-app experience plus capture/script/MCP surfaces, real software paths, and CPU oracle coverage.
- **Extended systems** — extracted systems with capture/script/MCP surfaces first; native windows and boot depth vary by machine.

## Current system surface

Primary systems:

- Sinclair ZX Spectrum family
- Commodore 64
- Nintendo Entertainment System
- Commodore Amiga OCS/ECS/AGA
- Nintendo Game Boy
- Dragon 32

Extended systems include Amstrad CPC464, Atari 800XL/2600/5200/7800, Acorn BBC Micro/Electron/Atom, Sega Master System/SG-1000, MSX1, ColecoVision, Commodore VIC-20/PET, Sinclair ZX80/ZX81, Sord M5, Memotech MTX, Mattel Aquarius, Tatung Einstein, Jupiter Ace, Spectravideo SVI-328, and Oric-1/Atmos.

Do not treat this README as the detailed status matrix. For boot depth, required ROMs, missing chips, and per-system remaining work, use the status docs linked above.

## Principles

- Accuracy is the release criterion, not a future polish pass.
- Shared infrastructure stays narrow; machine timing stays family-specific.
- Host conveniences live above the emulation boundary.
- If a hardware path is incomplete, keep that limitation visible instead of faking the missing behaviour.

Examples:

- Spectrum `--autoload-tape` drives the real ROM editor and tape transport; it is not an instant-load trap.
- C64 `--disk game.d64 --autoload-disk` mounts media into the drive path and types the real load command.
- Dragon `--tape game.cas --autoload` runs through the ROM cassette path.
- Dragon `--snapshot game.pak` restores machine state; it is not cartridge media.

## Building

The workspace tracks the latest stable Rust toolchain.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/coverage.sh
```

The shared verification gate for current systems is:

```bash
scripts/verify-current-systems.sh
```

It runs in-repository checks and conditionally runs local ROM/media smoke tests when configured assets exist. Missing local assets are recorded as skips, not failures.

## Running

Use `--help` on each binary for the complete flag set. Add `--no-default-features` when you want a headless-only build for screenshots, scripts, CI, or MCP use.

### Spectrum keyboard modes

The Spectrum starts in **Host Keyboard** mode. Type letters, digits and ordinary
BASIC punctuation using your host keyboard layout: `"`, `;`, `$`, `=`, `+` and so
on are translated into Spectrum key combinations. Host Shift is used to choose
the character, not forwarded as an extra CAPS SHIFT press. Arrow keys and
Backspace edit; Home is EDIT, and Pause is BREAK.

Hold **Tab** for SYMBOL SHIFT: **Tab+G** enters THEN, **Tab+W** enters `<>`,
and **Tab+A** enters STOP. Hold **Shift+Tab**, then release both keys to enter
extended mode; R then enters INT. While Tab is held, keys use the Spectrum's
physical mapping. Release it to resume normal host-layout typing. Option/AltGr
remain available for characters produced by your keyboard layout.

The ROM still interprets the keyboard. At the K cursor, P enters PRINT, L enters
LET and R enters RUN; this mode does not let you spell out keywords or paste
whole listings. The mapping assumes normal letter mode for text. Spectrum caps
lock, graphics and extended modes still have their target-defined effects.
Unsupported characters are identified in the window title rather than replaced
with the physical key's letter. IME composition and multi-character input are
not implemented.

Choose **Machine → Keyboard → Original Keyboard** for direct target controls:
Shift is CAPS SHIFT and Alt/Option is SYMBOL SHIFT. Use this mode for games or other software that depend on raw target keys.
**Cmd/Ctrl+Shift+K** switches modes (including on Linux, which has no native menu).
The window title shows the active mode. Switching modes or losing window focus
releases held target keys; releasing a translated key uses the combination chosen
when it was pressed, even if a host modifier has changed in the meantime.

### Keeping a Spectrum BASIC program

After entering `SAVE "greeting"` in BASIC, press a key at the tape prompt and
wait for SAVE to finish. Choose **Tape → Export Recording…**, or press
**Cmd/Ctrl+Shift+E**, and select a new `.tap` filename. A confirmation names the
file written. Export is separate from Save State: it creates a tape image that
the Spectrum can load through its tape interface.

The export includes every decodable recording accumulated in the current tape
session. It does not clear the recording. Cancelling the picker or encountering
an error leaves it available to export again. Existing destination files are
rejected, including loaded tape images; choose a new filename for each version.
If no decodable recording is available, the UI explains how to make one first.

To check the file independently, close the session and start the emulator with
`--tape greeting.tap --autoload-tape`. After loading, use BASIC `LIST` and `RUN`
and make another edit. The desktop File menu also provides the tape-slot picker;
start playback with Tape → Play after issuing `LOAD ""` in BASIC. Linux uses the
export shortcut because the shared UI currently has no native menu there.

Representative commands:

```bash
# Sinclair ZX Spectrum
cargo run --release -p emu198x-spectrum -- --rom 48.rom --tape game.tzx --autoload-tape

# Commodore 64
cargo run --release -p emu198x-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk game.d64 --autoload-disk

# NES windowed
cargo run --release -p emu198x-nes -- game.nes

# NES headless screenshot
cargo run --release -p emu198x-nes --no-default-features -- --rom game.nes --frames 300 --screenshot nes.png

# Amiga A1200 + Kickstart 3.1 + Workbench 3.1
cargo run --release -p emu198x-amiga -- --model a1200 --kickstart kick31a1200.rom --disk workbench31.adf

# Game Boy windowed
cargo run --release -p emu198x-game-boy -- game.gb

# Dragon 32
cargo run --release -p emu198x-dragon -- --rom ~/.emu198x/roms/dragon/dragon32.rom --tape game.cas --autoload --video crt

# MSX1 headless capture
cargo run --release -p emu198x-msx -- --bios ~/.emu198x/roms/microsoft-msx/msx.rom --frames 200 --screenshot msx-boot.png

# Atari 800XL agent/MCP surface
cargo run --release -p emu198x-atari-800xl -- --mcp
```

## ROMs and software

Emu198x does not ship commercial firmware, ROMs, disks, tapes, or cartridges. Provide your own lawful dumps or licensed copies. Legal availability varies by platform:

- Sinclair ZX Spectrum ROMs are commonly distributed through World of Spectrum under Amstrad’s non-commercial permission.
- C64 and Amiga ROMs are commercially licensed through Cloanto packages.
- NES and Game Boy commercial cartridges should be self-dumped; NES has no system ROM.
- Some extended systems require firmware/BIOS files before their boot surface can progress.

The status docs record which local assets each smoke test expects.

## Verification strategy

Emu198x does not treat “boots one thing” as sufficient proof. Verification is layered:

- chip and format unit tests;
- machine wiring tests;
- real ROM/software regressions;
- CPU oracle suites and external reference suites where appropriate;
- screenshots, audio captures, scripts, and MCP probes for system-level checks.

Coverage is an audit signal, not the primary correctness gate. The testing standard is the [testing policy](https://github.com/emu198x/docs/blob/main/testing-policy.md) in the [`emu198x/docs`](https://github.com/emu198x/docs) repository.

## Repository map

- [`crates/`](crates) — Rust workspace: chip cores, format parsers, machine models, and `emu198x-*` binaries.
- [`docs/status/current-system-usability.md`](docs/status/current-system-usability.md) — authoritative current-state matrix.
- [`docs/status/outstanding-work.md`](docs/status/outstanding-work.md) — cross-system remaining work.
- [`knowledge/decisions/`](knowledge/decisions) — binding architectural decisions.
- [`emu198x/docs`](https://github.com/emu198x/docs) — project documentation: the [testing policy](https://github.com/emu198x/docs/blob/main/testing-policy.md), per-system status pages, plans, and the [archive](https://github.com/emu198x/docs/tree/main/archive) of superseded material.

Source comments may reference local working-note paths such as `knowledge/chips/`, `knowledge/systems/`, or `knowledge/concepts/`. Only `knowledge/decisions/` is public project canon.

## Documentation precedence

For current work, prefer these sources in order:

1. `RULES.md` for binding engineering rules.
2. `docs/status/` for current system state.
3. `knowledge/decisions/` for architectural decisions.
4. The [testing policy](https://github.com/emu198x/docs/blob/main/testing-policy.md) in `emu198x/docs` for verification expectations.
5. [`plans/`](https://github.com/emu198x/docs/tree/main/plans) and [`archive/`](https://github.com/emu198x/docs/tree/main/archive) in `emu198x/docs` for planning and historical context.

If a status claim in an older plan or archive conflicts with `docs/status/`, treat `docs/status/` as current.

## Versioning

The system binaries stay at `0.x` by design. Production readiness is signalled per system by passing the relevant catalogue/status gate, not by an umbrella `1.0` label. See [`knowledge/decisions/versioning-milestones.md`](knowledge/decisions/versioning-milestones.md) and [`knowledge/decisions/versioning-strategy.md`](knowledge/decisions/versioning-strategy.md).

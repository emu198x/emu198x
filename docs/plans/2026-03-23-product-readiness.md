---
title: "Product Readiness: From Engineering Project to Usable Emulator"
type: feat
date: 2026-03-23
---

# Product Readiness

## The Gap

We have 26 systems, 140 crates, 1,700+ tests, and every feature
imaginable. But the compat harness tells us that a third of games
don't boot, nobody has played a game to completion, and there's no
way for a non-developer to use it. This plan closes that gap.

## Principle

Fix what exists before building new things. Every item here makes
the emulator more usable, not more featureful.

## Phase A: Know What's Broken (1-2 days)

Before fixing anything, understand the actual state.

### A1: Play-test the four core systems

For each of Spectrum, NES, C64, and Amiga:
- Load 10 popular, well-known games
- Play each for at least 5 minutes
- Document: does it boot? Is display correct? Is audio right?
  Do controls work? Any crashes?
- Compare screenshots against VICE/Fuse/Mesen/WinUAE

Target games:
- **Spectrum**: Manic Miner, Jet Set Willy, Sabre Wulf, Atic Atac,
  Knight Lore, Head Over Heels, R-Type, Chase HQ, Dizzy, Elite
- **NES**: Super Mario Bros, Zelda, Metroid, Mega Man 2, Contra,
  Castlevania, Tetris, Kirby, Donkey Kong, Final Fantasy
- **C64**: Impossible Mission, Last Ninja, International Karate+,
  Maniac Mansion, Turrican, Creatures, Paradroid, Wizball
- **Amiga**: Shadow of the Beast, Turrican II, Speedball 2,
  Sensible Soccer, Monkey Island, Lemmings

Record issues in a tracking file.

### A2: Audio quality audit

For each system, record 30 seconds of audio output and compare
against a reference emulator's output. Note: wrong pitch, missing
channels, distortion, clicking/popping.

### A3: Performance profile

On a mid-range machine (or this Mac):
- Run each system, check CPU usage
- Does every system hit its target frame rate?
- Identify bottlenecks if not

## Phase B: Fix Emulation (1-2 weeks)

Address the issues found in Phase A. Likely areas:

### B1: Spectrum accuracy

Known issues from compat harness:
- 128K games hang (harness only creates 48K)
- Contention timing edge cases
- +2A/+3 FDC not fully implemented

### B2: NES accuracy

Known issues:
- 2 panics in chr_read (index out of bounds)
- Mapper 28 not supported
- Some test ROMs fail (timing-sensitive)
- Sprite 0 hit timing edge cases

### B3: C64 accuracy

High boot rate (100% on D64 games) but:
- VIC-II raster effects (VSP, FLD, AGSP)
- SID filter accuracy (already have reSID curves)
- CIA TOD clock accuracy

### B4: Amiga accuracy

Partial save states, known display issues:
- A4000 KS 3.0 dark display
- Copper timing edge cases
- Blitter line mode edge cases

## Phase C: User Experience (3-5 days)

### C1: README with screenshots

GitHub landing page that answers:
- What is this?
- What does it look like? (screenshots)
- How do I build it?
- Where do I put ROMs?
- What's the keyboard layout?

### C2: First-run experience

When the app launches for the first time:
- Show a welcome screen explaining ROM setup
- Provide a link to the docs
- Spectrum should work immediately (embedded ROM)
- Other systems should explain what's needed

### C3: User-facing save states

- Quick save: F2 (saves to the most recent slot)
- Quick load: F4 (loads the most recent slot)
- Save state menu with slot selection (already exists but buried)
- On-screen notification: "State saved" / "State loaded"

### C4: Keyboard shortcut reference

In-app help overlay (F1 or ?) showing:
- System controls
- Debugger shortcuts (F5/F9/F10/F11)
- Save state shortcuts
- Speed controls

### C5: ROM file association

On macOS/Linux, register file types so double-clicking a .z80 or
.nes file opens Emu198x directly.

## Phase D: CI and Releases (2-3 days)

### D1: GitHub Actions CI

- `cargo check --workspace` on every push
- `cargo test --workspace --lib` on every push
- `cargo clippy --workspace` on every push
- WASM build verification
- Runs on: macOS, Linux, Windows

### D2: Release automation

- Tagged releases (v0.1.0, v0.2.0, etc.)
- GitHub Actions builds release binaries:
  - macOS: universal binary (aarch64 + x86_64)
  - Linux: x86_64 AppImage or tarball
  - Windows: x86_64 .exe
- WASM package published alongside
- Changelog generated from commit messages

### D3: Version numbering

Semver. Current state = v0.1.0 (alpha). Criteria for milestones:
- v0.1.0: app launches, Spectrum works, debugger works
- v0.2.0: four core systems playable
- v0.5.0: stable enough for daily use
- v1.0.0: feature-complete, all systems accurate, documented

## Phase E: Community (ongoing)

### E1: Contributing guide

- How to build
- How to add a system (point to docs/adding-a-system.md)
- How to run tests
- Code style (rustfmt, clippy config)
- PR expectations

### E2: Issue templates

- Bug report (system, ROM, steps to reproduce, expected vs actual)
- Feature request
- New system request

### E3: Licence

MIT is declared but no LICENSE file exists in the repo. Add it.

## Implementation Order

| Order | Phase | What | Why first |
|-------|-------|------|-----------|
| 1 | A1 | Play-test 4 systems | Know what's broken |
| 2 | B | Fix the worst issues | Games need to work |
| 3 | C1 | README + screenshots | First impression |
| 4 | D1 | CI | Prevent regressions |
| 5 | C2-C4 | UX improvements | Make it usable |
| 6 | D2-D3 | Releases | Let people download it |
| 7 | E | Community | Let people contribute |

## Non-Goals for v0.1.0

- Perfect accuracy (that's a years-long journey)
- Every system playable (focus on the core four)
- Mobile support
- Netplay
- TAS tools

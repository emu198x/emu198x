# Decision: Product Roadmap (April 2026)

## Vision

Rebuild all 35+ systems from the old codebase at the new accuracy standard. Every CPU core cycle-perfect. Every system correct from day one. Ship as per-system standalone binaries plus a unified launcher.

## October 2026 (CRASH! Live launch)

Four Code198x platforms, in order:

1. **Spectrum** — done (6 variants, 100% Tom Harte, Signal Part 3)
2. **C64** — next (6502 tick core exists, VIC-II timing well-documented)
3. **NES** — third (shares 6502 core, PPU is the challenge)
4. **Amiga** — last (68000 + custom chipset, most complex, most dev time)

If behind schedule, Amiga is the cut candidate.

## Must-haves for October

- **Capture pipeline**: headless mode, PNG screenshots, video capture, input scripting, MCP
- **CRT filter**: shared across all systems
- **Serialisation traits**: built into every system from the start

## Post-October waves

**Wave 2** — historically significant systems (by impact, not CPU convenience):
- Atari 2600 (racing the beam, started it all)
- BBC Micro (British heritage, natural Code198x expansion)
- MSX (international, reuses Z80 + AY)
- Master System (Z80, gateway to Mega Drive)

**Wave 3+** — remaining systems grouped by shared chip reuse:
- Z80 family: CPC, ZX80, ZX81, ColecoVision, SG-1000
- 6502 family: Atari 800XL/5200/7800, Electron, Atom, VIC-20, PET, Oric
- 68000 family: Atari ST, Mega Drive
- 6809 family: Dragon, CoCo, Vectrex
- Long tail: Jupiter Ace, Aquarius, MTX, Sord M5, SVI-328, Einstein

## Accuracy bar

Same as Spectrum for every system. Non-negotiable. See [fresh start rationale](fresh-start-rationale.md).

## Product shape

Per-system standalone binaries (`emu198x-spectrum`, `emu198x-c64`, etc.) **plus** a unified launcher (`emu198x`) that presents a system catalogue. Both ship. Shell infrastructure is a shared crate that every system links against.

## Chip reuse map

| Chip | Systems |
|------|---------|
| Z80 | Spectrum, MSX, CPC, SMS, SG-1000, ColecoVision, ZX80/81, Mega Drive |
| 6502 | C64, BBC, Electron, Atom, Atari 800XL/5200, VIC-20, PET, Oric |
| 2A03 (6502 variant) | NES |
| 68000 | Amiga, Atari ST, Mega Drive |
| 6809 | Dragon, CoCo, Vectrex |
| AY-3-8912 | Spectrum 128K+, MSX, CPC, Oric, ST |
| TMS9918 | MSX, ColecoVision, SG-1000 |
| SN76489 | SMS, Mega Drive, ColecoVision, BBC |

Each system added after its CPU and shared chips exist is significantly cheaper.

## Open questions

- SID emulation approach (port / rewrite / reSID wrapper)
- 68000 tick-level conversion strategy (largest single risk)
- NES mapper coverage for curriculum

## Drift triggers

Roadmaps drift through scope creep and reprioritization, not code patterns. If I'm about to suggest any of these, stop and raise the scope change explicitly rather than silently acting on it.

**Scope drift to reject:**

- Adding any system to the October list beyond Spectrum / C64 / NES / Amiga
- Reordering the October priorities (Spectrum → C64 → NES → Amiga is fixed)
- Cutting October must-haves (capture pipeline, CRT filter, serialisation traits)
- Starting Wave 2 work before October has shipped
- Adding "nice-to-haves" to the October scope

**Accuracy drift to reject:**

- Lowering the accuracy bar for any new system ("we can start with 90% accurate and improve later")
- Per-system accuracy exceptions ("the NES PPU is hard, let's ship approximate timing")
- "Add accuracy later" framing anywhere — see also [fresh-start-rationale.md](fresh-start-rationale.md)
- Retrofitting accuracy after shipping

**Product-shape drift to reject:**

- Collapsing per-system binaries into one monolithic app
- Dropping the unified launcher "to save time"
- Skipping the shared shell crate (`emu198x-shell`) and reimplementing per system
- Adding a web version / mobile version / etc. before October

**Phrases that signal drift:**

- "We can cut X from the October scope"
- "Let's add [other system] before the launch"
- "The accuracy bar for [system] can be lower"
- "Maybe Amiga doesn't need to ship in October" — Amiga *is* the documented cut candidate, so if I'm proposing this without the user raising it first, I'm not reading the roadmap
- "Let's just do one system well instead of four"

**What to do when triggered:** the October 2026 CRASH! Live launch is a hard deadline. Any roadmap change is a user decision, not mine. Raise scope concerns explicitly and early; do not silently narrow or expand scope.

## Related

- [Fresh start rationale](fresh-start-rationale.md) — why accuracy is non-negotiable
- [Crate naming](crate-naming.md) — how new crates should be named
- [Brainstorm doc](../../docs/brainstorms/2026-04-05-accuracy-to-product-roadmap-brainstorm.md) — full discussion

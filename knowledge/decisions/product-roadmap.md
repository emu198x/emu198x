# Decision: Product Roadmap (April 2026)

## Vision

Rebuild all 35+ systems from the old codebase at the new accuracy standard. Every CPU core cycle-perfect. Every system correct from day one. Ship as per-system standalone binaries. **Amended 2026-05-23.** The original framing committed to "plus a unified launcher" alongside the per-system binaries; that commitment is retired per [`no-unified-launcher.md`](no-unified-launcher.md). The "Emu198x" brand is the GitHub org and the README, not a single mega-app.

## October 2026

**Amended 2026-05-06.** Originally this section listed four October platforms in priority order with Amiga as the cut candidate. The framing changed when Code198x narrowed to Spectrum-only for its October launch. See [October catalogue Log](october-catalogue.md#log) for the cross-project rationale.

**Public launch (Crash! Live):** Spectrum only. The system Code198x ships and Crash! Live's audience cares about. **Spectrum SOLID** — variants stable across all 11 supported variants, all 10 [October catalogue](october-catalogue.md) entries pass, real-hardware validated against Spectrum Next + Fuse, Code198x screenshot/video pipeline reliable — is the October-public goal.

> **Status (2026-06-03): Spectrum SOLID engineering bar MET**, ahead of the October launch. The "gate that opens attention to non-Spectrum catalogue completion" (below) is open — C64/NES/Amiga and the donor systems are now the active engineering frontier, not deferred. See [October catalogue Log](october-catalogue.md#log) and RULES.md § Session start.

**Engineering bar (priority order, no October deadline):**

1. **Spectrum** — 13 variants in the codebase (12 listed in the wiki overview plus Spectrum+ added 2026-05-06). **8 in October SOLID scope:** 16K, 48K, Spectrum+, 128K, +2, +2A, +2B, +3. **Deferred to post-October:** Pentagon, Scorpion, TC2048, TC2068, TS2068. 100% Tom Harte, Signal Part 3 working. Full SOLID criteria locked in [October catalogue](october-catalogue.md#october-bar-definition).
2. **C64** — 6502 + CIA + VIC-II + SID; KERNAL boots end-to-end, 1541 path operational. Catalogue passes when it passes.
3. **NES** — 2A03 + 2C02 + APU; nestest 8991/8991, Super Mario Bros. renders. Catalogue passes when it passes.
4. **Amiga** — 68000 + OCS chipset; Workbench 1.3 desktop, A1000 / A500-family runtimes. Catalogue passes when it passes.

C64, NES, and Amiga continue as engineering bars. They progress in this priority order, but none have an October deadline. Spectrum SOLID is the gate that opens attention to non-Spectrum catalogue completion.

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

## Best-in-class ladder (added 2026-07-03)

A new axis **on top of** the engineering bar, decided at umbrella level — see
[`../../../../decisions/emu198x-best-in-class.md`](../../../../decisions/emu198x-best-in-class.md)
and the programme plan at
[`2026-07-03-best-in-class-programme.md`](../../docs/plans/2026-07-03-best-in-class-programme.md).

The victory condition is **best consistent multi-system suite + reference-class
depth on the four headliners, run as staged campaigns**, backed by three moats
(agent-native tooling, embeddable published crates, browser reach).

**Campaign order (reference-class depth, not catalogue completion):**

1. **Spectrum** — launch anchor, deepest already, most beatable incumbent (Fuse)
2. **NES / Game Boy** — public, finite canon; "provably equal to Mesen2/SameBoy" is a checklist
3. **C64** — demo canon frame-accuracy is the loudest single claim available
4. **Amiga** — standing multi-year campaign with staged claims (OCS demo canon → IPF originals → ECS/AGA parity), never one distant "matches WinUAE" goal

This ladder does **not** reorder the engineering-bar catalogue priorities below
(C64 → NES → Amiga): catalogue-passing is a prerequisite rung of each campaign,
not a competitor to it. "Reference-class" is a public claim — same test canon
green as the incumbent, same protected originals booting, evidenced by a
published per-system dashboard.

Supporting commitments: a **hardware-truth pipeline** (real machines + capture
gear for campaign systems, measurements landing in the umbrella `reference/`
library as a hardware-measured provenance layer), and **WASM as a strategic
priority, scoped** — curriculum-owned code on firmware-permission systems,
superseding-in-part [`wasm-sequencing.md`](wasm-sequencing.md) (see its Log),
sequenced so it cannot displace Spectrum launch-hardening before Crash! Live
(see amended drift trigger below).

**Two lanes:** campaigns are the priority lane; the engineering frontier
(donor systems, Tier B/C breadth) continues opportunistically per RULES.md
session-start anchors. Neither pauses the other; the campaign dashboard is
the stall detector. Licensing intent for the published-crates moat:
[`crate-licensing-split.md`](crate-licensing-split.md). Docs-site canon:
[`docs-site-canon.md`](docs-site-canon.md).

## Accuracy bar

Same as Spectrum for every system. Non-negotiable. See [fresh start rationale](fresh-start-rationale.md).

## Product shape

**Amended 2026-05-23** — see [`no-unified-launcher.md`](no-unified-launcher.md).

Per-system standalone binaries (`emu198x-spectrum`, `emu198x-c64`, etc.) are the product. There is no unified launcher; the `emu198x` binary name stays reserved. Shell infrastructure (`emu198x-shell`) is a shared crate that every per-system binary links against, providing common headless-session shape, MCP server boilerplate, audio / video sinks, and the query provider trait — at the **library** layer, not the application layer.

Cross-system features (rewind, cheats, save-state browser, controller config, screen filters) live inside each per-system binary, not in a host process. Distribution is per-system formulae / installers; an optional `emu198x-suite` meta-formula that pulls all six is a convenience, not the primary path.

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

- Treating any system other than Spectrum as October-public ("we need C64 done by October too")
- Reordering the engineering-bar priorities (Spectrum SOLID first; then C64 → NES → Amiga; Wave 2 systems are post all of these)
- Cutting Spectrum SOLID's must-haves (capture pipeline, CRT filter, serialisation traits)
- Starting Wave 2 work before Spectrum SOLID
- Adding "nice-to-haves" to the Spectrum SOLID scope
- Inferring an October deadline for C64/NES/Amiga from the pre-2026-05-06 framing — the public October system is Spectrum only

**Accuracy drift to reject:**

- Lowering the accuracy bar for any new system ("we can start with 90% accurate and improve later")
- Per-system accuracy exceptions ("the NES PPU is hard, let's ship approximate timing")
- "Add accuracy later" framing anywhere — see also [fresh-start-rationale.md](fresh-start-rationale.md)
- Retrofitting accuracy after shipping

**Product-shape drift to reject:**

- Collapsing per-system binaries into one monolithic app
- Adding a unified launcher (superseded 2026-05-23 — see [`no-unified-launcher.md`](no-unified-launcher.md); rejecting the launcher is now the binding decision)
- Adding a `--launcher` mode to per-system binaries (same coupling cost, distributed across every binary)
- Skipping the shared shell crate (`emu198x-shell`) and reimplementing per system
- Adding a web version / mobile version / etc. before October — **amended
  2026-07-03**: WASM is now strategic scope (see § Best-in-class ladder), but
  the trigger's protective half stands — WASM work must not displace Spectrum
  launch-hardening before Crash! Live. Reject *displacement*, not the target.

**Phrases that signal drift:**

- "We can cut X from Spectrum SOLID"
- "Let's add [other system] before Spectrum SOLID"
- "The accuracy bar for [system] can be lower"
- "Maybe we should ship C64 too at Crash! Live" — Crash! Live is Spectrum-only public scope; if I'm proposing C64/NES/Amiga as October-public without the user raising it first, I'm working from the pre-2026-05-06 roadmap

**What to do when triggered:** the October 2026 Crash! Live launch is a hard deadline for **Spectrum SOLID**. Engineering-bar work on C64/NES/Amiga has no October deadline. Any roadmap change is a user decision, not mine. Raise scope concerns explicitly and early; do not silently narrow or expand scope.

## Related

- [Fresh start rationale](fresh-start-rationale.md) — why accuracy is non-negotiable
- [Crate naming](crate-naming.md) — how new crates should be named
- [No unified launcher](no-unified-launcher.md) — the 2026-05-23 supersession of the launcher commitment
- [Best-in-class decision (umbrella)](../../../../decisions/emu198x-best-in-class.md) — victory condition, campaign staging, moats (2026-07-03)
- [Brainstorm doc](../../docs/brainstorms/2026-04-05-accuracy-to-product-roadmap-brainstorm.md) — full discussion

## Log

### 2026-07-03 — Best-in-class ladder added

Following an eight-dimension codebase audit and a strategy session on standing
shoulder-to-shoulder with WinUAE/VICE/Mesen2/Fuse, Steve decided the victory
condition: best unified suite + staged reference-class campaigns
(Spectrum → NES/GB → C64 → Amiga standing), hardware-truth investment for
campaign systems, and WASM as sequenced strategic scope. Binding record at the
umbrella: [`emu198x-best-in-class.md`](../../../../decisions/emu198x-best-in-class.md).
Programme detail:
[`2026-07-03-best-in-class-programme.md`](../../docs/plans/2026-07-03-best-in-class-programme.md).
The pre-October web-version drift trigger amended (reject displacement, not the
target); engineering-bar order and October scope unchanged.

### 2026-05-23 — Launcher commitment retired

The original April-2026 framing committed to shipping "per-system standalone binaries plus a unified launcher." That commitment is retired per [`no-unified-launcher.md`](no-unified-launcher.md). Per-system binaries are the product; the `emu198x` binary name stays reserved (deferred decision on whether a thin stub eventually fills it). Cross-system features move to per-system implementations. Drift trigger flipped: "adding a unified launcher" is now the pattern to reject, replacing the old "dropping the unified launcher" trigger.

### 2026-05-06 — Public October scope narrowed to Spectrum

Originally the October 2026 section listed four anchor platforms in priority order with Amiga as the cut candidate. The framing changed when Code198x narrowed to Spectrum-only for its October launch. Public October launch is now Spectrum SOLID only; C64 / NES / Amiga continue as engineering bar with no October deadline. See [October catalogue Log](october-catalogue.md#log) for the cross-project rationale.

# Decision: No unified launcher — per-system binaries are the product

**Date:** 2026-05-23
**Status:** Locked. Supersedes the "plus a unified launcher" framing
in [`product-roadmap.md`](product-roadmap.md) § Product shape and
amends [`../../../docs/plans/2026-05-23-post-october-roadmap.md`](../../../docs/plans/2026-05-23-post-october-roadmap.md)
§ Phase D.

## What this is

The product-shape decision. Each Emu198x system ships as its own
binary; there is no unified `emu198x` desktop app on top of them.
The "Emu198x" brand is the GitHub org, the README, and the family
of related binaries — not a single mega-app.

This contradicts the previous product-roadmap commitment to ship
"per-system standalone binaries plus a unified launcher." That
commitment was made in April 2026 before the project's audience
shape and per-system binary distribution model had hardened. It is
hereby retired.

## The decisions

1. **No unified launcher binary.** The `emu198x` binary name stays
   unused (or reserved for a future thin stub if one is ever
   warranted — see "Open question on the stub" below).
2. **Per-system binaries are the primary product surface.**
   `emu198x-spectrum`, `emu198x-c64`, `emu198x-amiga`,
   `emu198x-dragon`, `emu198x-game-boy`, `emu198x-nes` — and any
   future per-system binaries — are the things users install, run,
   and learn the names of.
3. **Cross-system features stay per-system.** Rewind, cheats,
   save-state browser, controller config, screen filters, library
   metadata — every one of these lives inside the relevant
   per-system binary, not in a host process.
4. **`emu198x-shell` shared crate stays.** It provides cross-system
   infrastructure at the **library layer** — common headless
   session shape, MCP server boilerplate, audio / video sinks,
   query provider trait. It does *not* power an application-layer
   launcher because there isn't one.
5. **Distribution: per-system formulae / installers.**
   `brew install emu198x-spectrum`, `brew install emu198x-c64`,
   and so on. A meta-formula (`emu198x-suite` or `emu198x-all`)
   that pulls all six is optional convenience, not the primary
   path.
6. **Marketing surface is the README + GitHub org.** Not a binary
   name. "Emu198x is a family of cycle-accurate emulators for
   1970s–early-2000s home computers and consoles" — that's the
   brand framing.

## Why

### The audience this project serves doesn't use launchers

Per-system tools dominate the practical retro-emulation landscape.
The audience's mental model is **Mesen2 / openMSX / Fuse / VICE /
WinUAE** — one app per system family, each polished within its
domain. Third-party launchers (LaunchBox, OpenEmu, RocketLauncher,
Playnite) wrap these per-system tools when users want library
management, and that's the right separation of concerns: emulators
emulate, launchers manage libraries.

Code198x is the practical "launcher" for the curriculum audience:
learners land on a lesson page, click a link, run a specific
emulator with a specific ROM. They don't browse a desktop game
picker. A unified launcher would serve users this project doesn't
have.

### Cost/benefit is asymmetric

Building a launcher well takes months: game scanning, metadata
ingestion, cover-art handling, controller config UI, save-state
browser, settings, library organisation, possibly online metadata
lookup. That work competes with the accuracy investments, the
Wave 2 systems, the debugger, the catalogue infrastructure — the
things this project's audience actually cares about.

Not building it costs almost nothing because the audience already
has alternatives: OS file associations for "I clicked a .tap file,"
the README + Homebrew tap for "I want to install Emu198x,"
third-party launchers for "I want library management." Every
problem the launcher would solve has an alternative answer that's
already in place or cheap to put in place.

### Cross-system features don't need a host

The argument that "rewind / cheats / save-state browser need a
launcher" doesn't hold up under examination:

- **Rewind** is per-system because the rewind buffer shape is
  per-system (Spectrum's tick is half-cycle; NES's is per-dot;
  Amiga's is per-CCK). The buffer doesn't share between systems
  meaningfully.
- **Cheats** are per-system because Game Genie / Action Replay
  encodings are per-system. The shared infrastructure is just
  "an in-memory patch table at known address" — already a
  per-binary feature.
- **Save-state browser** is per-system because save state contents
  are per-system-incompatible. Even *listing* saves is per-system:
  `~/.emu198x/saves/spectrum/`, `~/.emu198x/saves/c64/`, etc.
- **Controller config** is per-system because the input mapping
  shape is per-system (Spectrum has Kempston + Sinclair + Cursor;
  NES has standard pad; Amiga has joystick + mouse + analogue
  port).
- **Screen filters** are mostly per-system (CRT for raster
  systems, LCD for Game Boy, etc.) and the few that aren't ship
  via `emu198x-native-video` already.

The per-system binary is the natural home for every one of these.
A launcher would either duplicate them (with per-system-aware UI)
or restrict the per-system binaries from having them (worse UX).

### The Unix philosophy fits the codebase shape

The workspace is already organised around per-system binaries:
each binary is independently built, tested, released via
cargo-dist, documented. The release pipeline ships 6 binaries
already. CI runs per-system suites. Decision records are
per-system. The codebase already *is* the per-system-tools model;
the launcher would have been a layer on top that fights the grain.

## Alternatives considered

- **(a) Build the launcher as originally promised in product-roadmap.**
  Months of work for a feature the audience doesn't want. Rejected.
- **(b) `--launcher` mode on every per-system binary.** Distributes
  the launcher coupling across every binary instead of isolating
  it. Worse architecturally; same audience-fit problem. Rejected.
- **(c) Thin stub `emu198x` binary that prints available systems
  and execs the right one.** Cheap (~50 LOC) and friendly. Not
  rejected per se — deferred to "if someone asks for it." See open
  question below.
- **(d) Defer the decision indefinitely.** Leaves a contradictory
  commitment in product-roadmap.md that future planning has to
  work around. Rejected.

## What we are NOT doing

- **Building `emu198x` as a desktop app.** Not a window, not a
  picker, not a settings UI.
- **Building `emu198x` as a TUI.** Same logic — would still need
  the library management, settings, scanning machinery.
- **Folding system selection into any per-system binary.** Each
  binary knows about its own system. No system-routing logic
  anywhere in the codebase.
- **Building a system-detection tool that opens the right
  emulator.** OS file associations do this. If a `emu198x-route
  mystery.bin` tool ever becomes useful, it's a tiny CLI that
  inspects the file header and prints which binary to run — not a
  launcher.

## What changes downstream

1. **[`product-roadmap.md`](product-roadmap.md) § Product shape
   amended.** Inline note + Log entry.
2. **[`product-roadmap.md`](product-roadmap.md) § Drift triggers
   amended.** The trigger "Dropping the unified launcher 'to save
   time'" is removed (it's been *deliberately* dropped, not drifted
   into dropping). Replaced with "Adding a unified launcher" as the
   new trigger to reject.
3. **[`../../../docs/plans/2026-05-23-post-october-roadmap.md`](../../../docs/plans/2026-05-23-post-october-roadmap.md)
   § Phase D** loses the "game library / launcher" work item.
   Other Phase D work (Game Boy CGB / SGB / link cable,
   cross-system polish, cheats) stays.
4. **Post-October roadmap § Open questions** removes Q4 (launcher
   shape).
5. **Post-October roadmap § Cross-system gaps** updates the "Game
   library / picker UI" row from "Worth doing" to "Out of scope —
   per [`no-unified-launcher.md`](../../knowledge/decisions/no-unified-launcher.md)."
6. **Cross-system roadmap items get reframed as per-system.** Rewind,
   cheats, save-state browser, controller config, screen filters —
   each becomes per-system feature work, scheduled per-system.
7. **Distribution story sharpens.** README explicitly opens with
   "Emu198x is a family of cycle-accurate emulators…" framing.
   Homebrew tap design is per-system formulae + optional meta-formula.

## Open question deferred

**The stub `emu198x` binary.** A 50-line program that does nothing
except print "Available systems: emu198x-spectrum, emu198x-c64, …"
when invoked, and (optionally) execs the right one when given an
argument like `emu198x spectrum --tape foo.tap`. This is friendly
discoverability without becoming a launcher. **Deferred** until
someone actually asks for it; trivial to add later. Reserved
binary name; nothing else.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"We need a way for users to browse all their games in one
  place"** — third-party tools (LaunchBox, OpenEmu, Playnite,
  Steam ROM Manager) do this well and integrate with per-system
  emulators. Building it ourselves duplicates existing solutions
  and doesn't serve our actual audience.
- **"Let's add a `--launcher` mode to the per-system binaries"** —
  same coupling cost as a separate launcher binary, distributed
  across every per-system binary. Worse architecturally. The
  decision applies equally to in-binary launchers.
- **"Cross-system rewind / save-state browser / cheats needs a
  host process"** — see "Cross-system features don't need a host"
  above. The per-system binary is the right home.
- **"The product feels fragmented across six binaries"** — the
  brand is the README and the GitHub org, not a binary. If
  fragmentation becomes a real complaint, the fix is better
  README + better Homebrew tap, not a launcher.
- **"Code198x learners need a unified launcher"** — they don't;
  they need curriculum pages that link to specific
  emulator+ROM combinations. Code198x is the practical launcher
  for the curriculum audience.
- **"We promised a unified launcher in product-roadmap.md"** —
  superseded by this record. Old commitments get amended; that's
  what decision records are for.

## Log

### 2026-05-23 — Decision locked

Brainstorm in-session with Steve: he challenged the product-roadmap
commitment to a unified launcher with "I don't currently believe
it's important." The challenge held up under examination. Both the
"launcher solves problems" steelman and the "launcher is a wasted
investment" case favor the latter for this project's audience.

Key arguments that decided it:
- The audience's mental model is per-system tools (Mesen2, openMSX,
  Fuse, VICE).
- Code198x is the practical launcher for the curriculum audience.
- Cross-system features can ship per-system without losing
  meaningful integration.
- Cost/benefit is asymmetric: launcher is months of work, not
  having one costs almost nothing for the actual audience.
- The codebase shape already is per-system; the launcher would
  fight the grain.

Steve confirmed: "this decision makes sense, and the challenge was
useful." Locked and propagated to product-roadmap.md (amendment +
drift trigger flip) and post-October-roadmap.md (Phase D work item
removed, open question removed).

The thin stub `emu198x` binary remains a deferred possibility — not
ruled out, not actively planned. Tracked as the open question
above.

# Decision: WASM deferred — no system ships a WASM build until concrete demand

**Date:** 2026-05-23
**Status:** SUPERSEDED IN PART, 2026-07-03 — see the Log. WASM is
now strategic scope, **narrowly**: Code198x curriculum embeds
running curriculum-owned code, on firmware-permission systems
(Spectrum first), sequenced so it cannot displace launch-hardening
before October. The deferral **stands** for everything else this
record rejected — general demo pages, BYO-ROM play in the browser,
and marketing the project "web-ready". Binding scope at
[`../../../decisions/emu198x-best-in-class.md`](../../../decisions/emu198x-best-in-class.md).
Originally: Locked. Resolves the WASM sequencing question that
appeared in the post-October roadmap's open-questions list. The
question was "which system ships first to WASM?"; the answer was
"none, until concrete demand surfaces."

## What this is

The WASM build decision. The April 12 coherent plan demoted WASM
behind core correctness. The post-October roadmap (Phase G) had
WASM tentatively scoped as "one system, probably Spectrum, for
Code198x curriculum embeds." Context has shifted enough since that
draft that the WASM case no longer holds; this record settles it
formally.

## The decision

**No system ships a WASM build until concrete demand surfaces.**
Defer indefinitely. Re-evaluate only when one of the triggers
below fires.

When/if it does happen, the candidate first-system is Spectrum
(smallest, October-anchor, simplest chip stack); the candidate
content is freely-redistributable test ROMs (nestest, ZEX, free
homebrew) plus a BYO-ROM file picker.

## Why deferred

### The strongest argument was killed by ROM legality

The April-2026 framing for WASM was "Code198x curriculum pages
embed a playable emulator." That requires shipping content with
the embed — and most retro content is commercial copyrighted ROMs
that cannot be bundled in a web embed. This is the same argument
that killed the unified launcher's web framing in
[`no-unified-launcher.md`](no-unified-launcher.md) and the
debugger's web frontend in
[`debugger-architecture.md`](debugger-architecture.md).

A WASM emulator with no content is half a demo. The audience this
serves (Code198x learners) gets more value from a native binary
they install once than from a degraded web embed.

### No version-tied pressure to ship a demo

The [`versioning-milestones.md`](versioning-milestones.md) decision
established that there's no binary 1.0 milestone. Without a 1.0
event to anchor "we shipped WASM by 1.0!" marketing, there's no
calendar pressure pulling WASM forward.

### The engineering surface is real

A WASM build for an emulator that currently uses wgpu / winit /
cpal / gilrs needs:

- Web audio (different API from cpal — `AudioContext` /
  `AudioWorklet` or wgpu-compute-shader audio)
- Gamepad API (gilrs doesn't target web; need a separate web-gamepad
  layer)
- File I/O via File System Access API or `<input type="file">`
  (no `std::fs` on the web)
- Storage via IndexedDB or LocalStorage instead of `~/.emu198x/`
- Persistent canvas / wgpu surface setup
- Build tooling: `wasm-bindgen`, `wasm-pack`, hosting story
- Performance tuning: WASM speed is usually 60-80% of native; for
  cycle-accurate emulation that matters

Best case: each per-system binary gains a WASM variant with the
host layer abstracted behind a trait. Realistic effort: 6-8 weeks
for a single-system MVP. Maintenance overhead forever after.

### The remaining cases don't justify the investment

What's left after the strongest case is gone:

- **Demo page running test ROMs:** "see Spectrum boot and run
  nestest" or similar. Cute but low audience value — the audience
  for "I want to see your emulator run a test ROM" is small.
- **itch.io WASM audience:** real audience but small overlap with
  "wants accuracy-focused emulation." itch.io users mostly want
  to play games, which loops back to ROM legality.
- **Curriculum BYO-ROM via file picker:** technically works, but
  Code198x lesson UX of "drag your ROM file into the page" is
  awkward compared to "follow these CLI commands to test what you
  wrote." Native binaries serve learners better.

Add them together and the total demand is "nice to have someday,"
not "ship now."

## Triggers that would re-open this decision

If any of these become true, the decision is re-opened:

1. **A Code198x lesson UX surfaces that genuinely needs WASM** —
   not a generic "we should have a web version" framing, but a
   specific concrete lesson interaction that the native binary
   can't serve.
2. **A specific publishing partner needs the embed** — a magazine,
   a tutorial site, an academic course wants an Emu198x embed and
   it's worth the investment to support them.
3. **A contributor offers to do the WASM work** — if someone else
   wants to do the engineering, the cost/benefit looks different.
4. **A specific test-ROM-only demo page becomes a clear product
   need** — the "see this thing run without downloading" framing
   becomes load-bearing for project discovery / marketing /
   onboarding.

None of these are true today.

## What about WASM for the debugger UI?

The [`debugger-architecture.md`](debugger-architecture.md) decision
already settled this: the debugger is native (egui), not web. Same
ROM-legality argument applied; same conclusion. WASM doesn't help
the debugger any more than it helps the emulator.

## What we are NOT doing

- **Building a WASM variant of any per-system binary** in the
  current roadmap. Phase G in the post-October roadmap mentioned
  this; consider it retired by this record.
- **Adding `wasm-bindgen` / `wasm-pack` to the workspace** in
  preparation. Premature.
- **Abstracting the host layer (audio / video / input / storage)
  behind a trait** in anticipation of a WASM backend. The current
  host abstraction (`emu198x-shell`) is already at the right
  level; further abstraction without a concrete second backend is
  speculative.
- **Marketing the project as "web-ready"** or implying WASM is
  on a near-term path.

## What happens if a trigger fires

The realistic first cut:

1. Pick the smallest, most-tested system (Spectrum) as the WASM
   target
2. Build a separate `emu198x-spectrum-web` crate that wraps the
   existing `runtime-sinclair-zx-spectrum` with web-targeted host
   layer (web audio, canvas wgpu, file input, indexeddb storage)
3. Ship a hosted demo page that runs free test ROMs + BYO file
   picker
4. Evaluate against the trigger that opened it (did the lesson
   work? did the partner adopt? did discovery improve?)
5. Decide whether to extend to other systems based on the
   evaluation

Estimated effort: 6-8 weeks for the Spectrum WASM MVP.

## Drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"We should ship a WASM build for Code198x"** — re-read § The
  strongest argument was killed by ROM legality. The
  curriculum-embed angle that justified WASM doesn't work without
  shippable content.
- **"WASM would help discovery / marketing"** — possibly, but no
  concrete signal. Re-read § The remaining cases don't justify
  the investment.
- **"Let's abstract the host layer now so WASM is easy later"** —
  speculative abstraction without a concrete second backend.
  Re-read § What we are NOT doing.
- **"Just a small test-ROM demo page would be cool"** — yes, it
  would. But 6-8 weeks of engineering for "would be cool" is not
  the cost/benefit shape this project optimises for.
- **"WASM for the debugger"** — already settled negatively in
  [`debugger-architecture.md`](debugger-architecture.md); same
  reasons apply.

## Log

### 2026-07-03 — Superseded in part: curriculum embeds are in scope

During the best-in-class strategy session
([`../../../decisions/emu198x-best-in-class.md`](../../../decisions/emu198x-best-in-class.md)),
WASM was named a strategic priority — then this record surfaced as a
direct contradiction and was resolved explicitly rather than
shadowed. Steve chose **supersede for curriculum embeds**:

- **What changed:** the ROM-legality argument (§ The strongest
  argument was killed by ROM legality) assumed embeds need
  commercial game ROMs. Code198x lessons teach *writing* games —
  the embed content is curriculum-owned assembly the learner just
  built, which we can ship freely. And the October system's
  firmware is Amstrad-permissioned for distribution with emulators.
  For that specific case, the killer argument does not apply.
- **What did not change:** the engineering-surface estimate (6–8
  weeks MVP + maintenance) is accepted, not refuted. The deferral
  stands for demo pages, BYO-ROM browser play, and "web-ready"
  marketing. Firmware legality still excludes C64/Amiga embeds
  (Cloanto/Kickstart) until separately resolved.
- **New scope:** `emu198x-spectrum-web`-shaped work (this record's
  § What happens if a trigger fires remains the right first cut),
  a Code198x embed API, curriculum-owned content only. Sequenced
  post-October-protection per the roadmap's amended drift trigger.

Effectively, re-open trigger 1 ("a Code198x lesson UX that genuinely
needs WASM") was judged fired by the live-machines-in-lessons
framing once the content-legality rebuttal was on the table.

### 2026-05-23 — Decision locked

Brainstormed in-session as the WASM sequencing question. Context
shifts since the post-October roadmap draft (no unified launcher,
no binary 1.0 milestone, debugger explicitly native) collapsed
the WASM case. The strongest remaining argument (Code198x
curriculum embed) was killed by ROM legality — the same constraint
that killed the unified launcher's web framing and the debugger's
web frontend.

Decision: defer indefinitely. Re-evaluate only when a trigger
fires (concrete lesson UX need, publishing partner, contributor
offer, test-ROM-only demo becoming load-bearing for discovery).
First-system-if-forced is Spectrum; first-content-if-forced is
freely-redistributable test ROMs + BYO-ROM file picker.

The post-October roadmap's WASM mention in Phase G is now
historical and gets struck through next time that doc is
substantively touched.

# Best-in-class programme — six workstreams

**Date:** 2026-07-03.
**Binding decision:** [`198x/decisions/emu198x-best-in-class.md`](../../../../decisions/emu198x-best-in-class.md).
**Roadmap hook:** [`product-roadmap.md § Best-in-class ladder`](../../knowledge/decisions/product-roadmap.md).
**Origin:** the 2026-07-03 eight-dimension codebase audit plus the strategy
session that followed. This plan turns the decided victory condition — best
unified suite + staged reference-class campaigns + three moats — into
workstreams with definitions of done.

Campaign order (staged, not parallel): **Spectrum → NES/Game Boy → C64 →
Amiga (standing campaign)**. W1 is the floor for everything; W2–W6 are the
programme.

## Capacity and horizon (added 2026-07-03) — read this before treating the workstreams as a schedule

Pace is **evenings and weekends**, in bursts. That fact governs how the rest of
this document is read:

- **This is a multi-year programme, and that is the honest framing.** The six
  workstreams are an *ordering*, not a set of parallel tracks. At this pace,
  attempting more than one campaign's worth of depth at a time is how the whole
  thing stalls.
- **Milestone-gated, not calendar-gated.** Do not attach dates to W2–W6. The
  gate is "the previous rung is done," not "it is now month N." The one hard
  calendar anchor remains external and unchanged: Spectrum SOLID for Crash!
  Live, October 2026 — and that is launch-*hardening* of an already-met bar, not
  new campaign depth.
- **Near-term scope is deliberately small.** The only work in flight at any time
  is: **W1 (the floor)** + **the Spectrum campaign** + **the three unblocking
  prerequisites below**. NES/GB, C64, Amiga, hardware rigs (W3), flux media
  (W5), WASM, and netplay are an *ordered backlog* — real, sequenced, and not
  started until their rung is reached. The plan should never read as if they are
  concurrently underway.
- **Agents are the throughput multiplier, not a second developer.** They extend
  what one person's evenings cover (the W4 compat sweep, regression loops,
  research) — they do not add a parallel campaign's worth of independent
  capacity, and they do not reduce the bus-factor (see
  [`continuity-and-succession.md`](../../knowledge/decisions/continuity-and-succession.md)).

Consequence for W3 (hardware-truth): capital and a genuinely new skillset (flux
imaging, logic analysis) on an evenings-and-weekends budget means W3 starts
**small and late** — one campaign system, one open measurement question at a
time — and the crowdsource-captures-before-buying-rigs option in W3 below is the
preferred first move, not a fallback.

## Near-term unblocking prerequisites (scheduled 2026-07-03)

Three load-bearing prerequisites are pulled out of "someday" into owned
near-term work, because each silently caps a workstream if discovered late:

1. **Resolve the `isa-disasm` git dependency** (unblocks all 63 crates behind
   the embeddable-crates moat). Publish it from Asm198x or make it
   dev-only/feature-gated. Cheap, one-time; do it before more crates accrete the
   blocker. Feeds W6 publishing.
2. **Add the CLA/DCO clause to CONTRIBUTING before the first crate publishes.**
   An hour now; a real mess if an external PR lands on a dual-licensed crate
   without the grant. Gate on this, not on "when publishing starts."
   ([`crate-licensing-split.md`](../../knowledge/decisions/crate-licensing-split.md).)
3. **Open ROM-legality outreach as a slow parallel track.** Start the
   Cloanto/Kickstart conversation via the umbrella
   `canonical-outreach-catalogue.md`; if it stalls, the fallback is a conscious
   scope decision to keep browser/netplay on firmware-free systems (NES,
   Spectrum, Game Boy). It gates two moats on the two flagship systems, so it
   must not sit as an unowned external given.

**Effort calibration (added same day):** the campaign apparatus below (rigs,
flux, demo canon, dashboards) applies to **Tier A** systems only — those with
a living reference-grade incumbent. Tier B (maintained-but-narrower incumbent,
often Windows-only) gets canon-parity + the W4 compat pipeline; Tier C
(dead/absent/legacy-OS incumbents — much of the donor fleet) is best-in-class
by default at the existing accuracy bar: catalogue, peripherals, docs, uniform
tooling. Note the Tier C inversion — no reliable reference exists there, so
per-claim primary-source rigour matters *more* (Sord M5 / Einstein donor-map
lessons), and Tier C is where the W3 "producer of accuracy research"
milestone lands cheapest. Full tier definitions and drift triggers in the
umbrella record.

---

## W1 — The floor (audit debt)

Nothing above is credible while the strongest validation sits `#[ignore]`d on
one machine. From the 2026-07-03 audit, the items that protect claims:

- **Nightly accuracy-tier CI**: a scheduled job that provisions the external
  corpora (Tom Harte, ZEXALL, Lorenz, Dormann, FUSE, Tennant) and runs
  `--ignored` + catalogue + goldens. Plus a fixtures manifest and
  `scripts/check-fixtures.sh`; standardise skip-with-visible-warning.
- **Snapshot round-trips fleet-wide**: generic boot → snapshot → restore →
  assert-identical template across all 28 runtimes (4 covered today).
- **Catalogue backfill**: C64/NES/Amiga manifests to their stated 10 titles;
  wire remaining runtimes into the catalogue dispatch.
- **VIC-I audio + bitmap/multicolor modes** — the one live rule-20 violation
  on a shipping machine.
- **Lint inheritance**: `[lints] workspace = true` on the 20 crates missing it.
- **Golden tolerance mode** (bounded pixel diff) for timing-sensitive rows;
  `#[ignore]` reason taxonomy (`fixture:` / `diagnostic` / `stale: #NNN`).
- **6502 + 6809 `isa_conformance.rs`** mirroring the Z80/68000 pattern; 6809
  external corpus integration.

**Done when:** CI red means an accuracy regression anywhere in the fleet, and
green means the full canon we possess actually ran.

## W2 — Canon + public dashboard

Per campaign system, enumerate the full public test canon, get it green, and
publish a live pass-rate dashboard on the docs site (which is how SameBoy and
Mesen2 earned trust). The dashboard is the "reference-class" claim.

Definition of shoulder-to-shoulder, per system:

- **Spectrum**: Fuse test parity; z80test 6/6 (have); floating-bus and
  contention corpora; full TZX edge-case corpus; top-tier demo corpus
  frame-accurate; RZX replay wired as regression capture.
- **NES / Game Boy**: mealybug full pass (known open); TASVideos accuracy list
  enumerated and green; blargg complete including exact
  `sprdma_and_dmc_dma` counts (Mesen2 reference output, per project memory —
  don't guess); mooneye 75/75 (have); dmg-acid2 pixel-perfect (have);
  SameBoy/Mesen2 test-ROM parity table published.
- **C64**: Lorenz 15/15 (chained-run fix — currently 14/15); VICE testprogs
  repository wholesale in CI; demo canon checkpoints (Edge of Disgrace,
  Lunatico, Uncensored) frame-accurate; SID A/B against real-hardware captures.
- **Amiga**: SPS test images; OCS demo canon; a WinUAE cross-validation harness
  for chipset edge cases (staged — see W-campaign notes below).

## W3 — Hardware-truth pipeline

Graduate from consuming accuracy research (the rule-32
reference-emulator ceiling) to producing it.

- **Acquire**: real machines for campaign systems (48K/128K/+3; frontloader
  NES + DMG; breadbin 6581 + C64C 8580; A500 + A1200), a Greaseweazle-class
  flux imager, a Saleae-class logic analyzer.
- **Workflow**: captures land in the umbrella `reference/` library as a new
  top provenance layer — **hardware-measured** — above datasheet-derived and
  reference-emulator-derived. Every accuracy claim records its provenance.
- **First targets**: SID 6581/8580 A/B recordings vs the filter model;
  Spectrum floating-bus captures; C64 VIC-II timing via the expansion port.
- **Graduation milestone**: publish a test ROM that an incumbent emulator
  fixes a bug against.

## W4 — Agentic compatibility at scale

The asymmetric bet: 28 deterministic, headless, MCP-scriptable machines + an
agent fleet can do in months what took the giants decades of user reports.

- Pipeline: TOSEC sweep → headless deterministic boot → frame-hash
  classification → agent plays each title briefly and grades it → published
  per-system compat database.
- Prototype on Spectrum (everything needed already exists), then scale per
  campaign system.
- Failures feed W2 as minimised repro cases; the compat DB drives peripheral
  prioritisation (what do the failing titles actually need?).

## W5 — Preservation-grade media

Table stakes for "runs protected originals". Ordering follows campaign order:

1. **Spectrum**: +3 fuzzy sectors / weak bits — fix the µPD765A
   rotational-position approximation first (it is exactly what weak-bit
   protection defeats); TZX loader edge cases.
2. **C64**: G64/NIB half-tracks + weak bits; true-drive protected originals.
3. **Amiga**: SCP + IPF (capsimg is vendored at `198x/emulators/emu-libs/` —
   **licence question open**, see below); DMS/ADZ/HDF; DF1–DF3.
4. Cross-cutting: flux-level abstractions shared, not per-system forks.

## W6 — Reach and community

- **Publishing**: per the decided
  [`crate-licensing-split.md`](../../knowledge/decisions/crate-licensing-split.md) —
  dual-license clean-room crates at publish time, per-crate provenance audit,
  ported crates (reSID-derived SID, vAmiga/Mesen2-derived code) stay GPL,
  cleanest crates publish first. Publish `isa-disasm` (or feature-gate it) to
  unblock the 63 crates behind the git dep; per-crate README + `//!` docs +
  `examples/drive_cpu.rs` lifting the pin-level contract out of
  `cpu-bus-interface.md`. Version carve-out per `versioning-strategy.md`.
- **Docs site**: per the decided
  [`docs-site-canon.md`](../../knowledge/decisions/docs-site-canon.md) —
  mdBook; body = docs/systems + decisions + a provenance-reviewed promoted
  subset of the chip/system distillation (promotion rides campaign order,
  private copy deleted at promotion); rewrite the two stub on-ramp docs
  (`architecture.md`, `adding-a-system.md`); no shipped page links to
  unshippable paths; host the W2 dashboards and W4 compat DB as static
  CI-generated artifacts.
- **WASM**: scoped per the superseded-in-part
  [`wasm-sequencing.md`](../../knowledge/decisions/wasm-sequencing.md) —
  Code198x curriculum embeds running curriculum-owned code, Spectrum first
  (firmware-permission systems only; no BYO-ROM browser play, no "web-ready"
  marketing). Web shell (cpal→WebAudio, media loading, script surface in
  place of stdio MCP; no ffmpeg capture in-browser) + a Code198x embed API.
  Cores are pure safe Rust with no sim-path filesystem/RNG/wall-clock
  dependencies, so they compile clean. **Sequencing constraint (binding):**
  must not displace Spectrum launch-hardening before Crash! Live.
- **Heterogeneous netplay (fourth moat)**: the Rachel programme — the same
  game on different machines playing each other. Sequenced: serial/user-port
  peripherals per system (they're also compat items in W5's orbit) → the
  RS232→TCP bridge → RUBP interop per
  `docs/systems/rachel-readiness.md`. No incumbent attempts this; it needs
  exactly the fleet-wide consistency the suite has.
- **Tooling leapfrog** (feeds Forge198x): symbols from Asm198x + real
  breakpoints in the shared `DebugTarget` tier first (fleet-wide at once),
  then the visual layer — event viewer, chip-state viewers, memory heatmaps —
  then rewind/replay (determinism-by-construction makes this cheap; generalise
  the RZX idea).

---

## Sequencing and horizons

| When | What |
|------|------|
| Weeks | W1 complete. W4 Spectrum prototype started. |
| ~2 months | W2 Spectrum + NES/GB canons enumerated and dashboarded — first public reference-class claims. W3 hardware ordered/arriving; capture workflow defined. |
| ~6 months | Spectrum campaign complete (canon + fuzzy-sector media + demo corpus + hardware captures). NES/GB parity table published. W4 scaled to campaign systems. Crates publishing decision executed. |
| ~12 months | C64 campaign substantially complete (Lorenz 15/15, testprogs, demo canon, SID A/B). Amiga standing campaign at stage 1 (OCS demo canon + SPS images). WASM embed shipping in Code198x. |
| Standing | Amiga ladder: stage 2 (IPF/flux originals, DF1–3, HDF), stage 3 (ECS/AGA parity, WinUAE cross-validation). |

October 2026 (Crash! Live, Spectrum-only) remains the hard external anchor;
W1/W2 Spectrum work doubles as launch-hardening.

## Open questions

- **IPF/capsimg licensing** — SPS licence terms vs GPL workspace; may need the
  IPF path as an optional external component. Resolve before W5 Amiga stage.
- **Hardware sourcing/budget** — buy vs borrow per machine; where the rigs
  live; whether community capture partners supplement (they also seed W6
  community).
- **Dashboard hosting** — static on the docs site (generated from nightly CI
  artifacts) vs a small service. Default: static from CI.
- **NES/GB campaign scope** — does it include Famicom/FDS and GB link-cable
  peripherals, or are those post-campaign compat-DB-driven items? Default:
  post-campaign.
- **Binary naming reconciliation** — `crate-naming.md` mandates
  `emu198x-commodore-c64`-style names; the four flagship binaries use the
  short forms it rejects. User-facing, so it gets more expensive after
  October. Needs a call in one direction (rename binaries, or amend the
  naming record) — not yet made.

## Risks

- **Campaign dilution** — the decided failure mode; the umbrella record's
  drift triggers guard it. One campaign at a time (Amiga standing excepted).
- **Hardware-truth becomes a hobby** — captures must serve open model
  questions (provenance-tagged), not accumulate for their own sake.
- **Agentic compat cost** — the TOSEC sweep is large; tier it (boot-classify
  everything cheaply; agent-grade selectively).
- **Licence/provenance pass stalls publishing** — do it per-crate, shipping
  the clean-room crates first (`mos-6502`, `zilog-z80`, `motorola-68000`)
  rather than gating on the whole workspace.

# Code Like It's 198x Commercial Technique Taxonomy

**Status:** Draft
**Date:** 2026-04-12

## Purpose

Define the technique taxonomy that sits behind the `Code Like It's 198x` pattern library.

This is the planning layer for questions like:

- which techniques must be taught to reach commercial-quality work for a given era or machine family
- which of those techniques are universal, and which are machine-specific adaptations
- which other target systems deepen or sharpen a technique beyond the initial `Spectrum`, `NES`, `C64`, and `Amiga` scope
- how the project should record historical exemplars without turning folklore into false certainty

Related planning files:

- [Code Like It's 198x platform requirements](./2026-04-11-code-like-198x-platform-requirements.md)
- [Code Like It's 198x architecture decisions](./2026-04-11-code-like-198x-architecture-decisions.md)
- [Code Like It's 198x pattern library structure](./2026-04-12-code-like-198x-pattern-library-structure.md)
- [Code Like It's 198x pattern backlog](./2026-04-12-code-like-198x-pattern-backlog.md)
- [Code Like It's 198x historical exemplar registry](./2026-04-12-code-like-198x-historical-exemplar-registry.md)

## Working Assumptions

- Lessons remain prose-first and emulator-agnostic.
- The pattern library is a separate layer that teaches reusable technique with working code and buildable artifacts.
- Commercial quality is not just `machine -> techniques`; it depends on era, genre, market expectations, production budget, and storage or memory limits.
- Historical "firsts" should be treated as research claims with evidence levels, not as casual assertions.

## Taxonomy Model

Each technique entry should eventually record:

- `TechniqueId`
- `Name`
- `Category`
- `CommercialPurpose`
- `EraBands`
- `PrimaryFamilies`
- `DeepeningFamilies`
- `Prerequisites`
- `PatternEntries`
- `CommercialReadiness`
- `HistoricalEvidence`
- `TeachingReferences`

## Commercial Readiness Levels

Each technique should be teachable at more than one depth.

- `intro`
  - enough to demonstrate the concept and ship a toy example
- `shipping`
  - enough to support a small but credible commercial game on the target family
- `advanced`
  - enough to support larger content, stronger presentation, or broader genre demands
- `flagship`
  - enough to explain the machine-defining results seen in standout commercial titles

This lets one pattern entry exist at several levels without pretending every lesson must build the final polished form immediately.

## Historical Evidence Model

The public pattern library should not try to publish one simplistic "first appearance" field.

Instead, record:

- `earliest_known_use`
- `earliest_known_commercial_use`
- `breakthrough_popularization`
- `canonical_teaching_exemplar`
- `confidence`
- `notes`

`confidence` should be one of:

- `hypothesis`
- `likely`
- `well-supported`

This gives the project room to teach from strong examples without overclaiming provenance before the research is solid.

## Commercial Quality Axes

Commercial quality should be evaluated against five axes:

- `era`
  - what players and publishers expected at that time
- `machine family`
  - hardware limits, media, and standard input or display expectations
- `genre`
  - platformers, adventures, sports, strategy, shooters, and educational software all demand different technique stacks
- `market tier`
  - budget tape game, full-price disk game, magazine covermount, first-party console release, and premium boxed computer release are not the same target
- `production tooling`
  - many late commercial techniques are really pipeline or content-production advances rather than one clever runtime trick

## Top-Level Categories

### 1. Universal Craft

These patterns apply almost everywhere:

- game loops and frame or tick structure
- input sampling and debouncing
- object update ordering
- collision and response
- state machines
- randomness and replayability
- score, lives, health, progression, and difficulty curves

### 2. Rendering And Display

These patterns are the most machine-shaped:

- text and character-cell presentation
- software sprite redraw
- hardware sprite composition
- tile maps and metasprites
- scrolling and camera control
- status bars, split screens, and raster-time updates
- palette strategy, color restrictions, and artifact-aware rendering
- bitmap, bitplane, and mixed-mode composition

### 3. Audio And Music

- beeper and one-bit sound design
- PSG and APU sound effect handling
- music driver structure
- channel prioritization
- sample playback
- tracker-style production
- audio memory budgeting

### 4. Input, Feel, And Human Factors

- keyboard-first controls
- joystick-first controls
- keyboard-to-joystick mapping
- mouse or hybrid input
- latency and repeat behavior
- pause, remap, trainer, and accessibility concessions

### 5. Memory, Data, And Content Scale

- tight memory budgeting
- bank switching
- streaming and staged loading
- compression and decompression
- data-driven entities and levels
- save data, passwords, high scores, and persistent progress

### 6. Storage, Loading, And Distribution

- tape loaders and loader UX
- disk boot flow
- cartridge startup constraints
- firmware or Kickstart dependencies
- fast loaders and custom loaders
- legal and practical packaging of redistributable artifacts

### 7. UI, Presentation, And Commercial Polish

- title screens
- options and pause flows
- attract mode or demos
- transitions and cutscenes
- tutorialization and affordances
- credits, high scores, save or password entry, and failure recovery

### 8. Tooling And Production Pipeline

- cross-assemblers and build systems
- data conversion
- map or room pipelines
- music and sound import
- test harnesses and hardware validation
- asset compression and packaging

### 9. Platform-Specific Exploitation

- VIC-II raster IRQ scheduling
- NES NMI-driven frame work
- Amiga Copper and Blitter orchestration
- Spectrum contention-aware timing
- mapper-specific content expansion
- clone- or enhancement-aware capability negotiation

## Initial Family Anchors

These are the first families that should shape the initial pattern library.

| Family | What it anchors | Why it matters |
|---|---|---|
| `Spectrum` | software sprite redraw, attribute-aware art, keyboard-matrix flows, tape-era commercial packaging | forces the library to teach how to ship games without hardware sprites or generous memory |
| `NES` | tile and metasprite pipelines, NMI-driven frame flow, scrolling cameras, mapper-era content growth | anchors console-era structured rendering and cartridge-driven scale |
| `C64` | hardware sprites, multiplexing, raster splits, SID-driven identity, disk and tape commercial workflows | anchors the line between machine tricks and shippable production values |
| `Amiga` | bitplanes, DMA arbitration, Copper lists, blitter-assisted composition, sample-driven audio, floppy-era production scale | anchors the transition from 8-bit constraints to larger 16-bit presentation and tooling expectations |

## Where Other Target Systems Deepen The Approaches

The broader catalog should not be treated as "more of the same." Many families sharpen or complicate the core techniques.

| Family Or Group | Deepens Which Approaches | Why It Changes The Teaching |
|---|---|---|
| `ZX80` / `ZX81` | text-first interaction, ultra-low-memory design, visible display-generation tradeoffs | strips the machine back to fundamentals and makes timing and memory austerity concrete |
| `Timex`, `Pentagon`, `Scorpion`, and other Spectrum clones | compatibility boundaries, alternate video timings, banked-memory differences, clone-aware testing | turns "Spectrum technique" into a compatibility discipline rather than one machine assumption |
| `Spectrum Next` | enhanced graphics layers, sprites, DMA-like assists, SD-backed workflows, modern retrofit expectations | shows how a family can grow beyond its classic baseline without replacing the need to teach the baseline |
| `PET` | keyboard-first UX, text and business-software presentation, character-set animation, very constrained memory layouts | deepens early microcomputer software design before game-first assumptions harden |
| `VIC-20` | cartridge and tape pragmatism, tiny-memory game design, character-cell action, budget-era packaging | makes commercial compromise visible very early in the stack |
| `C16` / `Plus/4` | TED-driven color and audio tradeoffs, low-cost software production, family divergence inside a vendor line | prevents the Commodore path from collapsing into `VIC-II` assumptions |
| `Atari 2600` | beam racing, kernel loops, scanline budgeting, display-time gameplay tradeoffs | forces a beam-driven pattern family that does not fit tile- or framebuffer-first thinking |
| `Apple II` | artifact-color strategy, mixed text and graphics, disk-driven workflow, educational and business crossover | adds display and market expectations that differ sharply from Sinclair or Commodore |
| `MSX` | VDP-oriented design, slot architecture, ROM and disk coexistence, regional software ecosystems | deepens the `Z80` story without letting `Spectrum` define all `Z80`-era teaching |
| `SG-1000`, `Master System`, `Game Gear` | tile and sprite console baselines, VDP-driven pipelines, handheld adaptation | gives a second 8-bit console line that is not shaped by NES assumptions |
| `Game Boy` / `Game Boy Color` | handheld UI density, battery-backed saves, low-resolution readability, mobile session design | deepens portable-first commercial constraints |
| `PC Engine` | richer console scrolling, palette-heavy art, CD transition pressures, HuCard baseline discipline | helps bridge 8-bit console craft toward 16-bit presentation without full `D` complexity |
| `Mega Drive` / `Genesis` | larger scrolling worlds, sprite throughput limits, FM plus PSG audio identity, faster action conventions | expands action-game and audio-pipeline patterns sharply |
| `SNES` | DMA and HDMA presentation tricks, multi-layer polish, sample-based audio pipelines, broader genre expectations | deepens how commercial polish gets tied to hardware scheduling and content tools |
| `Atari ST` | 16-bit computer production without Amiga custom-chip assumptions, mouse-led software UX, disk-centric packaging | gives a useful counterweight to Amiga-specific thinking |
| `Amiga ECS` / `AGA` | enhanced display modes, bigger-memory content scale, more advanced asset expectations | deepens the Amiga path from early flagships to later production realities |

## Technique Ladders For The Initial Four Families

The first shipping pattern library should cover at least these ladders.

### Spectrum

- keyboard and joystick input handling
- screen and attribute memory
- software sprite redraw and dirty-region strategy
- collision and room or screen transitions
- tape-friendly packaging and save or high-score expectations
- beeper-first audio, then `AY` where the family expands
- `128K` banked-memory techniques after `48K` baseline patterns are stable

### NES

- frame pipeline around NMI
- background and sprite memory preparation
- metasprites, animation, and sprite-limit-aware design
- scrolling and camera control
- mapper-aware content growth once `NROM` techniques are solid
- APU-based SFX and music structuring
- password or battery-save flows where the game design warrants them

### C64

- joystick and keyboard flow
- VIC-II character and bitmap presentation choices
- hardware sprites and multiplexing
- raster IRQ scheduling and split-screen UI
- SID identity, channel budgeting, and SFX vs music priority
- disk or tape startup, loaders, and high-score persistence

### Amiga

- 68000 update structure and DMA awareness
- bitplanes and sprite or BOB strategy
- Copper lists for display orchestration
- Blitter usage and memory bandwidth budgeting
- Paula sample playback and tracker-era music thinking
- floppy-driven content layout and staged loading

## Cross-Machine Pattern Groups

The pattern library should teach these as shared concepts with family-specific variants rather than as isolated machine tricks:

- main loop and frame structure
- input abstraction and feel
- software sprite redraw
- hardware sprite composition
- scrolling camera
- status bars and split-screen presentation
- audio driver structure
- save, password, or persistent score handling
- data-driven content and banked or streamed assets
- startup, title, and attract-mode presentation
- loading experience and commercial packaging
- debugging, instrumentation, and regression testing

## Seed Historical Exemplar Candidates

These are starting points for the future evidence registry, not final claims.

| Technique Area | Candidate Exemplars To Track | Reason |
|---|---|---|
| attribute-aware platform presentation | `Manic Miner`, `Jet Set Willy`, `Head Over Heels` | strong teaching anchors for Spectrum-era visual compromise and readability |
| software-room or screen composition | `Knight Lore`, `Alien 8`, `Fairlight` | useful for depth, layout, and redraw strategy |
| hardware sprite multiplexing and raster polish | `Uridium`, `The Last Ninja`, `Mayhem in Monsterland` | strong C64-era exemplars of commercial presentation pressure |
| NMI-driven platform scrolling and controller feel | `Super Mario Bros.`, `Super Mario Bros. 3`, `Mega Man 2` | canonical teaching anchors for NES-era commercial action design |
| battery save and persistent world structure | `The Legend of Zelda`, later `Game Boy` and `SNES` exemplars | useful anchor for persistence as a commercial expectation |
| Copper and Blitter display orchestration | `Defender of the Crown`, `Shadow of the Beast`, `Agony` | strong Amiga-era anchors for spectacle and scheduling |
| sample-driven and tracker-friendly audio identity | `Turrican`, `Lemmings`, `Lotus Turbo Challenge 2` | good anchors for audio production technique on Amiga-class systems |
| beam-driven display-time programming | `Combat`, `Pitfall!`, later high-skill `Atari 2600` titles | anchors a technique family that does not map cleanly onto the initial four families |
| artifact-color and mixed-mode visual strategy | `Ultima II`, `Prince of Persia`, Apple II-era educational and action titles | useful for display models outside the mainstream tile or sprite story |

Before publishing these as historical claims, the project should verify them against a research process with provenance notes.

## Pattern-Library Implications

- Do not build the pattern library as one giant machine-indexed checklist.
- Do build it as a cross-machine technique graph with family-specific implementations.
- Use the initial four families to anchor the first wave, but let other families deepen the same concepts rather than forcing entirely separate curricula.
- Track historical exemplars as a research layer attached to patterns, not as the primary navigation structure.

## Next Planning Step

Turn this taxonomy into:

- a first pattern-library backlog for the initial four families
- a machine-deepening map that says which later systems should extend which existing patterns
- a separate historical evidence registry with explicit claim types and confidence levels

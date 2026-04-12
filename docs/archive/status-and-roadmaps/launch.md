# Emu198x Launch and Productisation Strategy

## 1. Core positioning

Emu198x is not competing with RetroArch on game playing. RetroArch already does that well enough for most people, and "more accurate" is not a compelling pitch for casual users.

Emu198x's positioning is:

**For developers:** The first integrated development environment for retro hardware — edit, assemble, load, run, and source-level debug in one tool with live register/memory/video/audio views. No other emulator offers this as a unified, polished experience.

**For educators and learners:** A teaching tool that lets you see inside the machine at every level — watch memory change as BASIC runs, see how the ULA builds a frame, hear individual audio channels, print to an emulated ZX Printer. The companion to CL198x.

**For content creators:** The easiest way to produce beautiful retro computing content — screenshots with authentic CRT rendering, video capture, GIF export, per-channel audio extraction, asset ripping. One click to capture, one click to share.

**For preservationists:** A verification and cataloguing tool — batch processing against TOSEC/No-Intro, format conversion, metadata extraction, automated regression testing. Preservation infrastructure, not just an emulator.

**For the retro community:** A love letter to the machines, built by someone who was there. Accurate, beautiful, and deeply respectful of the hardware.

The tagline candidates:

- "See inside the machine"
- "Every system. Every detail."
- "The retro computing workshop"

---

## 2. Audiences

### 2.1 Primary audiences

| Audience | Size | Motivation | What they need | Where they are |
|----------|------|-----------|----------------|----------------|
| Homebrew developers | Small but vocal | Making new software for old hardware | IDE, debugger, accurate timing | Forums (CSPect, NESDev, Lemon64), Discord, GitHub |
| Demoscene | Small, influential | Pushing hardware limits | Cycle accuracy, signal-level fidelity | Pouët, scene parties, Discord |
| Educators / learners | Growing | Understanding computing fundamentals | BASIC loading, observation tools, CL198x integration | Schools, YouTube, coding clubs |
| Content creators | Medium | Producing retro content (YouTube, blogs, podcasts) | Capture, CRT filters, audio extraction | YouTube, Twitter/X, Mastodon, Reddit |
| Preservationists | Small, dedicated | Archiving and verifying software | Batch tools, format support, database integration | Archive.org, TOSEC, No-Intro, Redump communities |

### 2.2 Secondary audiences

| Audience | What draws them in |
|----------|-------------------|
| Casual retro gamers | Beautiful CRT rendering, "it just works" experience, bundled homebrew |
| Music producers / chiptune | Per-channel audio export, register log capture |
| Pixel artists | Asset export, tile/sprite ripping with correct palettes |
| Computer science students | Observable machine internals for architecture courses |
| Journalists / historians | Screenshot and video capture for articles and documentaries |

### 2.3 Anti-audiences

People we are not primarily building for:

- Users who want the fastest way to play ROMs (RetroArch serves them well)
- Users who want achievements, cloud saves, social features (RetroArch/Steam)
- Users who want mobile-first (possible future, not launch priority)

---

## 3. Per-system release strategy

### 3.1 Principle

Each system is released as an independent, complete product. "Emu198x for ZX Spectrum" is a news event. "Emu198x now supports the C64" is another. Don't announce 100 systems on day one — that's a promise, not a product.

### 3.2 Release order

The order is driven by three factors: personal connection (Spectrum first — it's the heart of the project), architectural value (each system proves something about the platform), and audience reach (each system taps a different community).

| Order | System | Why this order | Community |
|-------|--------|---------------|-----------|
| 1 | **ZX Spectrum** | Personal connection, CL198x content, UK retro community, tests tape/ULA/contention | World of Spectrum, Spectrum Computing, ZZAP! Live, Revival |
| 2 | **Commodore 64** | Largest retro community globally, tests SID/VIC-II/IEC, CL198x content | Lemon64, CSDB, C64 Scene Database |
| 3 | **NES / Famicom** | Largest homebrew community, tests mapper system/PPU/expansion audio | NESDev, ROM hacking community |
| 4 | **BBC Micro** | UK education angle, tests Econet/second processors, strong preservation community | StarDot forums, Centre for Computing History |
| 5 | **Amstrad CPC** | Underserved by existing emulators, strong European community | CPCWiki, CPC-Power |
| 6 | **Amiga** | Tests OCS/ECS/AGA/accelerators, large community, complex hardware | EAB (English Amiga Board), Amiga.org |
| 7 | **Atari 8-bit** | Underserved, unique architecture (ANTIC/GTIA/POKEY) | AtariAge |
| 8 | **ZX Spectrum Next** | Modern recreation, proves extension architecture, Next community is active and engaged | Spectrum Next forums, KickStarter backers |
| 9 | **MSX** | International appeal (Japan, Brazil, Middle East, Europe) | MSX.org, MSX Resource Center |
| 10 | **Mega Drive / Genesis** | Large community, tests 68000+Z80 dual CPU | Sega Retro, ROM hacking community |
| 11+ | SNES, Master System, Game Boy, Atari ST, Apple II, PC Engine, etc. | Expand based on demand and contribution interest | Various |

### 3.3 What "release" means

A system release includes:

- All common variants (e.g., Spectrum 48K, 128K, +2, +2A, +3 — not just one model)
- All major media formats for that system
- System-specific debug views
- Input mapping with sensible defaults
- CRT preset tuned for the system's native output
- A curated set of bundled freely-available homebrew
- User-facing documentation for that system
- At least one "wow moment" demo (loading screech for Spectrum, SID music visualisation for C64, composite artefacting for NES)

### 3.4 Release cadence

No fixed schedule. Release when it's ready. But the gap between system 1 (Spectrum) and system 2 (C64) should be short enough to establish momentum — ideally within 2-3 months. After that, the shared infrastructure is proven and each new system is primarily machine-specific work.

---

## 4. First-run experience

### 4.1 The problem

Every emulator's first-run experience is terrible. Download → launch → empty window → "where do I get ROMs?" → Google → sketchy sites → confusion about file formats → give up.

### 4.2 The solution

**Step 1: System selector.** On first launch, show a visual system selector. Each system shows a photograph or illustration of the hardware, its name, year, and a one-line description. Systems without configured ROMs are shown but dimmed with a "Set up" badge.

**Step 2: Bundled content.** Each supported system ships with a curated homebrew collection (see §5). The user can immediately launch a game or demo without supplying any ROMs. The first thing they see is a running system with beautiful CRT rendering, not a configuration dialog.

**Step 3: ROM setup (optional).** When the user wants to load their own software, a guided flow helps them: point to a directory → emu198x scans and identifies ROMs by hash → reports what it found → organises by system automatically. No manual file-by-file configuration.

**Step 4: Quick start.** For each system, a "Quick Start" overlay (dismissable, shown once) explains: how to load software, keyboard shortcuts, where to find the debug panels, how to capture screenshots.

### 4.3 The "wow" moment

Within 30 seconds of first launch, the user should experience something that makes them think "this is different." Candidates:

- The Spectrum loading screech playing through the CRT-filtered border stripes — audio and visual, together, authentically
- The per-channel audio visualiser showing a SID tune with three voices dancing
- The NES composite artefacting producing colours that don't exist in the palette
- The ZX Printer rendering output with silver-paper appearance
- Typing a BASIC program and watching memory change in the hex editor in real time

### 4.4 Open-source ROM fallbacks

Where open-source ROM replacements exist, bundle them:

| System | Open-source ROM | Notes |
|--------|----------------|-------|
| Spectrum 48K | Open SE BASIC / SE BASIC IV | GPL, functionally compatible |
| Spectrum 128K | Partial replacements exist | May need community contribution |
| C64 | OpenROMs / open-roms project | LGPL, partial compatibility |
| NES | No system ROM needed | Cartridge-based, ROM is the game |
| Game Boy | Replacement boot ROM exists | SameBoy boot ROM |

For systems without open-source ROM replacements, the emulator works but requires user-supplied ROMs. The ROM setup flow (§4.2 step 3) handles this gracefully.

---

## 5. Bundled homebrew showcase

### 5.1 Principle

Every supported system ships with 5-10 freely distributable homebrew titles that demonstrate the system's capabilities and the emulator's features. These are not filler — they're curated to show off specific emu198x features.

### 5.2 Curation criteria

- **Freely distributable** — explicit permission from the author, or released under a permissive license
- **Technically impressive** — demonstrates something the system does well
- **Showcases emu198x features** — uses audio channels worth visualising, has interesting video worth inspecting, loads from tape/disk in an interesting way
- **Diverse** — mix of games, demos, utilities, music disks
- **Quality** — polished, not broken, good first impression

### 5.3 Sourcing

Contact homebrew authors directly. Most are enthusiastic about their work being showcased. Offer clear attribution in the emulator (author name, release year, link to their page). The retro community is small enough that personal outreach works.

Sources: Pouët (demos), itch.io (homebrew games), CSDb (C64 scene), World of Spectrum archives (freely released Spectrum software), NESDev competition entries.

### 5.4 Per-system showcase targets

| System | Showcase focus |
|--------|---------------|
| Spectrum | Loading screech demo, border effects demo, AY music, BASIC program |
| C64 | SID music visualisation, sprite multiplexer demo, BASIC program |
| NES | Mapper variety, expansion audio (if homebrew uses it), sprite-heavy game |
| BBC Micro | Mode 7 demo, Econet showcase (if applicable), BBC BASIC program |
| Amiga | Copper effects, MOD music with per-channel visualisation, blitter demo |

---

## 6. Website and online presence

### 6.1 Domain

emu198x.com (or .org). Not a GitHub Pages site — a proper domain with its own identity.

### 6.2 Site structure

- **Home** — hero section with CRT-filtered screenshot, tagline, download button, 90-second demo video
- **Systems** — one page per supported system with screenshots, feature list, download
- **Features** — IDE, debugger, CRT filters, capture pipeline, audio visualisation, each with screenshots/GIFs
- **Documentation** — getting started, per-system guides, developer guide (adding systems), architecture overview
- **Blog** — release announcements, development updates, technical deep-dives (cross-posted to stevehill.xyz)
- **Community** — Discord link, GitHub link, contribution guide

### 6.3 Visual identity

The CRT filter is the visual identity. Every screenshot on the website should use the Clean RGB or PVM preset — the authentic look, not raw pixels. The website itself should be clean and modern (not retro-themed — let the content be retro, let the presentation be professional).

### 6.4 Demo video

90 seconds. No narration — just captions and the sounds of the machines. Structure:

1. (0-15s) System selector → pick Spectrum → loading screech plays → game loads
2. (15-30s) Open debug panels — disassembly, memory, tile viewer. Step through code.
3. (30-45s) Switch to C64 → SID tune playing → per-channel audio visualiser
4. (45-60s) IDE workflow — edit assembly → assemble → run → hit breakpoint → inspect
5. (60-75s) CRT filter comparison — Development mode vs Clean RGB vs Consumer TV vs RF
6. (75-85s) Capture — one-click GIF, screenshot with CRT, per-channel audio export
7. (85-90s) Logo. Download link.

Post on: YouTube, Twitter/X, Mastodon, Reddit (r/retrogaming, r/retrocomputing, system-specific subreddits), Hacker News, relevant forums.

---

## 7. Community and social

### 7.1 Discord server

Set up from day one, even before launch. Channels:

- `#announcements` — releases, blog posts
- `#general` — discussion
- `#spectrum`, `#c64`, `#nes`, `#amiga`, etc. — per-system channels, added as systems launch
- `#development` — IDE, assembler, homebrew development using emu198x
- `#cl198x` — CL198x learners
- `#bug-reports` — structured bug reporting
- `#feature-requests` — community input
- `#showcase` — users sharing GIFs, screenshots, projects made with emu198x

### 7.2 Existing community connections

Leverage personal connections for warm introductions:

| Connection | How they can help |
|-----------|-------------------|
| David Pleasance | Amiga community credibility, potential foreword/endorsement |
| Oliver Twins | Game development angle, industry credibility |
| ZZAP! Live / Revival attendees | Word of mouth, demo opportunities |
| Retro computing YouTubers | Reviews, coverage (approach after first system release) |
| CPC / Spectrum / C64 forum moderators | Announcements in relevant communities |

### 7.3 Conference presence

Events to demo at (already attending or connected to):

| Event | Audience | Demo focus |
|-------|----------|------------|
| ZZAP! Live | C64/retro community | SID visualisation, C64 IDE |
| Revival | Broad retro | Multi-system demo, CRT filters |
| RetroFest / Play Expo | Gaming-focused retro | Game loading, capture features |
| Acorn/BBC events (Centre for Computing History) | BBC Micro community | Econet, BBC BASIC, second processors |

A physical demo station with a real CRT alongside the emulator running on a laptop would be a powerful visual comparison — "here's the real hardware, here's emu198x, can you tell which is which?"

---

## 8. Content marketing

### 8.1 Blog posts (stevehill.xyz and emu198x.com)

Each blog post should be genuinely interesting on its own, not just a product announcement. Topics:

**Technical deep-dives (developer audience):**
- "Why your emulator's CRT filter looks wrong" — phosphor blur, PAR, signal decode
- "The NMI race condition: why master clock timing matters"
- "Building an Econet file server in 2026"
- "How the SID filter actually works (and how to emulate it)"
- "The ZX Spectrum's floating bus: every emulator gets it wrong"

**Educational (learner audience):**
- "What happens when you type PRINT on a Spectrum" — trace from keypress to screen
- "Inside the NES PPU: how 8-bit consoles draw pictures"
- "Your first assembly language program (in the emulator)"

**Preservation (archivist audience):**
- "Batch-verifying 10,000 Spectrum tapes with emu198x"
- "Why some TZX files don't load (and how we fix them without hacks)"

**Human interest (broad audience):**
- "Why I'm building an emulator in 2026"
- "The machines that taught a generation to code"

### 8.2 Social media cadence

Don't force a schedule. Post when there's something worth showing:

- Development progress GIFs (captured by emu198x itself — dog-fooding the capture pipeline)
- Before/after comparisons (raw pixels vs CRT filter)
- "Today I learned" moments from hardware research
- Per-channel audio clips
- Side-by-side with real hardware

### 8.3 Cross-promotion with CL198x

CL198x lessons reference emu198x as the recommended development environment. Emu198x's BASIC editor and IDE are the tools CL198x teaches with. The Vault entries in CL198x link to emu198x for running the referenced software. They're complementary products that drive users to each other.

### 8.4 Cross-promotion with books

"Rescue Engineering" can reference emu198x as a case study in architecture. The vintage computing love letter book can include screenshots produced by emu198x's capture pipeline with CRT filters. The CL198x technical book series is directly built on the emulator.

---

## 9. Distribution

### 9.1 Platforms

| Platform | Distribution | Priority |
|----------|-------------|----------|
| macOS | .dmg download, Homebrew cask | High (primary dev platform) |
| Windows | .msi or portable .zip, winget | High (largest user base) |
| Linux | AppImage, Flatpak, distro packages | Medium |
| Web (WASM) | Hosted at emu198x.com | Medium (demo/education, limited features) |
| iOS / Android | Not at launch | Future (if demand warrants) |

### 9.2 Auto-update

Not essential for v1 but desirable. Consider Sparkle (macOS), WinSparkle (Windows), or a simple "new version available" check on launch.

### 9.3 WASM demo

The web shell serves as a try-before-you-download demo. Host it at emu198x.com/try/ with the bundled homebrew pre-loaded. Limited feature set (no multi-window, no video capture, no MCP) but demonstrates the core experience. This is the single most effective conversion tool — someone clicks a link, plays a game in their browser, thinks "I want the full version."

---

## 10. Metrics and feedback

### 10.1 What to measure

- Downloads per system per platform (from website analytics)
- Discord member count and activity
- GitHub stars, forks, issues, PRs
- Blog post / demo video views
- Homebrew showcase plays (if tracked in WASM demo)
- Bug reports per system (quality signal — more reports on a new system = more users trying it)

### 10.2 What not to measure

- Daily active users (we're not a SaaS product)
- Retention (people use emulators in bursts, not daily)
- Revenue (it's free)

### 10.3 Feedback channels

- GitHub issues (structured bug reports, feature requests)
- Discord (informal feedback, community pulse)
- Thumbs up/down on bundled homebrew (helps curate the showcase)

---

## 11. Timeline sketch

This is illustrative, not a commitment. Adjust based on reality.

| Milestone | Target | Notes |
|-----------|--------|-------|
| Architecture doc complete | Done | This document + the architecture doc |
| Website domain + placeholder | Before first release | "Coming soon" with email signup |
| Discord server | Before first release | Invite early testers |
| **Spectrum alpha** | When it works | Private testing with trusted community members |
| **Spectrum release** | When it's polished | First public release, blog post, demo video |
| CL198x Spectrum integration | Shortly after Spectrum release | First CL198x lessons using emu198x |
| **C64 release** | 2-3 months after Spectrum | Second system, establishes multi-system credibility |
| **NES release** | 2-3 months after C64 | Taps into largest homebrew community |
| Conference demo | Next available event after first release | Live demo at Revival, ZZAP! Live, or similar |
| WASM demo live | After 2-3 systems | Try-in-browser experience |
| **BBC Micro release** | When ready | UK education angle, Econet showcase |

---

## 12. Things not to do

- Don't announce a 100-system roadmap. Ship one system at a time. Let the architecture speak through results, not promises.
- Don't theme the website/UI with retro aesthetics (scanline backgrounds, pixel fonts). The content is retro. The presentation is professional. Retro-themed websites signal "hobby project" not "serious tool."
- Don't compete on feature checklists. Compete on quality and experience. One system done beautifully beats ten done adequately.
- Don't ask for donations before shipping. Ship first. If people use it and value it, contribution options (GitHub Sponsors, Patreon, Ko-fi) can follow.
- Don't write a manifesto about emulation philosophy. Show, don't tell. The CRT filter, the audio visualiser, the IDE workflow — these demonstrate the philosophy more effectively than any essay.
- Don't gatekeep. The preservationist who wants batch processing and the kid who wants to play Manic Miner are equally valid users. Design for both.

# Speedlock — reference for our open Spectrum tape bugs

External reference notes compiled 2026-05-17 while investigating the
silent-music cluster. Speedlock is the most prevalent commercial tape
protection on the ZX Spectrum (and ported to Amstrad CPC), used in
hundreds of late-1980s games, the majority from Ocean / U.S. Gold /
Imagine / Firebird / Hit Squad. We hit it in three of our open
threads:

- [`speedlock-tape-incomplete.md`](../decisions/speedlock-tape-incomplete.md) — Green Beret wedge,
  Speedlock 2 anti-tamper at `$fbcb` via the `$feb3` indicator
- [`speedlock-silent-music.md`](../decisions/speedlock-silent-music.md) — silent music on
  Rainbow Islands / Bubble Bobble / Out Run / RoboCop / Operation Wolf,
  Speedlock 4 (Out Run) and Speedlock 7 (the others)
- `spectrum-plus3-disk-loading-incomplete.md` (closed) —
  Speedlock 7+ on the +3 disk path; cleared by the
  five-root-cause FDC fix on 2026-05-10

## Version landscape

Compiled from Alessandro Grussu's [ZX Spectrum Loading
Schemes](https://www.alessandrogrussu.it/loading/Schemes/schemes.html)
catalogue, cross-referenced against
[Wikipedia](https://en.wikipedia.org/knowledge/Speedlock) and Craig's Retro
Computing [protection-systems
deep-dive](https://craigsretrocomputingpage.eu5.org/howtohack/commercialprotectionsys.html).
Original authors: **David Aubrey-Jones** and **David Looker**, 1983.
First commercial release: Daley Thompson's Decathlon, Ocean, October
1984.

| Version | Era | Sample games | In our catalogue |
|---|---|---|---|
| 1 | 1983-84 | Daley Thompson's Decathlon, Knight Lore, Beach Head, Mikie | — |
| 2 | mid-80s | Enduro Racer, **Green Beret** | `green-beret*` (wedge — see other doc) |
| 3 | 1985-86 | Leviathan | — |
| 4 | 1986-87 | Arkanoid, Athena, **Out Run** | `out-run*` (silent music) |
| 5 | 1987 | Road Blasters, **Bubble Bobble** | `bubble-bobble*` (silent + controls menu) |
| 6 | 1987-88 | The Fury, Platoon | — |
| 7 | 1988+ (Hit Squad re-releases) | **RoboCop**, **Operation Wolf**, **Rainbow Islands** | `robocop*`, `operation-wolf*`, `rainbow-islands*` |
| 8 (?) | 1991+ | RoboCop 3 | — |

## Three generations of anti-tamper

Per Craig's Retro Computing, Speedlock's protection machinery
**groups by generation** (Type 1 / 2 / 3), not version-by-version.
Versions 1-3 progress through the three types; 4-7 elaborate Type 3
with additional decryption + checksum layers.

### Type 1 (Speedlock 1)

- One or two BASIC loaders
- Main code loaded via distinctive "clicking" leader tones
- Decryption uses **IY register XOR** tricks: the loader manipulates
  IY (normally `$5C3A`, BASIC sysvar base) freely with interrupts
  disabled
- **Exploits undocumented Z80 ops**: `FD` prefix on operations
  targeting IY-halves (IYH / IYL) — the Z80 silently treats IY as
  splittable into 8-bit halves even though Zilog's spec doesn't
  document this
- Cracking attack: manually set HL / DE / BC / A to the
  pre-decryption values, CALL the decryption routine directly,
  restore IY to `$5C3A` before resuming

### Type 2 (Speedlock 2)

- Single short BASIC loader + one extended CODE block
- Audible "beep" sequences during loading
- Countdown timer display in the border
- **Six cascading decrypters** before a complex moving routine
- Anti-Multiface check in the third decrypter (Multiface = popular
  cartridge for snapshot/cracking; crashing when present is the
  protection's "if you're trying to read me, die" guard)
- **LD SP, XXXX self-modification**: the loader searches for its own
  `LD SP, n` instruction (opcode `0x31` + two bytes), discovers its
  own runtime location, then relocates itself — reconstructable via
  stack trace
- Our Green Beret thread (`speedlock-tape-incomplete.md`) traced
  three separate Speedlock 2 wipe triggers:
  - `$fd6c`: `LD A,$3A / CP L / JP NZ $fbcb` — bit-shift verifier
    (fixed in commit `80ec856` via the TZX partial-last-byte parser)
  - `$feca`: XOR-fold check, `H` accumulates XOR across iterations
  - `$ff00`: `LD A,($feb3) / OR A / CALL NZ, $fbcb` — runtime indicator

### Type 3 (Speedlock 3-7)

- Single extended BASIC loader
- **~144 cascading decrypters** (per Speedlock 3 analysis)
- Crashes if Multiface remains active during decryption
- **Post-decryption anti-tamper**: a fatal check fires after the
  visible loader phase completes — the symptom in our catalogue is
  the loader entering a "decoy" state (LD-BYTES key-scan stall) or
  the music driver silently refusing to write AY registers
- Initial code moves remaining instructions into correct memory
  addresses
- A CALL instruction at the loader's beginning is **modified
  throughout decrypter execution** — its target address is
  transformed by the decryption chain itself, so a static disassembly
  of the loader doesn't show where it actually ends up calling

### Speedlock 4-7 encryption parameters

From [SkoolKit
documentation](https://skoolkit.ca/) for `tap2sna.py`'s Speedlock
support:

- **XOR value: `0xC1` (193)** applied per byte during in-place
  decryption
- **ADD value: `0x11` (17)** also applied per byte, alongside the XOR
- Apply to the **turbo data blocks** (TZX ID 0x11 or 0x14, flag byte
  in our catalogue is typically `$98`)

This means: TZX block bodies are stored *scrambled*. Standard TZX
playback delivers the scrambled bytes to the loader; the loader runs
its in-place XOR + ADD decryption to recover the original code. If
our TZX parser delivers the bytes verbatim, decryption works.
**Our parser does this correctly** — the SkoolKit decryption
documentation is just describing the per-byte transform the *loader's
runtime code* performs, not a TZX format detail.

## The R register dependency (load-bearing)

From [Muckypaws' Speedlock '87
analysis](https://muckypaws.com/2024/01/29/speedlock-1987/) of the
Amstrad CPC port (architecturally identical on Spectrum):

> The R register increments in value depending on instructions
> executed by the Z80 processor. It is predictable based on the op
> codes ... provided Interrupts are disabled.

Speedlock 4-7 uses **Z80's R register as the XOR seed** for an
in-place decryption pass. Each byte XORs with R, and R increments
predictably as the loader's instruction stream executes (every M1
fetch increments R; some DD/FD/CB/ED prefix variants have specific
two-step increments). Because the decryption seed depends on the
exact instruction sequence executed with IFF=0, **any single-cycle
R-increment difference between us and real hardware corrupts the
entire decrypted region**.

We pass Tom Harte 100% for single-instruction R behaviour, but Tom
Harte doesn't exercise:

- DD/FD prefix sequences (where each `0xDD` or `0xFD` is its own M1
  fetch that increments R, then the next prefix-modified opcode is
  another M1 fetch)
- DD/FD on undocumented IXH/IXL/IYH/IYL ops (Speedlock 1 uses these)
- IM 2 vector acceptance R behaviour (the interrupt service M1 reads
  a vector byte that does not increment R per the spec, but
  implementations vary)
- HALT phantom M1 cycles (each phantom fetch should increment R)

Our HALT fix (commit `81a4697`) does increment R on phantom cycles
via the regular M1 path — verified in
`halt_blocks_until_irq_then_irq_returns_past_halt`. The IM 2 case
hasn't been audited yet against Z80 reference behaviour.

**Critical vulnerability noted in Muckypaws**: the decrypt has *no
feedback* — each byte's transform depends only on R, not on the
previously-decoded byte. This means a cracker can compute the
required patch values to bypass with just a handful of POKEs. For
us, it means **R divergence shows up as a single decryption boundary
mismatch**: every byte after the first wrong R value is wrong.

## TZX format mapping

Speedlock games on tape use the TZX format (`.tzx`). The per-block
shape is captured in `format-sinclair-zx-spectrum-tzx`. Block IDs
relevant to Speedlock:

| ID | Name | Use in Speedlock games | Our handling |
|---|---|---|---|
| `0x10` | Standard Speed Data | Header block + initial BASIC loader stub | ✓ |
| `0x11` | Turbo Speed Data | Speedlock-encrypted body blocks (flag `$98`); explicit pilot/sync/zero/one/pilot-count/last-bits/pause/length | ✓ (timing carried per-block) |
| `0x12` | Pure Tone | Rare; alternative pilot specification | ✓ |
| `0x13` | Pulse Sequence | Up to 255 individual pulse lengths — used by some custom loaders | ✓ |
| `0x14` | Pure Data Block | Turbo block without pilot/sync — for in-block data sections | ✓ (partial-byte fix in `80ec856`) |
| `0x15` | Direct Recording | Sample-by-sample EAR state — for the most paranoid protections | ? worth verifying |
| `0x20` | Pause / Stop | Inter-block silence; stop-flag if pause = 0 | ✓ |
| `0x2A` | Stop tape if 48K mode | Used to halt playback when game detects 128K | ? worth verifying |

**TAP format does not carry per-block timing** — only block bodies
with implicit ROM-speed timing. Using a TAP file for a Speedlock-7
game (as our catalogue did until 2026-05-17 for the five silent
titles) corrupts the timing of every non-ROM-speed block. Migration
to TZX paths is the manifest-side fix.

**FUSE has no Speedlock-specific code path** — verified by
`grep -i speedlock` across `fuse-emulator-libspectrum`. FUSE just
plays the TZX timing values faithfully. So a game working in FUSE
and not for us means our Z80 / ULA / tape transport differs from
real hardware in some way independent of any Speedlock-awareness in
our codebase.

## Tooling

- **[SkoolKit](https://skoolkit.ca/)** ([GitHub](https://github.com/skoolkid/skoolkit)) —
  Python toolkit for ZX Spectrum game disassembly + tape decoding.
  Critical for us: `tap2sna.py` accepts TZX input and bakes the
  game into a `.sna` snapshot, running the loader through an
  internal Z80 emulator. With `--tape-analyse` and Speedlock flags
  it can decrypt Speedlock 4-7 blocks. **The output snapshot is a
  known-good post-load RAM image** we can byte-diff against ours to
  pin where our emulator diverges.
- **[Spectrum Analyser /
  Taper](https://worldofspectrum.net/legacy-info/tape-decoding-using-taper/)** —
  TZX block-level analysis; identifies Speedlock variant from block
  structure
- **[Craig's Retro Computing](https://craigsretrocomputingpage.eu5.org)** —
  BASIC-program "hacker" scripts for Speedlock 3 / 4 that POKE
  through the protection; useful as ground truth for "what bytes
  does the loader expect after decryption"
- **CPCWiki Speedlock article** ([link, currently 403 from
  us](https://www.cpcwiki.eu/index.php/Speedlock)) — historically the
  most thorough technical reference; worth retrying via Wayback
  Machine on the next investigation

## Affected catalogue entries

20 entries (5 games × 4 variants: 128k, +2, +2A, +2B), plus three
+3-disk siblings that load fine via DSK (Speedlock 7+ on the +3 disk
path is closed). Migrated from TAP → TZX paths in the manifest on
2026-05-17. Post-migration states:

| Game | Speedlock | Post-TZX state | Audio |
|---|---|---|---|
| Rainbow Islands | 7 | Proper attract screen rendered | Mostly silent (2 sample values) |
| Bubble Bobble | 5 | Controls-select menu (needs keypress) | Silent |
| Out Run | 4 | OPTION/TRAFFIC/CONTROLS menu | Audible (different hashes per variant pair) |
| RoboCop | 7 | "SORRY LOAD ERROR" message | Silent |
| Operation Wolf | 7 | "PROGRAMMING BY ANDREW DEAKIN" credits | Silent |

## Next investigation moves (highest-leverage first)

1. **SkoolKit `tap2sna.py` diff**: install SkoolKit, run Rainbow
   Islands' TZX through with Speedlock-7 decryption, dump the resulting
   snapshot's RAM, then diff against our `EMU198X_DUMP_MEM` output at
   the same waypoint. The first differing byte pins our divergence
   point. This is the single highest-leverage move because it
   converts "we're somewhere wrong" into "we're wrong at address X".

2. **R-register multi-instruction conformance test**: extend our Z80
   test surface with multi-instruction R-tracking fixtures covering
   DD/FD prefix sequences (including chains like `DD DD CB` and
   `FD DD ED`) and IM 2 vector acceptance. Tom Harte covers single
   instructions; Speedlock's decryption spans hundreds of
   instructions and is unforgiving of any single R divergence.

3. **Speedlock 7 layer-by-layer trace** in the spirit of the Green
   Beret thread. The existing
   `crates/runtime-sinclair-zx-spectrum/tests/speedlock7_tape_ram_dump.rs`
   harness needs ~50 lines of additions to track call chains in
   Rainbow Islands / RoboCop the same way it tracks Green Beret.

4. **CPCWiki via Wayback Machine** for the missing technical
   reference, in case our hypotheses miss a documented variant.

## Sources

- [Sinclair Wiki — Loading routine "cores"](https://sinclair.wiki.zxnet.co.uk/knowledge/Loading_routine_%22cores%22)
- [Alessandro Grussu — ZX Spectrum Loading Schemes (Speedlock 1-8 catalogue)](https://www.alessandrogrussu.it/loading/Schemes/schemes.html)
- [Wikipedia — Fast loader (Speedlock section)](https://en.wikipedia.org/knowledge/Speedlock)
- [Craig's Retro Computing — Commercial Protection Systems (Type 1/2/3 deep dive)](https://craigsretrocomputingpage.eu5.org/howtohack/commercialprotectionsys.html)
- [Craig's Retro Computing — Speedlock III Hacker](https://craigsretrocomputingpage.eu5.org/smashtips/games/speedlock3.html)
- [Craig's Retro Computing — Speedlock 4 Hacker](https://craigsretrocomputingpage.eu5.org/smashtips/games/speedlock4.html)
- [Muckypaws — Speedlock 1987 reverse engineering (CPC; R-register pattern is identical to Spectrum)](https://muckypaws.com/2024/01/29/speedlock-1987/)
- [TZX format technical specification (Toysoft / spectnetide mirror)](https://github.com/Toysoft/spectnetide/blob/master/Core/Spect.Net.SpectrumEmu/_Documents/TZX%20technical%20specifications.html)
- [Claus Jahn — TZX format reference](https://worldofspectrum.net/zx-modules/fileformats/tzxformat.html)
- [SkoolKit — supports Speedlock XOR+ADD decryption via tap2sna.py](https://skoolkit.ca/)
- [SkoolKit on GitHub](https://github.com/skoolkid/skoolkit)
- [CPCWiki Speedlock page (currently 403; try Wayback)](https://www.cpcwiki.eu/index.php/Speedlock)
- [World of Spectrum — Taper documentation](https://worldofspectrum.net/legacy-info/tape-decoding-using-taper/)

## Log

| Date | Event |
|---|---|
| 2026-05-17 | Reference compiled while investigating the silent-music cluster. Cross-referenced version landscape across Grussu's catalogue, Craig's Retro Computing, Wikipedia, Muckypaws (CPC port). Pinned **Speedlock 4-7 XOR=0xC1, ADD=0x11 with R register as seed** as the most-likely-load-bearing protection feature; pinned **R-register multi-instruction accuracy** as the most-likely divergence point. Listed next experiments in priority order. |

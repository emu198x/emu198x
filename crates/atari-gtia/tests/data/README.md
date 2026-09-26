# GTIA priority signal oracle

`priority-signals.txt` records 8,192 output masks from the vendored Altirra
`ATInitGTIAPriorityTables` function in `src/Altirra/source/gtiatables.cpp`.
Source SHA-256: `7a890f1e55c9a83d5836140670a6d3d9af2f99b6b5290646463f3f4db1d07306`.

Each row selects PRIOR bits 0-3 and multicolour bit 5 (packed as table bit 4).
Each column selects PF0-3 in bits 0-3 and P0-3 in bits 4-7. Each hexadecimal
output mask identifies enabled colour registers in those same bits, with bit 8
for background; zero means black. Fifth-player missiles must be routed into PF3
before lookup. Incompatible PF0-2 inputs retain Altirra's own input decoding.

Regenerate from the family workspace with:

```sh
python3 scripts/generate-gtia-priority-oracle.py
```

The generator compiles the pinned reference function in a temporary directory,
substituting the output signal mask for its final colour-table index. It changes
no priority equations and ships no copied C++ implementation. The committed text
is numeric output, so ordinary Rust tests need neither Altirra nor a compiler.

The Rust regression exercises 163,840 colour comparisons: every PRIOR 0-63,
every raw player/missile mask, background or one of four playfields, and two
palettes. One palette gives all eight registers separate bits; the other includes
zero registers. This checks composition/routing against the reference, not DMA,
analogue output, or independent hardware measurements.

The underlying logic is documented by Avery Lee in
[Reverse engineering Atari 8-bit video](https://virtualdub.org/blog2/entry_243.html).
Altirra and that explanation are the same witness, not two independent sources.

## Scope and source disagreement

The article's prose example for PRIOR `$18` says the fifth player can appear
above a real player when PF0/1 is underneath. Its equations and the pinned
executable table instead select PF0 for P0 + fifth player + PF0. The regression
`player_zero_inhibits_fifth_player_even_when_playfield_hides_it` records the
executable result; hardware measurement would be needed to adjudicate that
prose discrepancy independently.

The rendering checks cover fifth-player suppression of playfield, unchanged
collision inputs, and mode 9/11 nibble gating by input player signals. Hires
background is routed through PF2 priority, but this does not validate the full
hires luminance-substitution or paired-pixel collision circuitry. Neither those
remaining paths nor mid-line GTIA-mode transitions are certified by this table.

## Hires and register follow-up

The separate hires regressions check late PF1-luminance substitution over a
player, playfield and black conflict, plus paired-bit PF2 collisions for players
and missiles in modes 2/3/F on PAL and NTSC. These expectations follow Altirra
`RenderMode8` and `SpriteState::Detect`, and the article's 40-column section.
They do not come from the priority table.

Register regressions follow the vendored `gtia.cpp` `WriteByte`, `ReadByte` and
`ReadConsoleSwitches`: colour writes ignore bit 0, unused reads return `$0F`, and
asserted CONSOL output bits pull their inputs low. Mode 9 can still generate odd
luminance after palette lookup. Both snapshot loaders normalize legacy colour
bits; the compact loader rejects every truncated prefix before changing state.
The Postcard test preserves the serialized field layout.

Remaining timing questions are outside these static checks: register propagation
delays, mid-line mode-transition latches and sprite shift-register behaviour when
position/size/graphics change while shifting. They require beam-timed probes,
not further changes to the static selector table.

# Recipes for the Apple TV app's local content

`scripts/bootstrap-apple-local-content.py` reads a recipe here and fills
`apple/Emu198x/LocalContent` from your own collections. A recipe holds no
game software, only where to find it and how to play it.

`dizzy.json` is the Codemasters Dizzy games, on the Spectrum, C64, Amiga,
NES, Master System and Game Gear. For each game it lists:

- `tape`: the Spectrum versions' TOSEC zip, relative to the TOSEC set's
  root. A game with no Spectrum version has none.
- `cover`: the box art, if any; without it the game is a name card. `wos` is a path in the World of Spectrum archive;
  `url` and `sha256` pin a download instead. `left` keeps the part of a
  cassette inlay right of that fraction of its width, the front panel, and
  `crop` is a pixel rectangle `[x, y, width, height]`. With neither, the
  image is used whole.
- `versions`: as in a `<Game>.game.json` (see the LocalContent README),
  without `file`, which the script fills in. A version on another machine
  lists its own TOSEC zips as `media`, one image each, in order: a tape, a
  cartridge, or an Amiga's disks, the first for DF0. The script names
  them after the game and version.

The Amiga versions are cracked copies where no uncracked ADF exists, named
as such, until the Amiga core reads IPF images
(`docs/plans/2026-10-21-apple-tv-other-systems.md`).

The keys each version needs were found by driving each game headlessly
from its title; `docs/plans/2026-10-21-apple-tv-game-versions.md` and
`docs/plans/2026-10-21-apple-tv-other-systems.md` record how.

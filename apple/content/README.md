# Recipes for the Apple TV app's local content

`scripts/bootstrap-apple-local-content.py` reads a recipe here and fills
`apple/Emu198x/LocalContent` from your own collections. A recipe holds no
game software, only where to find it and how to play it.

`dizzy.json` is the twelve Codemasters Dizzy games. For each game it lists:

- `tape`: the TOSEC zip, relative to TOSEC's `Sinclair/ZX Spectrum`
  folder.
- `cover`: the box art. `wos` is a path in the World of Spectrum archive;
  `url` and `sha256` pin a download instead. `left` keeps the part of a
  cassette inlay right of that fraction of its width, the front panel, and
  `crop` is a pixel rectangle `[x, y, width, height]`. With neither, the
  image is used whole.
- `versions`: as in a `<Game>.game.json` (see the LocalContent README),
  without `file`, which the script fills in.

The keys each version needs were found by driving each game headlessly
from its title; `docs/plans/2026-10-21-apple-tv-game-versions.md` records
how.

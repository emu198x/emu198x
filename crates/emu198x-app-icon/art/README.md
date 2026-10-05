# Emu198x application icon

The Emu198x stacked tile: the Emu198x blue cell carrying `19` over the off-white `8x` cell, framed in house brown.

## Where the SVGs come from

They come from the 198x-ui kit's `wordmarks/generate.py` at commit `1413e02` (`stevehill1981/198x-ui`). This repo keeps the rendered files, not the generator.

| File here | Kit source |
|---|---|
| `emu198x-icon.svg` | `wordmarks/emu198x-stacked-light.svg` |
| `emu198x-icon-small.svg` | `wordmarks/emu198x-stacked-compact-light.svg` (used for 16 and 24px) |
| `emu198x-icon-macos.svg` | `stacked("emu", PROJECTS["emu"], THEMES["light"], margin=0.098)`, which puts the tile on Apple's 824-of-1024 icon grid |

## Where the rendered files go

| Output | Used by |
|---|---|
| `../emu198x.ico` (16 to 256px) | Windows `.exe` file icon, embedded by `../src/lib.rs` from each emulator's `build.rs` |
| `../emu198x.png` (256px) | shipped in the release archives, for a Linux `.desktop` entry |
| `../../emu198x-ui/icon/emu198x-256.png` | the window icon on Linux and the taskbar icon on Windows |
| `../../emu198x-ui/icon/emu198x-small-32.png` | the title-bar icon on Windows |
| `../../emu198x-ui/icon/emu198x-macos-512.png` | the macOS Dock tile |

## Regenerating

1. In a 198x-ui checkout, run `python3 wordmarks/generate.py`, then copy the two `emu198x-stacked*-light.svg` files over the ones here.
2. From `wordmarks/`, write the macOS variant:

   ```sh
   python3 -c 'import generate as g; print(g.stacked("emu", g.PROJECTS["emu"], g.THEMES["light"], margin=0.098), end="")' \
     > <emu198x>/crates/emu198x-app-icon/art/emu198x-icon-macos.svg
   ```

3. From this repo's root, run `python3 crates/emu198x-app-icon/art/rasterise.py`. It needs `rsvg-convert`, and uses `oxipng` if it is installed.
4. Update the kit commit in this README.

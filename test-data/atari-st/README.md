# Atari ST validation inputs

`board-split-v1/` contains project-written firmware's original native states
and output. These are fixture-free regression data and contain no commercial
TOS image.

The existing external-firmware oracles use the 192 KB US image from the
[official EmuTOS 1.3 release](https://sourceforge.net/projects/emutos/files/emutos/1.3/).
Its `etos192us.img` SHA-256 is
`e0ce6c38c5106240249157737980ff075f1d95f7a70e2e150005bc40fcdce4fc`.
Set `EMU198X_EMUTOS_ROM` to a local copy, or keep it at
`~/.emu198x/test-data/emutos/etos192us.img`. The override does not change
production firmware selection. EmuTOS 1.4 is newer; 1.3 remains the existing
regression pin for this continuation.

Run the shared firmware gates with:

```sh
EMU198X_EMUTOS_ROM=/path/to/etos192us.img EMU198X_STRICT_FIXTURES=1 \
  cargo test -p common-atari-st --lib emutos_
EMU198X_EMUTOS_ROM=/path/to/etos192us.img EMU198X_STRICT_FIXTURES=1 \
  cargo test -p runtime-atari-st --test variants emutos_boots_through_the_runtime
```

The shared checks cover colour/monochrome boot, the line-A/mouse path and
floppy boot via DMA. Per-owner tests use synthetic firmware; model-specific
headless captures are separate evidence. No firmware image is included here.

# GTIA mode probe (Atari 800XL)

`atari-800xl-gtia-modes.bin` is an 8 KB cartridge this project wrote, built
from [`atari-800xl-gtia-modes.s`](atari-800xl-gtia-modes.s) by
[`build.py`](build.py). It carries no code from any commercial image, so it
sits in the repository and runs in CI with no ROM at all: the 800XL starts a
cartridge directly when it has no OS.

## What it is for

GTIA's three extra modes (PRIOR bits 6-7) turn an ANTIC mode F line into 16
luminances of one hue, 16 hues of one luminance, or a choice among nine
colour registers, by pairing the four two-clock pixels ANTIC sends it into
one nibble. The GTIA crate's own tests feed it hand-made pixel streams; this
cartridge makes ANTIC produce the stream, so the nibble pairing, the
one-clock delay of mode 10 and the way each mode colours the border are
checked on a picture the real chips would draw. The test that drives it is
[`crates/machine-atari-800xl/tests/gtia_modes.rs`](../../../crates/machine-atari-800xl/tests/gtia_modes.rs).

## How a test uses it

The two registers under test are read from zero page when the program
starts, so one image covers every mode and background colour:

| Address | Register |
|---|---|
| `$80` | PRIOR |
| `$81` | COLBK |

The screen is 16 mode F lines whose bytes repeat `$01 $23 $45 $67 $89 $AB
$CD $EF`, so pixel `p` of each line carries nibble `p mod 16` and every
value of every register appears in order across the line. COLPM0-3 and
COLPF0-3 hold distinct colours the test knows, so mode 10 can be checked
against the register each nibble selects. A test pokes PRIOR and COLBK,
runs three frames, and compares one row of the picture, border included,
against what the mode should produce.

## Regenerating

```sh
python3 test-data/atari/gtia-modes/build.py          # write the image
python3 test-data/atari/gtia-modes/build.py --check  # CI: compare with the source
```

Needs `asm198x` on the path. Deterministic; regenerating should produce no
diff.

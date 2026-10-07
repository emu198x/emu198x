# Running the VIC-20 VIC-I survey against VICE

This process answers how closely Emu198x's VIC-I output — the PAL 6561 and
the NTSC 6560 — agrees with VICE xvic, frame for frame, on a fixed set of
programs, at one identifiable Emu198x revision.

It is a software comparison. VICE is the reference emulator for the VIC-20
(RULES.md, rule 32), not the specification. Where VICE contradicts the 6560
data sheet or the *VIC-20 Programmer's Reference Guide* (the data sheet is
reprinted there, pp. 211-217), the primary source wins and the disagreement is
recorded, not tuned away. A matched pixel count is a measurement of agreement
with VICE, not a hardware-conformance claim.

## Inputs

| Input | Where | Pinned by |
|---|---|---|
| Case list, program and reference SHA-256 | [`cases-v1.json`](../../test-data/commodore/vic-20/vici-vice-survey/cases-v1.json) | tracked |
| Four raster programs written for the survey, each in a PAL and an NTSC build | [`programs/`](../../test-data/commodore/vic-20/vici-vice-survey/programs/) | tracked; `build.py --check` |
| VICE's VIC-20 test programs, SVN revision 46281 | `<fixture>/vice-testprogs/` | SHA-256 per program |
| VICE reference captures and palette calibration | `<fixture>/references/{pal,ntsc}/` | SHA-256 per PNG |
| `kernal.rom` (901486-07, PAL), `kernal-ntsc.rom` (901486-06), `basic.rom`, `char.rom` | the VIC-20 ROM directory | SHA-256 per ROM |

`<fixture>` is `EMU198X_VIC20_VICE_SURVEY_DIR`, defaulting to
`~/.emu198x/test-suites/vic20`; the ROM directory is `EMU198X_VIC20_ROM_DIR`,
defaulting to `~/.emu198x/roms/commodore-vic-20`. The ROMs are the official
images and byte-identical to the ones VICE 3.10 ships for xvic, so both
emulators boot the same firmware.

To stage VICE's test programs, export the five directories the manifest lists
from `https://svn.code.sf.net/p/vice-emu/code/testprogs/VIC20/` at revision
46281 into `<fixture>/vice-testprogs/`, for example
`svn export -r 46281 <url>/vic6561 <fixture>/vice-testprogs/vic6561`. The
survey uses 17 of their programs; the manifest's hashes confirm the bytes.

## Capturing VICE's frames

```sh
scripts/capture-vic20-vici-vice-references.py            # capture and check
scripts/capture-vic20-vici-vice-references.py --update   # capture and re-pin
```

The script needs `xvic` on the path (Homebrew `vice` 3.10 was used). For each
case it runs:

```sh
xvic -config <empty file> -default -console -silent -sounddev dummy \
  -VICborders 2 -VICfilter 0 -warp +autostart-delay-random \
  -model vic20pal -memory none \
  -limitcycles <capture_frame x 22152> -exitscreenshot <case>.png \
  -moncommands start.mon -nativemonitor
```

with `-model vic20ntsc` and 16,965 cycles a frame (65 x 261) for an NTSC
case, and `-memory 8k` where a case needs the expansion.

**How a frame is timed.** Both emulators start from power-on. The program
goes in at the first execution of `$E5EA`, the KERNAL editor's wait for a
key: `start.mon` sets a checkpoint there whose command plays back a second
file that deletes the checkpoint, `load`s the program at its own address, sets
BASIC's pointers at `$2D`-`$32` past it, and queues `RUN` + RETURN in the
keyboard buffer (`$0277`, count at `$C6`), followed by any keys the case lists
for the program to read. Both emulators reach `$E5EA` at the
same cycle (VICE's stopwatch reads 557,329; Emu198x's clock 557,330, the
difference being where each counts the reset sequence from). The frame is
then taken at an exact cycle from power-on: `-limitcycles` stops VICE at
`capture_frame` x the frame length and `-exitscreenshot` writes the frame it
has drawn;
the Rust survey runs the same number of cycles and stops at the same frame
boundary. VICE's autostart is not used, because its timing is VICE's own
and, by default, randomised; `+autostart-delay-random` is passed anyway.
The empty `-config` file keeps any user `vicerc` out.

`-VICborders 2` makes VICE draw the whole raster with each VIC-I pixel two
host pixels wide. On PAL the PNG is 568 x 312 and its pixel `(2x, y)` is
raster pixel `x` of line `y`; on NTSC it is 520 x 261 and starts at line 1, so
pixel `(2x, y)` is raster pixel `x` of line `y + 1`. Emu198x's framebuffer is
the window a set displays (`mos_vic_i::window_first_pixel` and
`window_first_line`): 230 x 288 from raster pixel 20 of line 24 on PAL, and
214 x 240 from pixel 4 of line 21 on NTSC. The survey compares that whole
window. The boot screens match VICE pixel for pixel at these mappings, which
is the check that they are right.

**Colours.** VICE renders its palette through its own colour adjustments, so
RGB values are never compared. The script captures sixteen calibration frames
per standard — a program that sets background colour *c* and border colour
*c & 7* and spins — and the survey reads VICE's rendering of each colour from
them. Every pixel of a reference is then classified by exact RGB match, and
Emu198x's pixels by exact match against `mos_vic_i::VIC_PALETTE`; the two
colour numbers are compared.

The script checks every capture against the manifest's SHA-256 and fails on
any difference. Two runs on 2026-10-07 produced identical bytes.

## Running the survey

```sh
cargo test -p machine-commodore-vic-20 --test vici_vice_survey -- --include-ignored --nocapture
```

`vic_i_matches_the_vice_survey` runs every case (in parallel), prints a table
of matched pixels per case with the raster lines that disagree, and asserts
the exact count for every case against `EXPECTED_MATCHES`. Any change, better
or worse, fails until the table is updated; update it in the same commit as
the change that moved it, and update the classification below.

`the_comparator_rejects_a_deliberately_wrong_frame` is the check that the
comparison can fail: the boot screen matches VICE exactly, and the same frame
with the border colour changed to 2 at the start of the capture frame keeps
exactly the 176 x 184 display and loses every border pixel. Two further tests,
which need no fixtures and run everywhere, check the comparator on synthetic
frames and that it refuses a screenshot of the wrong size, undoubled pixels,
and any colour outside either palette.

`dump_a_survey_case` writes Emu198x's frame, VICE's (in Emu198x's palette)
and a difference image, disagreements in magenta, for one case:

```sh
VIC20_SURVEY_CASE=raster-border VIC20_SURVEY_OUT=/tmp \
  cargo test -p machine-commodore-vic-20 --test vici_vice_survey -- --ignored dump_a_survey_case
```

## The cases

- `basic-boot`, `basic-boot-cursor`, `ntsc-basic-boot`: the power-on BASIC
  screen at frames 150 and 170, the second with the cursor lit. These also
  check that the 6502, the VIAs and the KERNAL's jiffy interrupt keep VICE's
  time to the frame.
- `vic6561-*`: VICE's `vic6561` programs. Each prints what a real VIC-20
  should show and then changes a VIC-I register at a VIA-timed point in the
  frame. Only each program's first screen is compared.
- `vic-9000test`: tokra's `$9000` split, needing the 8K expansion. Its
  `references/` directory holds photographs of a real VIC-20.
- `vic-vert0`, `ntsc-vic-vert0`: what the VIC-I fetches on the lines below
  the text area.
- `ntsc-vic-line0`: an NTSC test, from a sleepingelephant.com forum post, of
  when `$9004` reports line 0, with `N` queued to answer its interlace
  question.
- `split-timing`: tlr's `split-tests/timing`, which reads `$9003`, `$9004`
  and the open bus at every cycle of a line and keeps the results at
  `$17C0`-`$1BFF`; its `dumps/` hold the same results from four real
  machines. The survey compares the final screen.
- `raster-border`, `raster-background`, `raster-reverse`,
  `raster-auxiliary`, and their `ntsc-` builds: this project's programs. Each
  locks to the raster by reading `$9003` every line length plus one cycle
  until two reads see the same line, which happens only when the second read
  is the first cycle, as the CPU sees it, of a new line; no VIA timer is
  involved, so the programs measure the VIC-I alone. Each then writes a
  register on and, 20 cycles later, off again in a loop one cycle longer than
  a line, so over a line's worth of lines the band visits every cycle of a
  line once.

## Interpreting the results

The table the survey prints, with what each shortfall is; a row that a fix
moved says what it was:

| Case | Matched of 66,240 (PAL) or 51,360 (NTSC) | Where it disagrees, and why |
|---|---:|---|
| basic-boot, basic-boot-cursor, vic6561-test36867-1, vic-vert0, ntsc-basic-boot, ntsc-vic-vert0 | 100% | — |
| vic6561-test36867-2 | 100% | Was 65.990%: Emu198x showed all 23 rows. The program sets 7 rows only around line 0 and prints that "only 7 lines should be displayed"; the chip now reads the row count once, in cycle 2 of line 0, as VICE does. Cycle 1 shows this program all 23 rows and cycle 3 shows test36867-1 only 7, so the survey pins the cycle. |
| vic6561-test36866-1 | 100% | Was 99.227%: `$9002` written mid-line cut the first two rows short, where the program says each line keeps 22 columns. The chip now reads the column count once a line, in cycle 1, as VICE does. Reading it at cycle 0 cuts this program's rows short again, and at cycle 2 loses 716 pixels of test36866-2, so the survey pins the cycle. |
| vic6561-test36866-2, testcharheigh-2 | 95.494%, 95.823% | Column count and character height changed mid-frame: VICE advances its screen pointer row by row by the columns actually fetched; Emu198x recomputes each row's address from the live registers (#1644). |
| vic6561-test36864, test36865-1/2/3, testmemfetch-1/2, testcharheigh-1, vic-9000test | 99.2-99.98% | Origin, height and fetch registers written mid-line: VICE opens the display when the cycle counter equals `$9000` and keeps it open for the line; Emu198x re-reads every register for every pixel (#1644). |
| raster-border, raster-background, raster-reverse, raster-auxiliary | 95.3-99.6% | Colour-register writes: VICE shows a `$900F` or `$900E` write made in cycle *n* from pixel 4(*n*-7)+1 (reverse mode from 4(*n*-7)+3); Emu198x applies it to the very next pixels, 31 pixels to the right for a write in the same cycle. Every band edge is 27 pixels (reverse, 25) right of VICE's, not 31, because Emu198x's CPU sees each new line one cycle sooner than VICE's: traced in both emulators' monitors, the raster lock lands one cycle earlier here on PAL and NTSC alike, and the band with it (#1644). |
| vic6561-testback | 98.256% | The same colour-register delay in a VIA-timed program: its stripes sit 27 pixels right of VICE's, as the VIA-free raster programs' bands do. Was 97.348%, 23 pixels, while the VIA's timer 1 ran a cycle early (#1642). |
| split-timing | 100% | Was 43.034%: the VIA-timed stable raster did not settle, because a timer 1 read a fixed time after a T1C-H write returned one count less than VICE's, and the program measured 72 cycles a line instead of 71 (#1642). It now reaches its final screen, and its results at `$17C0`-`$1BFF` match both PAL hardware dumps in `dumps/` byte for byte in the header and the `$9003` and `$9004` columns. The `$9100` and `$9200` columns read the open bus: hardware returns the last byte on the bus, Emu198x `$FF`. |
| ntsc-raster-border, ntsc-raster-background, ntsc-raster-reverse, ntsc-raster-auxiliary | 94.3-99.4% | The colour-register delay as on PAL: every band edge is 27 pixels right of VICE's. Were 82.8-98.2%: the 6560 reports a new line 37 cycles before it draws it (see below), and Emu198x reported the line it drew, so its band started 148 pixels (37 cycles) further right again (#1643). |
| ntsc-vic-line0 | 99.938% | The colour-register delay alone, at its full 31 pixels: the program polls `$9004` every 7 cycles, and both emulators catch the change on the same read, so the one-cycle lock difference above does not hide 4 of them. Was 99.965% before #1643, when the 37-cycle error and the missing line-0 delay happened to land the 16-pixel mark partly on VICE's. |

The colour-register delay and the display-opening behaviour are one
mechanism in VICE: the chip fetches a character two cycles at a time several
cycles before it shows it, and the colour registers act at the output. The
data sheet and the Programmer's Reference Guide describe the registers but
give no cycle timing, so for these the evidence is VICE, the expectations the
`vic6561` programs print, and the hardware photographs and dumps where they
exist. #1644 is the work to model it.

**The 6560's raster phase (#1643).** On the 6560 registers 3 and 4 report
a new line 37 cycles before the chip draws it, and report line 261 for the
first 33 cycles of line 0. Two hardware sources support this, besides VICE's
`VIC20_NTSC_CYCLE_OFFSET`:

- tlr's `split-tests/timing` dumps read `$9003` and the open bus, which holds
  the VIC-I's last fetch, at every cycle of a line. On both 6561 dumps bit 7
  changes 16 cycles before the row's first character fetch, with `$9000` at
  12; on both 6560 dumps, 46 cycles before it, with `$9000` at 5. Register 0
  moves the fetch and the picture together, so taking it out leaves 4 cycles
  on the 6561 and 41 on the 6560: the 6560's line changes 37 cycles earlier
  relative to its own picture. This assumes the two chips take equally long
  from the origin match to the first fetch.
- tokra's photographs in `vic_line0` show a write whole lines and 48 cycles
  after `$9004` first shows a new line landing at the display's left edge for
  every value but 0, and 33 cycles further right for 0.

Why the 6560 steps its line counter there is not known; the data sheet gives
no timing. What register 3's bit 7 shows during the 33 cycles is VICE's
choice (line 261, so set), not a measurement. Emu198x does not model the
6560's interlace mode, in which the same test found the delay on one field
only.

Where the primary sources and VICE disagree:

- The Programmer's Reference Guide says "numbers over 27 will give a 27 column
  screen" (p. 214). VICE caps the column count at 32 on PAL and 31 on NTSC;
  Emu198x does not cap it. No survey case reaches it yet, and nothing here
  says which is right.

## Related documents

- [Survey inputs](../../test-data/commodore/vic-20/vici-vice-survey/README.md)
- [C64 VIC-II survey](c64-vicii-vice-survey.md), the pattern this follows
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [The framebuffer is the set's window](../decisions/the-framebuffer-is-the-sets-window.md)

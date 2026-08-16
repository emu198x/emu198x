# Spectron reference screens

Expected-output screenshots from [Spectron](https://github.com/oldbit-com/Spectron),
a cross-platform ZX Spectrum emulator, used as the external oracle for this
project's ULA / contention smoke tests (#10).

Checked in rather than mirrored, unlike the rest of
[`../../accuracy-corpora.md`](../../accuracy-corpora.md). Two reasons:

- **Licence permits it.** These screens are MIT (see [`LICENSE`](LICENSE)),
  which the corpora manifest already recorded. The *tapes* that produce them —
  RAMSOFT's floatspy, Woody's Float48k/Float128k — are long-circulated freeware
  not covered by that licence, and stay in the private store.
- **A comparator nobody can run is not a comparator.** These were absent from
  every developer machine, and `assert_screen_matches_spectron` skipped when it
  could not find them — which `emu198x-test-skip` reports as `ok`. The 48K
  floating-bus regression of 2026-08-11 (#939) went unreproduced locally for
  five days because of it, while the nightly, which does have them, failed every
  night. 116 KB is a cheap price for a local run that means what CI's means.

## Provenance

| | |
|---|---|
| Upstream | `github.com/oldbit-com/Spectron`, `tests/Results/` |
| Revision | `6e814e69ff0b7c8759de19001456010f09501a3d` (2026-08-01) |
| Licence | MIT, © 2026 Wojciech Sobieszek (OldBit) — full text in [`LICENSE`](LICENSE) |
| Integrity | [`SHA256SUMS`](SHA256SUMS) — `shasum -a 256 -c SHA256SUMS` |

Every PNG is a clean 4× nearest-neighbour scale of a bordered Spectrum frame;
the comparator asserts that scale before undoing it, so a re-render at a
different scale fails loudly rather than comparing blurred pixels.

## What is here, and what uses it

| Screen | Consumed by |
|---|---|
| `btime_48.png` | `machine-sinclair-zx-spectrum-48k` · `tape_smoke::btime_runs_to_completion` |
| `floatspy_48.png` | `machine-sinclair-zx-spectrum-48k` · `tape_smoke::floatspy_selftest_ok` |
| `halt2int_48.png` | `machine-sinclair-zx-spectrum-48k` · `tape_smoke::halt2int_runs_to_completion` |
| `btime_128.png`, `floatspy_128.png`, `halt2int_129.png`, `ptime_128.png` | not yet wired — the 128K half of #10 |
| `eihalt_49.png`, `eihalt_129.png` | no corresponding smoke in this workspace |

There is no `ptime_48.png` upstream, so `ptime`'s 48K smoke has no reference to
be held to and compares against its self-locked golden only.

## Overriding

`EMU198X_SPECTRON_RESULTS_DIR` still wins when set, so the nightly keeps pulling
from its own provisioned bundle. Unset — the developer default — the tests read
this directory.

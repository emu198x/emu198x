# First DMA loop request after multiword startup

An idle channel starts with LEN 1, 2, 8 or 64 and PER 124 or 424. Requested
words arrive at CCK 14 + 227*n; each visible IRQ is cleared immediately.
The producer executes the unmodified methods already retained by the interrupt
and handover corpora, with a new explicit grant schedule. WinUAE and vAmiga
agree on all 32 request times in `reference.csv`. Input hashes are pinned in
`inputs.json`; the parent corpora retain original copyright and source hashes.
WinUAE revision is c32694e338fa5f34977f522eb4898adb069d2e73; vAmiga revision
is 60fd1e6b69dcd77c9f44d1291bd37ec715362ab0.

Before the correction, LEN=8/PER=424 native IRQs were 15, 7025, 13809, 20593;
both references give 15, 6178, 12962, 19746. The held loop condition was latched on the fetch that reloaded the pointer,
one word after the last word arrived. Subsequent loop intervals already
agreed. LEN=2 and 64 show the same phase defect; LEN=1 does not. The fix
latches completion on the last word, preserving the full first buffer and
all later pointer reloads.
Commodore-Amiga's *Amiga Hardware Reference Manual*, third edition, pp.164–166,
describes the length counter, last-word interrupt and the startup state machine.

`audio_startup_length` checks every channel, direct/admitted path parity,
two complete word-address sequences including zero/one length boundaries,
the first loop and exact subsequent cadence. It explicitly
preserves the existing one-CCK native output-phase difference: the native output
phase decrements the newly loaded counter on the DAT-arrival clock; reference
event schedulers begin that countdown on the next clock. This change does not
claim to settle that phase difference. These are component observations, not
full-board timing or original-hardware captures.

```sh
python3 test-data/commodore/amiga/paula-audio/startup-length-probe/reference.py \
  --output /tmp/paula-startup-length
cargo test --locked --release -p emu198x-commodore-paula-8364 \
  --test audio_startup_length
```

Generated reference executables remain separate from Emu198x. Extracted code
retains the upstream licences and is never linked into the native core.

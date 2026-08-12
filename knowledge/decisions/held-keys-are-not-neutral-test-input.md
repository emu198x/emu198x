# A held key is not neutral test input

**Date:** 2026-08-12
**Status:** BINDING for harness and triage work
**Arose from:** emu198x/emu198x#872, closed not-a-bug after three rounds of
measurement against a defect that did not exist.

## The rule

Before reporting that guest software hangs, establish what the guest's own
input semantics are. Specifically:

1. **Never conclude "frozen" from a window in which a key was held.** Take
   the idle control first — same start state, no input, same number of
   frames. If the idle control animates, the hang belongs to the input, not
   the emulator.
2. **Compare frames pairwise, not against a baseline.** "Every frame differs
   from frame 0" and "every frame differs from the one before it" are
   different claims, and only the second one means the guest is running.
3. **Identify the game's controls before driving it.** Read them out of the
   guest's own key table if there is one; do not sweep the keyboard and
   assume every key is inert.

## Why — what #872 actually was

*Dizzy* was reported to hang after loading, with "zero pixels change" across
160-frame holds of each direction and 16,000 frames on the other container.
The Z80 was said to have stopped.

It had not. Idle, the game produces **24 distinct frames out of 24** across
2,400 frames. Every "pixel-identical" measurement had held a direction down,
and **P is the pause key**. Pressing P once pauses, pressing it again
resumes — measured as 3-of-3 distinct frames running, 1-of-4 paused, 4-of-4
running again.

The guest says so plainly once you look at it. While "frozen" it sits at
`$E577` inside its key-test routine with `hl = $E5EA` — the entry in its own
`(port_high, mask)` table for key code `$1B`, which is `DF 01`: port `$DFFE`,
mask `$01`, **P**. The caller is

```
D32D: CD 5D E5   CALL $E55D
D330: 20 F9      JR NZ,-7      ; loop WHILE still pressed
```

a wait-for-release, which is what a pause routine does. Holding P forever
pauses forever, and the measured signature — full-rate execution, all writes
confined to four stack addresses, no screen writes — is exactly a correctly
emulated wait loop.

## The two traps that made it durable

**A key sweep can change the input mode mid-sweep.** The title screen reads
"JUMP TO START OR K FOR KEMPSTON". Pressing `K` switches the game to joystick
input, so every keyboard test after that point in the sweep measured a game
that had been told to ignore the keyboard.

**Baseline-only comparison hides a stuck screen.** Comparing each swept key's
frame against the pre-sweep baseline showed all 38 as "changed", which read as
"no key had any effect". Comparing them against each other showed seventeen
consecutive keys producing one identical screen, with the state changing
exactly at `K`. The second comparison finds the Kempston switch on the first
pass; the first never does.

## What to do instead

Drive real software the way a player does — press and release — and reserve
indefinite holds for when the emulator's key latching is itself the thing
under test. For *Dizzy* specifically the scheme is **Z left, X right, Space
jump, P pause, K Kempston**, verified by watching Dizzy walk and jump.

None of the emulator's behaviour was at fault: loading, interrupts, the
keyboard matrix and the `IN` decode were all correct throughout.

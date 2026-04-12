# Signal Part 3 (acid test)

1992 Mikropol demo, originally a Your Sinclair covertape. Labelled "48K/128K". This was the acid test that drove the [fresh start](../../decisions/fresh-start-rationale.md) and validated the final architecture.

## What it does

Uses IM 2 with `I=$EF`, vector table filled with `$FF` → handler at `$FFFF` → `JR $FFF4` → `JP $D6E0`. The ISR at `$D6E0` is a music driver (`PUSH all regs`, `CALL $DC6B`, `CALL $DF4C`).

## The AY discovery

The music uses the [AY-3-8912](../../chips/gi-ay-3-8912.md) chip (ports `$FFFD`/`$BFFD`), **not** the beeper.

The ISR probes for the AY via register read-back:
```
OUT (C), A    ; write register
IN A, (C)     ; read back
AND $0F       ; mask
RET Z         ; if zero, AY absent — skip music
```

This means:
- Runs in 48K BASIC mode but requires a machine with AY hardware (128K/+2/+2A/+3)
- FUSE in "48K" mode fails (no AY). FUSE in "Spectrum 128" mode works
- The demo's "48K" label means "uses 48K memory only", not "runs on 48K hardware"

## What was wrong in earlier analysis

Earlier notes about "RET M in graphics data" and "contention-dependent flag state" were **wrong**. The ISR is a proper music driver, not graphics data. The confusion came from misreading the IM 2 vector chain.

## Status

**Working.** AY chip added, music plays, VU meters pulse. Resolved April 2026.

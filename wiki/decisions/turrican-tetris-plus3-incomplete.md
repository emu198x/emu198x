# Turrican and Tetris (+3) — black-screen loaders

**Status:** Known limitations as of 2026-05-12. Both titles' +3 disk loaders run without crashing, but neither reaches a playable state. They share the symptom (black framebuffer after 2 000 frames) but the cause is different per title. Diagnosis is paused after a 200-frame FDC trace per title; deeper work needs the same side-by-side-against-FUSE approach that closed the Speedlock-6 cluster (`marginal-encoding-model.md`).

## Turrican (1990, Rainbow Arts, +3) — sector-scan probe

The FDC trace shows the loader running a methodical sector-scan instead of consuming data:

1. Standard boot reads on track 0 (sector 1, then sector 2..9 via multi-sector `ReadData`).
2. `SeekTrack 2`, `ReadID` returns `C=2 H=0 R=1 N=6` — `N=6` means **8 192-byte sectors**, an unusual size code.
3. `Recalibrate`, then more `SeekTrack` / `ReadID` cycles across tracks 0, 1, 2 with no `ReadData` between them.
4. Multi-sector `ReadData(R=2, EOT=9, N=2)` once on track 0 — delivers 8 sectors successfully.
5. Resumes `SeekTrack` / `ReadID` polling indefinitely.

PC hot-spot is `$92xx` in loaded RAM, running a tight bit-banged loop:

```
$923f: ED 78        IN A,(C)         ; read MSR
$9241: 87           ADD A,A
$9242: 30 FB        JR NC,$923f       ; wait for RQM (bit 7)
$9244: 7E           LD A,(HL)
$9245: CB E0        SET 4,B
$9247: ED 79        OUT (C),A
$9249: CB A0        RES 4,B
```

The `SET 4 / RES 4` on `B` between reads is suspicious — looks like a bit-banged signal toggle, possibly to a custom drive-select latch we don't model, or a deliberate timing pulse the loader uses as part of the protection check.

Hypotheses (none investigated):
1. **N=6 sector handling** — our `ReadData` loop iterates by sector ID and uses `128 << N` for sector size; N=6 gives 8 192. The recorded `data_len` for N=6 sectors might not match nominal, exposing the same bug we hit on Tetris's track 12 earlier (zero-length EDSK on N>=5 sectors). Worth dumping the EDSK SIL for track 2.
2. **Rotational sector order** — the loader may be doing successive `ReadID` calls expecting different sectors on each call (modelling head rotation). Our `ReadId` always returns `sectors[0]` of the current track, so the loader would see "the same sector forever" and fail any rotation-based check.
3. **Custom drive-select latch** — the `SET 4 / RES 4` on B before each `OUT (C),A` writes the data port with bit 4 of B varying. Whether this affects which drive the FDC sees depends on the +3's gate-array decoding; our drive-select-mask is fixed at `0x01`.

Next investigation step: dump track 1 + track 2's recorded sectors (CHRN, ST1/ST2, data_len) the way we did for Op Wolf, then trace FUSE running the same DSK to see what `ReadID` rotation looks like.

## Tetris (1988, Mirrorsoft, +3) — clean black-screen wedge

After the 2026-05-11 fix to `format-amstrad-dsk`'s zero-length EDSK sector handling, Tetris parses cleanly and the loader runs. But:

- PC histogram spreads across pages `$18`, `$1C`, `$1E`, `$1F`, `$5E` — all lower RAM. Loader is bouncing through several routines, not stuck in one tight loop.
- Framebuffer is uniformly **pure black** — paper AND border are 0. That's an intentional wipe by the loader (`OUT $FE, 0` to set border, `LDIR` zeros into screen RAM), not a no-paint side-effect.
- `iff1 = false` so no IRQ can break out.

The pattern fits "loader has finished decrypting / decompressing, transferred control to the game's main loop, and the game is waiting on something we don't deliver." Candidates:
1. **Keyboard scan via a non-standard port** — Tetris might poll a keyboard column we route differently, never seeing any key press.
2. **AY register state** — Tetris's title screen plays the Korobeiniki tune. If the game runs from an AY-driven trigger, our AY chip not firing the expected envelope/tone callback could keep the main loop waiting.
3. **IM 2 vector table** — the game might install an IM 2 handler at a vector we don't deliver. With `iff1=0` though, IM 2 isn't relevant.

Next investigation step: capture the framebuffer at multiple frame counts (1 000, 5 000, 50 000) — if the screen stays pure black even at 50 000 frames, the game has fully wedged. If it changes between samples, the main loop is alive but rendering nothing.

## Why these aren't fixed in the same session as Speedlock-6

Each is a separate diagnosis ladder:
- Speedlock-6 needed a chip-level model (marginal encoding) and a clear hypothesis (weak sectors) that the FDC trace confirmed.
- Turrican needs an EDSK-side audit *plus* potentially a chip-level rotation model for `ReadID`. Different bug class.
- Tetris needs a non-FDC investigation — keyboard, AY, or post-load runtime state. Probably needs a different diagnostic harness than the FDC trace.

Bundling them into one commit-cycle would have meant moving from "land Speedlock-6 cleanly" to "land three independent fixes", which is exactly the kind of rolled-up change that prevents clean rollback.

## Catalogue scope today

Neither title currently has a +3 catalogue entry. They were on the 2026-05-11 survey but flagged as failing; nothing in `manifest/spectrum.toml` references them.

## Related rules and decisions

- RULES.md rule 20 / 21 — neither title gets a stub or a ROM trap. When they're fixed it will be by modelling whatever silicon behaviour they're checking for.
- `wiki/decisions/marginal-encoding-model.md` — the closest precedent for "chip-level model resolves a stuck protection check."
- `wiki/decisions/spectrum-plus3-disk-loading-incomplete.md` — the running diagnosis log; both titles appear there with their "black-screen" symptom.
- `wiki/decisions/no-rom-trap-load.md` — establishes the rule we're staying on the right side of.

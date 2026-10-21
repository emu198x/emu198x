# Amiga: up to four floppy drives

Steve, 2026-10-21: the Amiga should take up to four drives, DF0 to DF3.
Two-disk games need it: Fantastic Dizzy (AGA) asks for "volume fdizzy2 in
any drive" and, given disk 2 in DF0, asks for disk 1 back; Crystal
Kingdom Dizzy asks for disk two. AmigaDOS looks in every drive, so a
second drive holding disk 2 lets them carry on without swapping.

## Decision

A drive is fitted when a disk is put in its slot, or when asked for
(`--drives N`). With nothing extra the machine is a one-drive Amiga, as
before, so existing boots, fixtures and memory use do not change.
Kickstart counts drives at reset by reading each drive's ID, so a drive
fitted after boot is seen by AmigaDOS from the next reset.

## Source

Amiga Hardware Reference Manual (1989),
`reference/by-system/commodore-amiga/1989-amiga-hardware-reference-manual.txt`:

- CIA-B PRB (about line 11376): PB3 to PB6 are DSKSEL0* to DSKSEL3*,
  active low; PB7 is MTR*, PB2 SIDE*, PB1 DIR, PB0 STEP*.
- Appendix E, external disk connector (about lines 14670-14770):
  - MTRXD* is clocked into each drive's own motor flip-flop by the
    active transition of its SELxB*.
  - Identification mode: turn the motor on then off, then each select
    pulse shifts one bit of a 32-bit ID out on RDY*, MSB first, active
    low. `$FFFFFFFF` is a standard 3.5" drive, `$00000000` no drive.
  - At most three external drives.
  - RDY*, WPRO*, TK0* and CHNG* are driven by the selected drive.

`AmigaFloppyDrive` already models one drive's select, motor latch, ID
shift and status; DF0 alone is wired today, by
`decode_cia_b_prb_for_df0` in the OCS and A1200 machines.

## Steps

1. `peripheral-commodore-amiga-floppy`: a drive chain owning DF0 (always
   fitted) and DF1-DF3 (each fitted or absent). It decodes a CIA-B PRB
   write into each fitted drive's own select line (shared step, direction,
   side and motor data); combines the selected drives' active-low status
   for CIA-A PRA, with an absent drive leaving the lines inactive; names
   the selected drive for disk DMA; and ticks every drive. Tests, from the
   manual: a fitted external drive's ID reads `$FFFFFFFF` after motor
   on-then-off and 32 select pulses, an absent one `$00000000`; motor
   latches are per drive; a step moves only the selected drive.
2. OCS and A1200 machines use the chain in place of their single drive;
   disk DMA reads and writes the selected drive. Keep the DF0 accessors
   for existing callers; add per-drive ones.
3. `runtime-commodore-amiga`: media slots `floppy-1` to `floppy-3`
   ("DF1:" to "DF3:"). Inserting a disk fits that drive; ejecting leaves
   it fitted. A way to fit drives empty (`--drives N`). Queries for each
   drive's disk.
4. `emu198x-amiga`: `--disk1`, `--disk2`, `--disk3` and `--drives N`;
   scripts already reach the slots through `load_media`.
5. Snapshots: the chain is machine state, so advance the Amiga runtime
   envelope and amend `knowledge/decisions/savestate-live-machine-serde.md`,
   as earlier additions did.
6. Verify:
   - The one-drive path is unchanged: the existing Amiga tests and boot
     fixtures pass as before.
   - Kickstart 1.3 and 3.1 see a second drive.
   - Fantastic Dizzy (AGA, A1200, Kickstart 3.1) with disk 2 in DF1 gets
     past its "insert volume fdizzy2" request.
   - Crystal Kingdom Dizzy (A500, Kickstart 1.3) with disk 2 in DF1 gets
     past "insert disk two".

Not in this change: Fleet and the Apple app, which load media after boot.
They follow once the runtime has the slots.

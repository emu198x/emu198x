# Spectrum 48K Issue 1 and Issue 2 boards

Asked for by Steve on 3 November 2026, for *Code Like It's 198x* chapter 15
("the machine changed under its users"). The 48K core already models both ULA
families, and tests both, but only the Issue 3 board can be run.

## What differs, and the sources

| Board | ULA | Software can see | Source |
|---|---|---|---|
| Issue 1 | 5C102E, fixed by the external "dead cockroach" | Bit 6 of a port 254 read follows MIC as well as EAR. No "spider", so the ULA holds up every I/O cycle, not just its own. | Smith, *The ZX Spectrum ULA*, ch. 24 p. 250; Table 20-2 p. 220 |
| Issue 2 | 5C112E, with the spider | Bit 6 follows MIC as well as EAR | Same |
| Issue 3+ | 6C001E | What `spectrum_48k` runs today | |

Bit 6: with MIC alone set, the 5C's EAR input sits at 0.728 V, above the
0.714 V switching point, and reads 1; the 6C's sits at 0.652 V and reads 0
(Smith pp. 220 and 222). *Sinclair User* 26 (May 1984) printed a game that
checks the keyboard with `IN 65278<>255` and told Issue 3 owners to change
every 255 to 191.

The spider ORs A0 with /IORQ before the ULA's /IOREQ pin (Smith p. 250); the
6C001E still needed it. Without it, the contention handler treats every I/O
request as the ULA's, "interrupting requests for I/O ports such as the ZX
Printer". The ULA still only answers even ports.

The Issue 1 is modelled as sold, with the cockroach. Smith also gives the bare
5C102E's faulty I/O wait (`/IOWAIT = /(C3 + C2) + /(C2 + C1)`, p. 199); that
can be a further variant if it's ever wanted.

## Tasks

1. **The spider as a board property.** `Variant48kClass` gains a constant for
   whether the spider is fitted, true by default. Without it, the core shows the
   ULA's contention input A0 as low on I/O cycles, so every port contends. Being
   part of the type, it doesn't change the save format.
   - Verified by: a test that an `IN` from an odd port during the picture takes
     longer on the Issue 1 than the Issue 2, and the same on the Issue 2 and 3;
     it fails without the change. Existing contention and save baselines
     unchanged.
2. **Two machines.** `Spectrum48kIssue1` and `Spectrum48kIssue2` in
   `machine-sinclair-zx-spectrum-48k`, both with the 5C ULA.
   - Verified by: bit 6 reads 1 after `OUT 254,8` on both, 0 on the Issue 3.
3. **Two models.** `Spectrum48KIssue1Pal` and `Spectrum48KIssue2Pal` in the
   runtime: profiles, `spectrum_48k_issue1` / `spectrum_48k_issue2` on the
   command line, dispatch, saves (keyed by profile, as now), menus.
   - Verified by: booted from the real ROM, once a key has been pressed and
     let go, `IN 65278` reads 255 on both new boards and 191 on
     `spectrum_48k`; each saves and restores as itself. Before any key, all
     three read 191: the key click runs the ROM's BEEPER, whose `OR +08`
     leaves MIC's bit set in the last write to port 254 (Logan and O'Hara,
     *The Complete Spectrum ROM Disassembly*).
4. **The book.** Chapter 15 gets the listing and the measured results.

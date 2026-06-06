# Inter-Chip Timing Sequences

The individual chip documents describe each chip in isolation. This document
describes how they work together — the multi-chip sequences that run during
every frame, and the exact timing relationships that an emulator must honour.

## The Fundamental Constraint

One chip-bus transaction per colour clock (CCK). Agnus owns the bus and decides
who gets each slot. Every sequence below is constrained by this: if two things
need the bus on the same CCK, one waits.

## DMA Fetch → Denise Display

This is the core display pipeline: Agnus fetches bitplane data from chip RAM,
Denise shifts it out as pixels.

### Sequence (one fetch group, lowres)

```
CCK N+0:  Agnus: fetch group slot — free (copper or CPU can use)
CCK N+1:  Agnus: fetch BPL4DAT from chip RAM → Denise BPL4DAT latch
CCK N+2:  Agnus: fetch BPL6DAT (if 6 planes) → Denise BPL6DAT latch
CCK N+3:  Agnus: fetch BPL2DAT → Denise BPL2DAT latch
CCK N+4:  Agnus: fetch group slot — free
CCK N+5:  Agnus: fetch BPL3DAT → Denise BPL3DAT latch
CCK N+6:  Agnus: fetch BPL5DAT (if 5+ planes) → Denise BPL5DAT latch
CCK N+7:  Agnus: fetch BPL1DAT → ALL shift registers parallel-load
```

**The trigger:** Writing BPL1DAT (the last plane fetched) triggers the parallel
load of all shift registers simultaneously. If BPL1DAT is never written (wrong
plane count, DMA disabled), the shift registers keep outputting stale data.

**Pixel output:** After the parallel load, Denise shifts out one bit per plane
per pixel clock. At lowres, that is 2 pixels per CCK (16 pixels from the 16-bit
shift register = 8 CCKs per fetch group). At hires, 4 pixels per CCK (16 pixels
= 4 CCKs per fetch group).

### Scroll Delay (BPLCON1)

BPLCON1 adds a 0–15 lowres pixel delay between the parallel load and the first
pixel output. Odd planes (PF1) and even planes (PF2) scroll independently.
The delay is applied after the load — data still arrives at the same CCK, but
output is shifted in time.

### Pipeline Implications

- There is no buffering between Agnus and Denise beyond the holding latches.
  Data fetched on CCK N is displayed starting from CCK N+7 (lowres) or N+3
  (hires), plus any scroll delay.
- If the CPU or blitter steals a bitplane DMA slot (shouldn't happen — bitplane
  DMA has highest variable-slot priority), that plane's data is wrong.
- DDFSTRT/DDFSTOP control when fetches happen; DIWSTRT/DIWSTOP control when
  output is visible. These are independent — the fetch window can be wider than
  the display window (data is fetched but not shown).

## Audio DMA → Paula Output

Audio DMA has a pipeline with a return latency that models the chip-bus data
path.

### Sequence (one audio channel)

```
CCK $07:  Agnus: audio channel 0 DMA slot — fetches word from AUD0LC
          Paula: word enters return-latency counter (14 CCKs to ready)

CCK $08–$14: Return latency countdown
             Stalls when: refresh, disk, sprite, audio, bitplane DMA slots
             Does NOT stall: CPU slots, free slots, copper slots (unless fetching)

CCK ~$15:  Paula: word is available to the channel
           Channel consumes high byte first, then low byte
           Each byte plays for AUD0PER colour clocks
```

### Timing Details

- The 14-CCK return latency is not wall-clock — it stalls during DMA-owned
  slots. On a heavy display (6 planes + sprites + disk), the effective latency
  is longer.
- When a channel exhausts its word count (AUD0LEN reaches 0), it reloads the
  pointer from AUD0LC, reloads the length from AUD0LEN, and fires the audio
  interrupt. This is the point where a double-buffered interrupt handler must
  have the next buffer ready.

## Disk DMA → Paula → Agnus

Disk DMA involves all three chips: Paula drives the timing, Agnus provides the
bus slots, and the floppy drive provides the raw data.

### Read Sequence

```
1. Software writes DSKLEN twice (two-write safety protocol)
2. Paula begins searching the MFM bitstream for DSKSYNC ($4489)
3. On sync match:
   a. Paula sets DSKBYTR.WORDEQUAL
   b. DSKSYN interrupt fires (INTREQ bit 12)
   c. Paula begins assembling 16-bit words from the bitstream
4. Each complete word:
   a. Paula requests a DMA slot
   b. Agnus services the request during CCK $04–$06 (disk DMA slots)
   c. Word is written to chip RAM at DSKPT, pointer advances
   d. Word count (DSKLEN bits 13-0) decrements
5. When word count reaches 0:
   a. DSKBLK interrupt fires (INTREQ bit 1)
   b. Disk DMA stops
```

### Write Sequence

Same slot allocation, but Paula reads from chip RAM and presents words to the
drive's write head. The two-write protocol prevents accidental disk corruption.

## Interrupt Delivery

Interrupts cross three chip boundaries: the source chip → Paula → CPU.

### CIA Interrupt Path

```
1. CIA-A: Internal event (timer underflow, keyboard byte, TOD alarm)
2. CIA-A: ICR flag set for the source
3. CIA-A: If ICR mask enables the source, CIA-A asserts its IRQ output
4. Paula: Sees CIA-A IRQ → sets INTREQ bit 3 (PORTS)
5. Paula: If INTENA bit 3 AND INTEN (bit 14) are set:
   a. Paula drives IPL pins to level 2 (010)
   b. ~2 CCK delay between INTREQ set and IPL pin change
6. CPU: Samples IPL pins → if level > current SR mask, begins exception
7. CPU: Vectors to autovector handler for level 2
8. Software: Reads Paula INTREQ to confirm PORTS
9. Software: Reads CIA-A ICR to determine the specific CIA source
   (reading ICR clears ALL flags — must save the value)
10. Software: Clears Paula INTREQ bit 3 (write $0008 to INTREQ)
```

CIA-B follows the same path but at level 6 (EXTER, INTREQ bit 13).

### Custom Chip Interrupt Path

```
1. Agnus: Blitter finishes → requests BLIT interrupt
2. Paula: Sets INTREQ bit 6 (BLIT)
3. Paula: If INTENA bit 6 AND INTEN set → IPL = level 3
4. CPU: Level 3 autovector
5. Software: Reads INTREQ, sees BLIT (and possibly VERTB or COPER)
6. Software: Handles each active source at this level
7. Software: Clears INTREQ bits for handled sources
```

### Key Timing Notes

- **2-CCK IPL delay:** After INTREQ changes, the CPU does not see the new
  interrupt level for ~2 colour clocks. Software must not assume instant
  delivery.
- **ICR read-clears-all:** Reading CIA ICR returns the flags AND clears them.
  Reading twice returns 0 the second time. This is the most common CIA emulation
  bug — if the emulator doesn't clear on read, interrupts re-trigger endlessly.
- **INTEN gates everything:** INTENA bit 14 is the master enable. With INTEN
  clear, no interrupts reach the CPU regardless of individual enable bits.

## Copper Execution

The copper shares bus time with bitplane DMA and the CPU.

### Execution Cycle

```
Even CCK (in variable region, not taken by bitplane DMA):
  Copper slot available

CCK N:   Copper fetches IR1 (first instruction word)
CCK N+2: Copper fetches IR2 (second instruction word), executes:
         - MOVE: write IR2 to the register in IR1
         - WAIT: enter Wait state, begin checking beam position
         - SKIP: compare beam position, skip next instruction if >=
```

### WAIT Resolution

```
Each subsequent even CCK (copper slot):
  Compare (VPOS & mask, HPOS & mask) >= (wait_v & mask, wait_h & mask)
  V7 (bit 7 of VPOS) is ALWAYS compared, even if masked
  If comparison resolves: copper resumes instruction fetch on next slot
  If BFD bit clear: also wait for blitter to finish
```

### Copper Starvation

On a full 6-plane lowres display, all 8 CCKs per fetch group are used by
bitplane DMA. The copper gets no slots during the active display — it can only
execute during HBLANK or lines without bitplane DMA. AGA 8-plane mode is even
worse: zero free slots per group.

## Sprite DMA → Denise

### Per-Line Sequence

```
CCK $0B: Agnus fetches SPR0POS/SPR0CTL from sprite 0 pointer
CCK $0C: Agnus fetches SPR0DATA/SPR0DATB from sprite 0 pointer + 2
         Denise: arms sprite 0 — will display at HSTART position
         Agnus: sprite 0 pointer advances by 4 bytes

CCK $0D–$0E: Same for sprite 1
...
CCK $19–$1A: Same for sprite 7
```

After arming, Denise outputs the sprite pixels when HPOS reaches the sprite's
HSTART value. Sprite-playfield priority is resolved pixel-by-pixel using
BPLCON2.

### Sprite DMA Gating

Sprite DMA only occurs on lines between VSTART and VSTOP (from SPRxCTL). On
lines outside this range, sprite DMA slots are unused (available to CPU).
After VSTOP, the sprite pointer stops advancing until the next occurrence of
VSTART on a subsequent frame.

## Keyboard → CIA-A → Paula → CPU

The full path from a keypress to software handling:

```
1. Keyboard controller: scans matrix, encodes keycode
2. Keyboard: sends 8 bits serially via KDAT/KCLK
   Encoding: NOT(keycode ROL 1) — inverted, rotated left
3. CIA-A: SDR captures 8 bits, sets ICR bit 3 (SP)
4. CIA-A: If ICR mask enables SP, asserts IRQ output
5. Paula: Sets INTREQ bit 3 (PORTS)
6. Paula: IPL → level 2
7. CPU: Level 2 autovector → keyboard handler
8. Software: Reads CIA-A ICR (clears flags), checks SP bit
9. Software: Reads CIA-A SDR → gets encoded byte
10. Software: Decodes: keycode = NOT(SDR) ROR 1
11. Software: Handshakes:
    a. Set CIA-A CRA bit 6 (SP output mode)
    b. Wait ≥ 75µs
    c. Clear CIA-A CRA bit 6 (SP input mode)
    The falling edge on SP tells the keyboard to send the next byte
```

## Floppy Step → CIA-A FLAG

```
1. Software: Writes CIA-B PRA to select drive, set direction, pulse /STEP
2. Drive: Steps head to new cylinder
3. Drive: /INDEX pulse fires once per revolution (~200ms for DD)
4. CIA-A: FLAG pin detects falling edge of /INDEX
5. CIA-A: Sets ICR bit 4 (FLAG)
6. If enabled: → Paula PORTS → level 2 interrupt
```

The /INDEX pulse is used for rotation timing. Software counts index pulses to
measure the disk rotation period and synchronise track reads.

## Frame Timing Summary

One complete PAL frame (312 lines, 70,884 CCKs):

```
Line 0–24:    Vertical blank. COPJMP1 fires at line 0.
              Copper begins executing COP1LC.
              VERTB interrupt fires (INTREQ bit 5).
              No bitplane DMA (display window not yet open).
              All variable slots available to copper and CPU.

Line 25–43:   Pre-display. Copper typically sets up display registers.
              Sprite DMA active if sprites have VSTART in this range.

Line 44–299:  Active display. Bitplane DMA runs from DDFSTRT to DDFSTOP.
              CPU bandwidth reduced (depends on plane count).
              Copper slots available only in HBLANK and free bitplane slots.
              Audio and disk DMA continue in fixed slots.

Line 300–311: Post-display. Bitplane DMA stops.
              All variable slots available again.
              Copper may still be running (colour changes in bottom border).

Line 312:     Frame ends. LOF toggles if interlace is active.
              VPOS wraps to 0. New frame begins.
```

NTSC is the same pattern with 262 lines and different line numbers for the
display window.

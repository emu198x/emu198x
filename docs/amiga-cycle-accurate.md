# Amiga cycle-accurate timing reference

An implementation-level companion to `amiga-hardware-reference.md` and
`amiga-graphics-display.md`. Everything here is extracted from the WinUAE
cycle-exact core and is meant for somebody writing a new Amiga emulator who
needs to understand *exactly* what Agnus, Paula, Denise and the 68000 are
doing on every color clock.

Unless noted otherwise, file references are relative to
`~/Projects/Emu198x-Unclean/WinUAE/` and line numbers match the tree checked
out there. The term **CCK** means "color clock" (one 280 ns tick of the
master 3.546895 MHz PAL clock; 279.37 ns NTSC). One CCK corresponds to one
`agnus_hpos++`.

---

## Table of contents

1. [WinUAE's timing model overview](#1-winuaes-timing-model-overview)
2. [Per-colour-clock DMA slot table](#2-per-colour-clock-dma-slot-table)
3. [Long line / short line (227 vs 228)](#3-long-line--short-line)
4. [Bitplane fetch rules](#4-bitplane-fetch-rules)
   * [4.5 BPL1DAT commit trigger](#45-bpl1dat-commit-trigger)
5. [Sprite DMA](#5-sprite-dma)
6. [Copper timing](#6-copper-timing)
7. [Blitter pipeline](#7-blitter-pipeline)
8. [Blitter vs CPU contention](#8-blitter-vs-cpu-contention)
9. [CPU contention model](#9-cpu-contention-model)
10. [CIA timing](#10-cia-timing)
11. [Audio DMA pipeline](#11-audio-dma-pipeline)
12. [Disk DMA](#12-disk-dma)
13. [Refresh DMA](#13-refresh-dma)
14. [Event queue mechanism](#14-event-queue-mechanism)
15. [Wait-state table](#15-wait-state-table)
16. [Implementation notes for a new emulator](#16-implementation-notes-for-a-new-emulator)

Appendices:
* [Appendix A — DMA slot allocation table](#appendix-a--dma-slot-allocation-table)
* [Appendix B — wait-state matrix](#appendix-b--wait-state-matrix)
* [Appendix C — cycle-exact implementation checklist](#appendix-c--cycle-exact-implementation-checklist)
* [Appendix D — source map](#appendix-d--source-map)
* [Appendix E — gaps and WinUAE approximations](#appendix-e--gaps-and-winuae-approximations)

---

## 1. WinUAE's timing model overview

### 1.1 The master CCK loop

WinUAE's cycle-exact core is driven from a single function, `do_cck()`, in
`custom.cpp`. It runs once per color clock and it is the closest thing in
the emulator to a "clock edge." Everything the chipset does is arranged
around this call.

```c
// custom.cpp ~12350 — do_cck() is one Agnus CCK
static void do_cck(bool docycles)
{
    get_cck_clock();

    if (!custom_disabled) {
        bitplane_rga_ptmod();          // latch pointer + modulo for BPL/SPR slot decided last CCK
    }

    handle_rga_out();                  // actually perform the DMA that was queued earlier

    generate_dma_requests();           // BPL / UHRES / Copper / Blitter deciders
    if (!custom_disabled) {
        decide_bpl(agnus_hpos);        // BPRUN / DDF state machine
    }

    if (need_vdiw_check) {
        need_vdiw_check = false;
        check_bpl_vdiw();
    }

    if (custom_fastmode == 0) {
        check_hsyncs();
    }

    if (agnus_hpos == HARDWIRED_DMA_TRIGGER_HPOS) {  // == 1
        ...
    }

    decide_hsync();
    empty_pipeline();

    inc_cck();
    if (docycles) {
        do_cycles_normal(1 * CYCLE_UNIT);  // advance event clock by one CCK
    }

    dmacon_bpl = (dmacon & DMA_BITPLANE) && (dmacon & 0x200);

    handle_pipelined_write();
    handle_pipelined_custom_write(false);

    shift_rga();                       // slide the 4-entry RGA pipeline

    if (custom_fastmode <= 0) {
        generate_dmal();               // DMAL shifter (refresh/disk/audio/sprites)
    }
}
```

The important observation is that WinUAE does **not** run DMA decisions and
DMA execution in the same CCK. Requests are *generated* into the RGA
pipeline one color clock before they actually fire. The pipeline is 4
entries deep:

```c
// custom.cpp:113
static struct rgabuf rga_pipe[RGA_SLOT_TOTAL + 1];   // RGA_SLOT_TOTAL == 3
```

The three meaningful positions are `RGA_SLOT_BPL` (0), `RGA_SLOT_IN` (1),
`RGA_SLOT_OUT` (2), plus one trailing clear slot. The rotating offsets
`rga_slot_first_offset / rga_slot_in_offset / rga_slot_out_offset` all
decrement in `shift_rga()` so last CCK's "IN" becomes this CCK's "OUT":

```c
// custom.cpp:340
static void shift_rga(void)
{
    rga_slot_first_offset--;
    rga_slot_in_offset--;
    rga_slot_out_offset--;
    rga_slot_first_offset &= 3;
    rga_slot_in_offset   &= 3;
    rga_slot_out_offset  &= 3;

    struct rgabuf *r = &rga_pipe[rga_slot_first_offset];
    clear_rga(r);
}
```

This 1-CCK pipeline is not a convenience — it matches real Agnus. The
comment on `bitplane_rga_ptmod()` is explicit:

```c
// custom.cpp:9750
// Because BPL and SPR DMA is decided 1 CCK earlier than others
// PT and MOD need to be loaded in following cycle, cycle
// when request goes to DMA addressing logic.
```

So the timeline for a bitplane fetch is:

| CCK | Activity |
|---|---|
| N−1 | `generate_bpl()` called, BPRUN sequencer picks a plane, slot reserved at `RGA_SLOT_BPL` |
| N   | `bitplane_rga_ptmod()` latches `bplpt[bpl]` and `bpl1mod/bpl2mod` into the RGA entry (now at `RGA_SLOT_IN`) |
| N+1 | `handle_rga_out()` performs the chip-RAM fetch and calls `write_drga_dat_bpl16/32/64()` to deliver the word to Denise |

### 1.2 Cycle accounting: CYCLE_UNIT and E-clock

The fundamental quantum is `CYCLE_UNIT`:

```c
// include/sysdeps.h:511
#define CYCLE_UNIT 512
#define OFFICIAL_CYCLE_UNIT 512
```

One CCK is `1 * CYCLE_UNIT` "event units". The CPU cycle (`cpucycleunit`)
is `CYCLE_UNIT / 2` because a 7.09 MHz 68000 cycle is half a CCK. The
E-clock unit is also `CYCLE_UNIT / 2`:

```c
// cia.cpp:125
#define E_CLOCK_LENGTH 10
#define E_CYCLE_UNIT (CYCLE_UNIT / 2)
#define DIV10 (E_CLOCK_LENGTH * E_CYCLE_UNIT)  /* 10 E-clocks in "half-CCK" units */
```

One E-clock is 10 × `E_CYCLE_UNIT` = 10 × (CYCLE_UNIT/2) = 5 × CYCLE_UNIT.
5 CCKs per E-clock, which matches the hardware (7.09 MHz / 10 = 709 kHz;
3.55 MHz / 5 = 709 kHz).

### 1.3 `agnus_hpos`, `linear_hpos`, `vpos`, `vsyncmintime`

```c
// custom.cpp:107
uae_u8 agnus_hpos;
int agnus_hpos_prev, agnus_hpos_next, agnus_vpos_next;
```

`agnus_hpos` is the raw Agnus beam position, 0–226 (PAL/NTSC short line) or
0–227 (long line). It is 0 at the end of horizontal blank / start of
refresh slots, wraps at `maxhpos`, and is what the program observes via
`VHPOSR`.

`linear_hpos` is the display-space horizontal position, which only exists
so WinUAE can draw pixels into a linear framebuffer when the program has
done weird things with programmable sync (BEAMCON0). In normal PAL/NTSC
modes, `linear_hpos` tracks `agnus_hpos`.

`vpos` is the beam vertical position. `current_linear_vpos` is the scan
line index into the framebuffer.

`vsyncmintime` / `vsyncmaxtime` / `vsyncwaittime` are `frame_time_t` (host
wall-clock) values used only to rate-limit frame rendering against the
host's real time. They are not part of the Agnus timing model; the Agnus
model runs on `currcycle`.

```c
// events.cpp:28
evt_t event_cycles, nextevent, currcycle;
uae_u32 currcycle_cck;
```

`currcycle` is the master 64-bit cycle counter in `CYCLE_UNIT` ticks.
Every `do_cck()` advances it by one CCK via `do_cycles_normal(CYCLE_UNIT)`.
All scheduled events (CIA, audio, blitter, HSYNC) live on this counter.

### 1.4 Three execution paths (cycle-exact, line-exact, RTG-only)

WinUAE has three chip-emulation paths chosen by `custom_fastmode`:

* `custom_fastmode == 0`: the full `do_cck()` pipeline described above.
  Every CCK is advanced individually. Bitplane, sprite, copper, blitter,
  audio and disk DMA all use the RGA pipeline.
* `custom_fastmode > 0`: "fast mode." If the current line is identical to
  the last time WinUAE saw this linear Y position (same bitplanes, same
  palette, same sprites, no BPLCON changes), WinUAE jumps straight from the
  refresh slot to the next HSYNC event and reuses the previous frame's
  shifter output. DMA is still accounted for via `dmal_fast()` — it just
  doesn't go through the per-CCK decider. See `sync_equalline_handler()`
  in custom.cpp:12421.
* `custom_fastmode == -1` and `custom_disabled`: chipset is idle (RTG /
  Picasso96 mode). Only a fake HSYNC event keeps CIAs and the CPU ticking.

For a new emulator you almost certainly want only path 1. The "fast mode"
optimization exists because WinUAE has to run full-speed under 1990s games
and demos on modern CPUs. A modern emulator running a single 68000 at
7 MHz on a 4 GHz host does not need this.

### 1.5 How one CCK advances

Reading `do_cck()` top to bottom:

1. `get_cck_clock()` — compute next `agnus_hpos` (0..maxhpos-1, wrapping).
2. `bitplane_rga_ptmod()` — for the request that was placed in `RGA_SLOT_BPL`
   one CCK ago (now just arrived at `RGA_SLOT_IN`), resolve `bplpt[]` and
   modulo. This models the 1-CCK pointer-latch delay.
3. `handle_rga_out()` — take the request at `RGA_SLOT_OUT` (placed 2 CCKs
   ago) and actually execute it: issue a chip-RAM read, call the relevant
   register, increment pointers, dispatch to Denise / Paula.
4. `generate_dma_requests()` — this is the DMA arbiter. It calls, in order:
   * `generate_bpl(cck_clock)` — bitplane DMA (BPRUN state machine)
   * `generate_uhres()` — AGA UHRES bitplane/sprite (rarely used)
   * `generate_copper()` — Copper state machine
   * `generate_blitter()` — Blitter state machine (only if `blit_queued`)
5. `decide_bpl(hpos)` — DDF hard/soft limit checks, BPRUN latch/unlatch.
   This writes flags that `generate_bpl()` will consume on the *next* CCK.
6. `decide_hsync()` — emit the HSYNC event, flush Denise's line, handle
   long/short-line alternation, `hsync_handler()`.
7. `inc_cck()` + `do_cycles_normal(CYCLE_UNIT)` — advance master clock.
8. `shift_rga()` — slide the pipeline.
9. `generate_dmal()` — DMAL shifter tick (handles refresh / disk / audio /
   sprite DMA slots, see §2).

The order matters: `handle_rga_out()` runs before `generate_dma_requests()`
in the same CCK, because the request being "outed" this CCK was decided
two CCKs ago. The new requests generated by `generate_dma_requests()` will
sit at `RGA_SLOT_BPL` or `RGA_SLOT_IN` until it's their turn.

---

## 2. Per-colour-clock DMA slot table

This is the core of the document. WinUAE does not have a printed "slot
0..226, slot N = X" table anywhere — it decides on the fly from the actual
hardware state machine. What follows is the reconstruction.

### 2.1 The DMAL shifter — how non-bitplane DMA gets allocated

Sprite, audio, disk and refresh slots are driven by a shift register in
Agnus called the DMAL. WinUAE models this exactly:

```c
// custom.cpp:10235
#define DMAL_REFRESH0 (1 << 1)   // first refresh slot
#define DMAL_REFRESH1 (1 << 2)
#define DMAL_REFRESH2 (1 << 3)
#define DMAL_REFRESH3 (1 << 4)
#define DMAL_DSK0     (1 << 5)
#define DMAL_DSK1     (1 << 6)
#define DMAL_DSK2     (1 << 7)
#define DMAL_AUD0     (1 << 8)
#define DMAL_AUD1     (1 << 9)
#define DMAL_AUD2     (1 << 10)
#define DMAL_AUD3     (1 << 11)
// sprites have earlier DMA decisions
#define DMAL_SPR0A    (1 << 11)
#define DMAL_SPR0B    (1 << 12)
#define DMAL_SPR1A    (1 << 13)
... through DMAL_SPR7B (1 << 26)
```

Note that `DMAL_AUD3` and `DMAL_SPR0A` both equal `(1 << 11)`. This is
intentional: sprite slots are checked on *odd* `agnus_hpos`, audio slots
on *even*, so the bits reuse the same shift position.

The shifter starts at the end of HBLANK with bit 1 set:

```c
// custom.cpp:10273
static void start_dmal(void)
{
    dmal_shifter |= 2;              // set DMAL_REFRESH0
}
static void shift_dmal(void)
{
    dmal_shifter <<= 1;
}
```

`start_dmal()` is called once per line from `hsync_handler()`
(custom.cpp:11477). From that moment, each CCK calls `generate_dmal()`:

```c
// custom.cpp:12323
static void generate_dmal(void)
{
    handle_dmal();

    // even cycles only
    if (!(agnus_hpos_prev & 1) && (agnus_hpos & 1)) {
        shift_dmal();
    }
}
```

The shifter advances only on even→odd transitions, so each bit stays in
place for 2 CCKs. Combined with the even/odd check inside `handle_dmal()`,
this is what produces the real Agnus pattern "one refresh every other CCK,
then disk, then audio, then sprites across hpos 0x15..0x34". Each `1 <<
N` bit fires at CCK `(N − 1)` after `start_dmal()` was called.

### 2.2 The refresh cluster (hpos 0x00–0x07)

```c
// custom.cpp:10334
if (dmal_shifter & (DMAL_REFRESH0 | DMAL_REFRESH1 | DMAL_REFRESH2 | DMAL_REFRESH3)) {
    if (dmal_shifter & DMAL_REFRESH0) {
        uae_u16 reg = get_strobe_reg(0);           // VBL strobe: 0x38 / 0x3a / 0x3c
        refptr &= refmask;
        struct rgabuf *rga = write_rga(RGA_SLOT_IN, CYCLE_STROBE, reg, &refptr);
        rga->refdat = 0;
    }
    if (dmal_shifter & DMAL_REFRESH1) {
        uae_u16 reg = get_strobe_reg(1);           // usually also a strobe, else 0x1fe
        ...
        write_rga(RGA_SLOT_IN, CYCLE_REFRESH, reg, &refptr);
    }
    if (dmal_shifter & DMAL_REFRESH2) {
        write_rga(RGA_SLOT_IN, CYCLE_REFRESH, 0x1fe, &refptr);   // NOP register
    }
    if (dmal_shifter & DMAL_REFRESH3) {
        write_rga(RGA_SLOT_IN, CYCLE_REFRESH, 0x1fe, &refptr);
    }
}
```

These four slots fall at CCK positions (0, 2, 4, 6) after `start_dmal()`
— i.e., they are the first 4 even slots of the line. They are marked with
`CYCLE_REFRESH` or `CYCLE_STROBE` and always consume the chip bus; CPU,
Copper and Blitter cannot take them. Disk, audio, sprite and bitplane DMA
cannot take them either (it's physically impossible — the DMAL bit doesn't
fire for them at those positions).

The first refresh slot doubles as the VBL / HBL strobe fetch that Denise
uses to latch the line type. The register is 0x38 (STRHOR) on normal
lines, 0x3a (STREQU) during vertical blank, or 0x3c (STRVBL) during long
field VSYNC. `hsync_handler_post()` generates the interrupt on the
transition from 0x3c back to 0x38 — that is what triggers the VERTB
interrupt, not a dedicated VBL signal.

### 2.3 Disk slots (hpos 0x07, 0x09, 0x0b)

```c
// disk.cpp:5162
uae_u16 disk_dmal(void)
{
    uae_u16 dmal = 0;
    if (dskdmaen) {
        if (dskdmaen == DSKDMA_WRITE) {
            dmal = (1+2)*fifo_inuse[0] + (4+8)*fifo_inuse[1] + (16+32)*fifo_inuse[2];
            dmal ^= 63;
            ...
        } else {
            dmal = 16*fifo_inuse[0] + 4*fifo_inuse[1] + 1*fifo_inuse[2];
        }
    }
    disk_strobe = true;
    return dmal;
}
```

The 3 disk slots sit at DMAL positions 0x05, 0x06, 0x07 (bits 5,6,7).
Because the DMAL shifter advances 1 position per 2 CCKs, these slots fire
at `agnus_hpos` 0x07, 0x09, 0x0b respectively after `start_dmal()`. Each
slot has 2 bits in the `dmal` word so Agnus knows whether it is a read or
a write.

Only as many disk slots are used as the FIFO needs. If the drive is idle,
all three are free for the CPU.

### 2.4 Audio slots (hpos 0x0d, 0x0f, 0x11, 0x13)

```c
// custom.cpp:10307
if (dmaen(DMA_AUD0|DMA_AUD1|DMA_AUD2|DMA_AUD3) && ((dmal >> (2*3)) & 255)
    && (dmal_shifter & (DMAL_AUD0|DMAL_AUD1|DMAL_AUD2|DMAL_AUD3))) {
    for (int nr = 0; nr < 4; nr++) {
        if (dmal_shifter & (DMAL_AUD0 << nr)) {
            uae_u32 dmalbits = (dmal >> ((3 + nr) * 2)) & 3;
            if (dmalbits) {
                uaecptr *pt = audio_getpt(nr);
                struct rgabuf *rga = write_rga(RGA_SLOT_IN, CYCLE_AUDIO, 0xaa + nr * 16, pt);
                rga->auddat = dmalbits | (((3 + nr) * 2) << 8);
            }
        }
    }
}
```

`dmalbits` carries both "DMA needed" and whether the pointer should be
reloaded from AUDxLC (end of sample). AUD0 fires at `agnus_hpos` 0x0d,
AUD1 at 0x0f, AUD2 at 0x11, AUD3 at 0x13.

If an audio channel is in state 0 (idle) or its period has not expired, it
will not request DMA for that line and the slot is free for CPU / blitter /
copper.

### 2.5 Sprite slots (hpos 0x15–0x34)

```c
// custom.cpp:10283
static void handle_dmal(void)
{
    if (!dmal_shifter) return;

    if (agnus_hpos & 1) {
        if (!custom_disabled && !agnus_vb_active && (dmal_shifter & (
            DMAL_SPR0A|DMAL_SPR1A|...|DMAL_SPR7A|
            DMAL_SPR0B|DMAL_SPR1B|...|DMAL_SPR7B))) {
            for (int nr = 0; nr < 8; nr++) {
                if (dmal_shifter & (DMAL_SPR0A << (nr * 2))) generate_sprites(nr, 0);
                if (dmal_shifter & (DMAL_SPR0A << (nr * 2 + 1))) generate_sprites(nr, 2);
            }
        }
    }

    if (agnus_hpos & 1) return;      // audio/disk/refresh only on even hpos
    ...
}
```

`generate_sprites()` places a sprite DMA request into `RGA_SLOT_BPL` using
register 0x140..0x17e (POS/CTL/DATA/DATB). Each sprite gets 2 consecutive
DMA slots per line: one for POS+CTL on lines where `dmastate == 0`, one
for DATA+DATB on lines where `dmastate == 1`.

Because the sprite A/B bits are in odd DMAL positions, sprites fire on
odd `agnus_hpos`:

| Sprite | Slot 0 (POS / DATA) | Slot 1 (CTL / DATB) |
|---|---|---|
| 0 | 0x15 | 0x17 |
| 1 | 0x19 | 0x1b |
| 2 | 0x1d | 0x1f |
| 3 | 0x21 | 0x23 |
| 4 | 0x25 | 0x27 |
| 5 | 0x29 | 0x2b |
| 6 | 0x2d | 0x2f |
| 7 | 0x31 | 0x33 |

These are the traditional "H0/H1 sprite slots" from the HRM. They begin
at `agnus_hpos 0x15` because that's 20 CCKs (0x14) after `start_dmal()`,
which is the position of the DMAL_SPR0A bit (bit 11, times the 2-CCK step,
minus 1 for the odd-hpos shift).

Sprite DMA is suppressed during vertical blank (`agnus_vb_active`), before
the line given by `sprxpos` (via `vstart`), and after `vstop`. The actual
machinery is in `generate_sprites()` which maintains per-sprite
`dmacycle`/`dmastate` state exactly like the real chip.

### 2.6 Bitplane slots (hpos 0x18 onward)

Bitplane DMA has its own sequencer (`bprun`) and its own cycle-diagram
table. Bitplane slots start at DDFSTRT (typically 0x18 for the "hard
start" limit) and run in 8-CCK units, one bitplane word per slot. The
interleave pattern is controlled by the `bpl_sequence[]` table:

```c
// custom.cpp:1053
static const uae_u8 bpl_sequence_8[32] = { 8, 4, 6, 2, 7, 3, 5, 1 };
static const uae_u8 bpl_sequence_4[32] = { 4, 2, 3, 1 };
static const uae_u8 bpl_sequence_2[32] = { 2, 1 };
```

For 8 planes (lores fetchmode 0), in each 8-slot block the Agnus fetches
plane 8 first, then 4, 6, 2, 7, 3, 5, 1. This staggered order is what
makes the lower planes available in time for the Denise shifter without
blocking the odd/even interleave for collision detection.

The cycle-diagram table is precomputed once at boot:

```c
// custom.cpp:950
static uae_s8 cycle_diagram_table[3][3][9][32];
static uae_s8 cycle_diagram_free_cycles[3][3][9];
static uae_s8 cycle_diagram_total_cycles[3][3][9];
static const uae_s8 cycle_sequences[3 * 8] = {
    2,1,2,1,2,1,2,1,                /* 2-plane */
    4,2,3,1,4,2,3,1,                /* 4-plane */
    8,4,6,2,7,3,5,1                 /* 8-plane */
};
```

The [fm][res][planes] indexing gives you, for each combination of fetch
mode (0/1/2), resolution (lores/hires/shres) and bitplane count (0..8),
the 32-entry slot-usage array for one fetch block. A value of −1 is "free"
(available to blitter/CPU); 1..8 names which bitplane is fetched.

### 2.7 The 0xE0 region — the "right-hand half"

After the bitplane area ends at DDFSTOP (or the hard stop at 0xd7 in ECS),
the slots from DDFSTOP through the end of the line are mostly free and
become the main source of CPU and blitter bandwidth on a display-heavy
line.

The hard-wired positions set by WinUAE for line-edge events are:

```c
// custom.cpp:1038
hw_hpos_table[1]   = true;        // HARDWIRED_DMA_TRIGGER_HPOS
hw_hpos_table[9]   = true;
hw_hpos_table[18]  = true;
hw_hpos_table[35]  = true;        // (0x23 — HSYNC start PAL)
hw_hpos_table[115] = true;        // (0x73 — approx. HSYNC end PAL)
hw_hpos_table[132] = true;
hw_hpos_table[26]  = true;        // NTSC VSYNC end
hw_hpos_table[27]  = true;
hw_hpos_table[140] = true;        // NTSC HBSTRT
hw_hpos_table[141] = true;
```

These are the positions at which the various HSYNC / HBLANK / VSYNC
signals toggle in hardware, not DMA slots. They exist so that Agnus can
run its hard-coded sync generator independently of the programmable
BEAMCON0 path.

### 2.8 Slot summary for a PAL short line

Putting it all together, Appendix A has the full table. The quick summary
for a typical PAL line with full bitplane DMA, 4 sprites, and audio+disk
idle:

```
hpos 0x00  REFRESH0 (with STRHOR/STRVBL strobe)
hpos 0x02  REFRESH1
hpos 0x04  REFRESH2
hpos 0x06  REFRESH3
hpos 0x07  DISK0              (free if drive idle)
hpos 0x09  DISK1              (free if drive idle)
hpos 0x0b  DISK2              (free if drive idle)
hpos 0x0d  AUD0               (free if channel idle)
hpos 0x0f  AUD1               (free if channel idle)
hpos 0x11  AUD2               (free if channel idle)
hpos 0x13  AUD3               (free if channel idle)
hpos 0x15  SPR0 POS/DATA
hpos 0x17  SPR0 CTL/DATB
hpos 0x19  SPR1 POS/DATA
... through 0x33 ...
hpos 0x34  (free)
hpos 0x35  (free)
...
hpos 0x38  bitplane DMA starts (DDFSTRT normal = 0x38)
... bitplane slots in 8-CCK blocks, see §4 ...
hpos 0xd0  bitplane DMA ends (DDFSTOP normal = 0xd0, plus tail)
hpos 0xd7  bitplane hard stop
...
hpos 0xe2+ all free (CPU / blitter / copper)
hpos 0xe2  end of line (CCK 226 on a 227 short line)
```

Within the bitplane region, slots that don't match the plane sequence for
the current plane count (e.g. in 4-plane lores you consume only 4 of 8
slots per block) are free and usable by blitter and CPU. See §4.

### 2.9 CYCLE_* type bitmask

Every RGA request carries a `CYCLE_*` bitmask saying who allocated it.
These are in `include/custom.h:158` and they're how `handle_rga_out()` and
the debug DMA recorder know what to do:

```c
#define CYCLE_BITPLANE  (1 << 0)
#define CYCLE_REFRESH   (1 << 1)
#define CYCLE_STROBE    (1 << 2)
#define CYCLE_DISK      (1 << 3)
#define CYCLE_AUDIO     (1 << 4)
#define CYCLE_SPRITE    (1 << 5)
#define CYCLE_COPPER    (1 << 6)
#define CYCLE_UHRESBPL  (1 << 7)
#define CYCLE_UHRESSPR  (1 << 8)
#define CYCLE_BLITTER   (1 << 9)
#define CYCLE_CPU       (1 << 10)
```

A single slot can have multiple bits set in unusual situations (e.g., the
OCS sprite/bitplane collision: `CYCLE_BITPLANE | CYCLE_SPRITE`), and
`handle_rga_out()` has special cases for those.

---

## 3. Long line / short line (227 vs 228)

### 3.1 Why the line length alternates

In NTSC mode the scan line is 227.5 color clocks long. That is not an
integer and Agnus cannot split a CCK, so it alternates between 227-CCK
"short" lines and 228-CCK "long" lines. In PAL the line is always 227
(actually 227 CCKs and 4 ns, treated as 227 flat). Long lines are also
generated in programmable BEAMCON0 modes.

### 3.2 WinUAE's LOL flag

```c
// custom.cpp:11466
if (!(new_beamcon0 & BEAMCON0_PAL) && !(new_beamcon0 & BEAMCON0_LOLDIS)) {
    lol = lol ? false : true;
    linetoggle = true;
} else {
    lol = false;
    linetoggle = false;
}

setmaxhpos();
```

`setmaxhpos()` picks `maxhpos = lol ? maxhpos_long : maxhpos_short` where
`maxhpos_short = 227` and `maxhpos_long = 228` in NTSC. WinUAE exposes
this to Denise via a dedicated flag so the pixel shifter knows to emit
one extra lores pixel on long lines:

```c
// include/custom.h:266
#define DENISE_RGA_FLAG_LOL 0x40
#define DENISE_RGA_FLAG_LOL_ON 0x80
```

### 3.3 Where the extra slot goes

The 228th CCK is appended at the *end* of the line, after the last
"normal" slot (the Copper STRHOR handover, position 0xe2 in PAL). On a
long line, the extra CCK is at position 0xe3. It is always idle (no DMA
channel ever schedules anything in the long-line slot), so the CPU always
gets it if it's waiting.

The `lol` toggling happens in `hsync_handler()` (custom.cpp:6615 via
`hsync_handler_pre()`) so it's effective immediately at the start of the
next line, which is why the DMAL shifter and the bitplane sequencer see
the same `maxhpos` on the long line as the short line — only the last
"padding" slot is added.

### 3.4 The LOL detect signal

ECS Agnus exposes `LOL` via a dedicated output pin (read by some external
monitors). WinUAE reproduces this:

```c
// custom.cpp:10568 lof_detect update, and check_vsyncs_fast()
if (vpos == 3 && lof_store) {
    agnus_vsync = true;
    lof_detect = 1;
    update_lof_detect();
}
```

The LOL toggle and the LOF ("long frame") toggle are independent: LOF
alternates every frame (to achieve 50.00 Hz PAL = 312.5 lines × 2 fields)
while LOL alternates every line (to achieve 227.5 CCK average). A
cycle-exact emulator must implement both.

---

## 4. Bitplane fetch rules

### 4.1 DDF alignment and the 8-CCK fetch unit

The programmer writes `DDFSTRT` and `DDFSTOP` as color-clock positions,
but Agnus quantises them to the current fetch unit. The fetch unit (in
CCKs) depends on fetchmode and resolution:

```c
// custom.cpp:946
static const uae_u8 fetchunits[]  = { 8,8,8,0, 16,8,8,0, 32,16,8,0 };
static const uae_u8 fetchstarts[] = { 3,2,1,0,  4,3,2,0,  5,4,3,0 };
static const uae_u8 fm_maxplanes[]= { 3,2,1,0,  3,3,2,0,  3,3,3,0 };
```

Indexed as `[fetchmode * 4 + resolution]`. So:

| FMODE | Res | fetchunit (CCK) | fetchstart (1<<N) | max planes (2^N) |
|---|---|---|---|---|
| 0 (16-bit) | Lores    | 8  | 8  | 8 |
| 0 (16-bit) | Hires    | 8  | 4  | 4 |
| 0 (16-bit) | SHires   | 8  | 2  | 2 |
| 1 (32-bit) | Lores    | 16 | 16 | 8 |
| 1 (32-bit) | Hires    | 8  | 8  | 8 |
| 1 (32-bit) | SHires   | 8  | 4  | 4 |
| 2 (64-bit) | Lores    | 32 | 32 | 8 |
| 2 (64-bit) | Hires    | 16 | 16 | 8 |
| 2 (64-bit) | SHires   | 8  | 8  | 8 |

So e.g. "hires 4 planes" in FMODE 0 is 8 CCKs per block (`fetchunit=8`)
with `2^2 = 4` plane slots used in each block. A 6-plane-hires display
would be impossible because `fm_maxplanes[0]=2`, i.e. a maximum of 4.

Only FMODE 1 and 2 allow 6 or 8 hires planes, and they require
correspondingly larger fetch units — which is why AGA burning 6 hires
planes eats 16 CCKs per 16-pixel block (FMODE 1 hires) instead of 8.

### 4.2 The cycle diagram

`create_cycle_diagram_table()` (custom.cpp:992) builds a 32-entry slot
map for each `[fm][res][planes]` combination:

```c
// custom.cpp:992
static void create_cycle_diagram_table(void)
{
    for (fm = 0; fm <= 2; fm++) {
    for (res = 0; res <= 2; res++) {
        max_planes = fm_maxplanes[fm * 4 + res];
        fetch_start = 1 << fetchstarts[fm * 4 + res];
        cycle_sequence = &cycle_sequences[(max_planes - 1) * 8];
        max_planes = 1 << max_planes;
        for (planes = 0; planes <= 8; planes++) {
            freecycles = 0;
            for (cycle = 0; cycle < 32; cycle++) {
                cycle_diagram_table[fm][res][planes][cycle] = -1;
            }
            if (planes <= max_planes) {
                for (cycle = 0; cycle < fetch_start; cycle++) {
                    if (cycle < max_planes && planes >= cycle_sequence[cycle & 7]) {
                        v = cycle_sequence[cycle & 7];
                    } else {
                        v = -1;
                        freecycles++;
                    }
                    cycle_diagram_table[fm][res][planes][cycle] = v;
                }
            }
            cycle_diagram_free_cycles[fm][res][planes] = freecycles;
            cycle_diagram_total_cycles[fm][res][planes] = fetch_start;
            ...
        }
    }
    }
}
```

In plain English: for (lores, 4 planes, FMODE 0) an 8-CCK block gets the
slot pattern `4 2 3 1 - - - -` — planes 4, 2, 3, 1 fetched in cycles
0..3 and cycles 4..7 are free. For 5 planes lores `4 2 3 1 - - - -` is
impossible (max_planes=4 for lores fm0), so the result is `-1 -1 -1 -1 -1
-1 -1 -1` — **zero planes** are fetched. This is the "5-plane lores is
not supported" behaviour of OCS/ECS: the bitplane DMA disables itself
entirely, not just the 5th plane.

WinUAE exposes this via `real_bitplane_number[fetchmode][res][planes]`
and the `GET_PLANES_LIMIT(bc0)` inline:

```c
// custom.cpp:962
STATIC_INLINE int GET_PLANES_LIMIT(uae_u16 bc0)
{
    int res = GET_RES_AGNUS(bc0);
    int planes = GET_PLANES(bc0);
    return real_bitplane_number[fetchmode][res][planes];
}
```

The WinUAE comment immediately above makes this explicit:

```c
// custom.cpp:960
/* Disable bitplane DMA if planes > available DMA slots. This is needed
   e.g. by the Sanity WOC demo (at the "Party Effect").  */
```

### 4.3 BPRUN — the actual fetch gate

The bitplane sequencer is state-machine gated by `bprun`:

```c
// custom.cpp:9892 decide_bpl()
if (ecs_agnus) {
    bool dma = dmacon_bpl;

    if (hpos == 0x18) {
        ddf_limit_in = false;       // hard start: clear DDF limit
    }
    if (hpos == 0xd7) {
        if (!harddis_h) {
            ddf_limit_in = true;    // hard stop: set DDF limit (unless HARDDIS)
        }
    }

    // DDFSTRT (odd cycle)
    if ((hpos == ddfstrt_val && cyc > ddfstrt_cycle) || ...) {
        ddf_enable_on = 1;
        ...
    }
    // DDFSTOP (even cycle)
    if ((hpos == ddfstop && cyc > ddfstop_cycle) || ...) {
        ddf_enable_on = 0;
    }

    // BPRUN can only start if DMA, DIW or DDF state has changed since last time
    bool hwi = dma && diw && ddf_enable_on && (!ddf_limit_out || harddis_h);

    if (!bprun && hwi && !hwi_old) {
        bprun = 1;
        bprun_cycle = 0;
        bprun_start(hpos + 1);
    } else if (bprun && !hwi && hwi_old) {
        if (!ddf_stopping) ddf_stopping = 1;
    }
    hwi_old = hwi;
}
```

Key behaviours:

* `bprun` gates all bitplane DMA. Turning BPLEN off (`dmacon_bpl = 0`)
  immediately asks the sequencer to enter `ddf_stopping = 1`, but any
  already-pipelined slot in RGA will still fire.
* The *hard* DDF limits (`ddf_limit_in/out`) are ECS/AGA only unless
  `HARDDIS_H` is set. Hard start at 0x18 clears the limit; hard stop at
  0xd7 sets it. This is what prevents programs from writing DDFSTRT =
  0 and scrolling chip RAM off the side of the screen.
* On OCS the sequencer is one CCK *later* than ECS/AGA because
  `dmacon_bpl2 = dmacon_bpl` runs one tick behind (see custom.cpp:10040).

### 4.4 Slot assignment inside `generate_bpl()`

```c
// custom.cpp:9850
static void generate_bpl(bool clock)
{
    if (bprun > 0) {
        int hpos = agnus_hpos;
        bool last = islastbplseq();
        int cycle_pos = bprun_cycle & fetchstart_mask;

        if (dmacon_bpl) {
            bool domod = false;
            if (ddf_stopping == 2) {
                int cycle = bprun_cycle & 7;
                if (fm_maxplane == 8 ||
                    (fm_maxplane == 4 && cycle >= 4) ||
                    (fm_maxplane == 2 && cycle >= 6)) {
                    domod = true;
                }
            }
            int plane = bpl_sequence[cycle_pos];
            if (plane >= 1 && plane <= bplcon0_planes_limit) {
                int bpl = plane - 1;
                struct rgabuf *rga = write_rga(RGA_SLOT_BPL, CYCLE_BITPLANE,
                                               0x110 + bpl * 2, NULL);
                rga->bpldat = bpl | (domod ? 8 : 0);
            }
        }
        if (clock) bprun_cycle++;
        if (last) {
            if (ddf_stopping == 2) bpl_dma_normal_stop(hpos);
            if (ddf_stopping == 1) ddf_stopping = 2;
        }
    }
}
```

Notes:

* Register 0x110 + 2*N is `BPL1DAT` through `BPL8DAT`. Writing to any of
  these triggers the Denise shifter — see §4.5.
* The `domod` flag tells `handle_rga_out()` to add the per-line modulo
  (BPL1MOD or BPL2MOD) to the pointer after the fetch, rather than the
  normal +2/+4/+8 increment. It fires on the last fetch of the last
  block of the line.
* `bplcon0_planes_limit` is the result of `GET_PLANES_LIMIT()`, so a
  request for plane 5 in lores is silently discarded.

### 4.5 BPL1DAT commit trigger

Here is a critical subtlety that the HRM glosses over: the Denise shifter
does not latch any BPLxDAT until the programmer (or Agnus DMA) writes
`BPL1DAT`. *All* planes are loaded at once when BPL1DAT is written, and
they are loaded from Denise's internal `bplxdat[]` register bank, not
directly from chip RAM. Agnus/DMA populates `bplxdat[]` as each plane
arrives via its fetch slot, and the final plane 1 fetch commits the whole
group:

```c
// drawing.cpp:3765 — bpldat_docopy()
// bpl1dat write -> copy all bplxdats to internal registers
// (must copy all, not just current plane count because if planecount
// is decreased mid line, old higher planes must be still shifted out)
static void bpldat_docopy(void)
{
    if (aga_mode) {
        if (denise_bplfmode64) {
            bplxdat2_64[0] = bplxdat_64[0];
            ...
            bplxdat2_64[7] = bplxdat_64[7];
        } else {
            bplxdat2[0] = bplxdat[0];
            bplxdat2[1] = bplxdat[1];
            ...
        }
    }
    ...
}

// drawing.cpp:3738
// BPL1DAT allows sprites 1 lores pixel before bitplanes
static void bpl1dat_enable_sprites(void)
{ ... sprites_hidden2 &= ~2; ... }

static void bpl1dat_enable_bpls(void)
{ ... bpl1dat_trigger = true; ... }
```

Consequences for the emulator author:

1. If the programmer writes `BPL1DAT` manually from the CPU (even without
   any bitplane DMA), Denise will start shifting whatever is currently in
   `BPLxDAT`. This is how some demos make the "black border" background
   with a single CPU poke to BPL1DAT.
2. If you decrease the plane count mid-line, the old higher planes keep
   shifting out because `bplxdat[]` doesn't get cleared — only the
   `planes` register controls which bits are selected by the color
   lookup.
3. The BPL1DAT commit unhides sprites one lores pixel earlier than it
   unhides bitplanes, so a sprite on the leftmost bitplane column is one
   pixel to the left. This is handled in `bpl1dat_enable_sprites()` vs
   `bpl1dat_enable_bpls()`.
4. On OCS Denise, `denise_burst` (BURST mode) inhibits both triggers.

### 4.6 FMODE fetch width

```c
// custom.cpp:1075
fetchmode_bytes = 2 << fmm;           // 2 / 4 / 8
fetchmode_fmode_bpl = fm & 3;         // separate bit pair for bitplanes
fetchmode_fmode_spr = (fm >> 2) & 3;  // and for sprites
```

`fetchmode_bytes` is the number of bytes fetched per slot: 2 (FMODE=0,
word), 4 (FMODE=1/2, long), or 8 (FMODE=3, quadword). `fetchmode_fmode_bpl`
is what `handle_rga_out()` looks at to pick the right `fetch16 / fetch32 /
fetch64` function (custom.cpp:12193).

Sprite FMODE is independent so you can have 16-bit sprites + 32-bit
bitplanes.

---

## 5. Sprite DMA

### 5.1 Per-sprite state machine

```c
struct sprite {
    uaecptr pt;           // SPRxPT
    int vstart, vstop;    // decoded from POS/CTL
    int armed;
    int dmastate;         // 0 = looking for vstart, 1 = in active area
    int dmacycle;         // 0 = idle slot, 1 = slot 0 this pair, 2 = slot 1 this pair
    bool dblscan;         // AGA attached-sprite double-scan
};
```

Each sprite has two consecutive DMA slots per line. `generate_sprites()`
(custom.cpp:10157) is called twice per line per sprite — once from the
`SPR0A` DMAL bit, once from `SPR0B`. The sprite sequencer:

* If `dmastate == 0` (waiting for vstart): the slot loads POS (slot 0,
  register 0x140 + N*8) and CTL (slot 1, 0x142 + N*8). If `vpos ==
  s->vstart`, `dmastate` is set to 1.
* If `dmastate == 1` (active): the slot loads DATA (slot 0, 0x144 + N*8)
  and DATB (slot 1, 0x146 + N*8).
* If `vpos == s->vstop` or we hit vertical blank end, `dmastate` goes
  back to 0.

```c
// custom.cpp:10157
static void generate_sprites(int num, int slot)
{
    ...
    if (slot == 0) {
        if (!s->dmacycle && s->dmastate) s->dmacycle = 1;
        if (vpos == s->vstart) {
            s->dmastate = 1;
            s->dmacycle = 1;
            if (num == 0 && slot == 0) cursorsprite(s);
        }
        if (vpos == s->vstop || agnus_vb_active_end_line) {
            s->dmastate = 0;
            s->dmacycle = 1;
        }
    }
    if (dmaen(DMA_SPRITE) && s->dmacycle) {
        bool dodma = false;
        // if bitplane DMA ends and last BPL1DAT slot is also sprite slot and sprite DMA is active
        bool bplconflict = false;
        if (bprun && ddf_stopping == 2) {
            if (islastbplseq()) bplconflict = true;
        }
        if (bprun != 1 || bplconflict) {
            dodma = true;
            ...
            if (dodma) {
                uae_u32 dat = CYCLE_PIPE_SPRITE | (s->dmastate ? 0x10 : 0x00)
                            | (s->dmacycle == 1 ? 0 : 8) | num;
                int reg = 0x140 + slot + num * 8 + (s->dmastate ? 4 : 0);
                struct rgabuf *rga = write_rga(RGA_SLOT_BPL, CYCLE_SPRITE, reg, NULL);
                ...
            }
        }
    }
    ...
}
```

### 5.2 Sprite vs bitplane slot conflict

`if (bprun != 1 || bplconflict)` — sprite DMA is normally suppressed when
the bitplane sequencer is running, because sprites and bitplanes share the
0x15..0x34 DMAL area in a way that only works when the bitplane display
doesn't claim those slots. The one exception: the "last bitplane slot"
conflict, which is the well-known OCS "sprite reuse" edge case.

### 5.3 Reused sprites via Copper

The real Agnus has no "reused sprites" concept — it just runs the sprite
state machine from whatever the sprite pointer contains. The Copper
trick is: after a sprite has finished (`dmastate == 0` and `dmacycle ==
0`), the Copper writes new POS/CTL/DATA/DATB to `SPRxPOS/CTL/DATA/DATB`
(the Denise-side sprite registers) before the sprite's slot on the next
line. Because the sprite state machine sees `dmastate == 0` and its
pointer already advanced (from the last DMA write), it does not fetch
again that line — but Denise still has the new data and displays it.

In WinUAE, this all falls out of the model naturally: the Copper moves
data into Denise through the `custom_wput` path, which does not touch
`generate_sprites()`, so the sprite state machine keeps running
unchanged.

### 5.4 Sprite DMA enable edge case

```c
// custom.cpp:10211
evt_t c = get_cycles();
if (c == sprite_dma_change_cycle_on) {
    // If sprite DMA is switched on just when sprite DMA is decided,
    // channel is still decided but it is not allocated!
    // Blitter can use this cycle, causing a conflict.
    rga->alloc = 0;
    rga->conflict = &s->pt;
}
```

If DMACON bit 5 is set *exactly* on the CCK when a sprite slot is
decided, Agnus still decides it (puts the RGA request in flight) but
does not allocate the bus cycle. Blitter and CPU can still use it, and
you get a conflict: the sprite pointer is corrupted with `OR` of old and
new values. WinUAE models this via `rga->alloc = 0` and `rga->conflict`.

---

## 6. Copper timing

### 6.1 The odd-cycle rule

```c
// custom.cpp:69
#define COPPER_CYCLE_POLARITY 1

// custom.cpp:9446
static void generate_copper(void)
{
    int hpos = agnus_hpos;

    if ((hpos & 1) != COPPER_CYCLE_POLARITY) {
        // copper does not advance if hpos bit 0 didn't toggle
        ...
        return;
    }
    ...
}
```

Copper fetches only on odd `agnus_hpos`. This is the "Copper runs on odd
slots only" rule from the HRM. Combined with the 1-CCK delay through the
RGA pipeline, it means a Copper MOVE fully completes 4 CCKs after the
instruction started.

### 6.2 MOVE takes 2 slots (4 CCKs)

```c
// copper state machine
//   COP_read1     -> fetch IR1 (instruction word 0)
//   COP_read2     -> fetch IR2 (instruction word 1)
//   COP_strobe_delay1, COP_strobe_delay2 -> handle COPJMP
//   COP_wait_in2, COP_wait1, COP_wait -> WAIT states
//   COP_skip_in2, COP_skip1, COP_skip  -> SKIP states
```

A Copper MOVE runs:

1. `COP_read1` — fetch instruction word 0 at the next free odd slot
2. `COP_read2` — fetch instruction word 1 at the odd slot after that
3. The `handle_rga_out()` step actually writes the value to the target
   register, one CCK after the second fetch.

Total elapsed time from "Copper requests bus" to "register updated": 4
CCKs. Hence the HRM statement that a Copper MOVE consumes 2 bus cycles
but takes 4 color clocks.

### 6.3 WAIT takes 3 slots (6 CCKs + wake-up)

```c
// custom.cpp:9618
// WAIT: Got IR2, first idle cycle.
// Need free cycle, cycle not allocated.
case COP_wait_in2:
    if (bus_allocated) break;
    cop_state.state = COP_wait1;
    break;

// WAIT: Second idle cycle. Wait until comparison matches.
// Need free cycle, cycle not allocated.
case COP_wait1:
    {
        int comp = coppercomp(hpos, true);
        if (comp < 0) {
            ...
            break;
        }
        if (comp) break;
        if (bus_allocated) break;
        cop_state.state = COP_wait;
    }
    break;

// Wait finished, request IR1.
case COP_wait:
    if (!generate_copper_cycle_if_free(CYCLE_PIPE_COPPER | 0x04)) break;
    cop_state.state = COP_read1;
    break;
```

The 3 phases are `COP_read2` (the original IR2 fetch that made this a
WAIT), `COP_wait_in2` (idle cycle 1), `COP_wait1` (idle cycle 2, compare
beam position), then `COP_wait` which is the actual wake-up slot that
refetches IR1 once the position matches. So from WAIT-instruction-start
to the first fetch of the next instruction: 6 CCKs if the condition is
already true. If the condition isn't true yet, the Copper sits in
`COP_wait1` and keeps re-comparing every CCK (still consuming the cycles
as free, not allocated, so other devices can use them).

The key non-obvious rule: the wake-up cycle itself has to be a **free**
slot. If the slot at wake-up is taken by bitplane DMA, the Copper slips
to the next free odd slot. This is the "Copper wakes up one cycle late
due to DMA contention" phenomenon that breaks some careful timings if
the emulator models it wrong.

### 6.4 SKIP

```c
// custom.cpp:9686
case COP_skip_in2:
    if (bus_allocated) break;
    cop_state.state = COP_skip1;
    break;
case COP_skip1:
    if (bus_allocated) break;
    cop_state.state = COP_skip;
    break;
case COP_skip:
    if (!generate_copper_cycle_if_free(CYCLE_PIPE_COPPER | 0x005)) break;
    if (!coppercomp(hpos, false)) {
        cop_state.ignore_next = 1;
    } else {
        cop_state.ignore_next = -1;
    }
    ...
    cop_state.state = COP_read1;
    break;
```

SKIP has the same 3-phase shape as WAIT (one idle + one idle + one fetch),
but its wake-up cycle is the normal IR1 fetch of the next instruction.
If the beam has already passed the target, `ignore_next = 1` makes the
*next* MOVE a dummy (the bus cycle still happens — it's what the HRM
calls "the skipped MOVE still consumes the slot").

### 6.5 COPJMP semantics

COPJMP1/COPJMP2 are implemented as strobe writes to 0x88/0x8a. They set
`cop_state.strobe` and transition to `COP_strobe_delay_start_odd` or
`COP_strobe_delay_start`. The state machine then burns 2 slots (1fe, 8c,
RGA, 8c — see the comment at blitter.cpp:256) before starting the new
program. This is the source of the "COPJMP takes 4 cycles" rule.

### 6.6 MOVE to BPLCON0 mid-line

The Copper MOVE finishes by doing `custom_wput_copper()` on the target
register. For BPLCON0, that goes through `custom_wput_pipelined()` which
queues the write for 1 CCK later. Therefore a Copper MOVE to BPLCON0 does
*not* take effect on the same CCK it finishes. Combined with the 1-CCK
`bitplane_rga_ptmod()` delay for the bitplane DMA that already had a slot
allocated, the new BPLCON0 value only affects fetches in the next fetch
block — which is why the HRM says "you can't change plane count mid-block
without glitches."

### 6.7 Copper DMA enable edge case

```c
// custom.cpp:3072
static struct rgabuf *generate_copper_cycle_if_free(uae_u16 v)
{
    if (is_copper_dma(true) && check_rga_free_slot_in()) {
        struct rgabuf *rga = write_rga(RGA_SLOT_IN, CYCLE_COPPER, 0x8c, &cop_state.ip);
        ...
        return rga;
    }
    return NULL;
}
```

The Copper checks `check_rga_free_slot_in()` — if `RGA_SLOT_IN` is already
allocated by bitplane DMA, refresh, disk, audio or sprite, the Copper
cannot use it and the state does not advance. This produces the familiar
"Copper gets starved during BPL DMA" behaviour.

---

## 7. Blitter pipeline

### 7.1 The shifter model

WinUAE's blitter does not work from a cycle table. Instead it models the
real blitter's 4-stage shifter and a tiny state machine around it:

```c
// blitter.cpp:96
static bool shifter[4], shifter_out;
static bool shifter_d_armed;
static uae_u32 shifter_d1, shifter_d2, shifter_d_aga;
```

The four shifter stages correspond to the A, B, C and D channels.
`shifter_skip_b` is set when BLTCHB is disabled — the B stage is skipped,
so the shifter is 3 stages. `shifter_skip_y` is set when either BLTCHC or
BLTCHD is disabled (or fill mode needs an extra idle cycle) — the final Y
stage is skipped, making the shifter even shorter.

```c
// blitter.cpp:1147
shifter_skip_y = (blt_info.bltcon0 & (BLTCHD | BLTCHC)) != (BLTCHD | BLTCHC);
// fill mode idle cycle needed? (D enabled but C not enabled)
if (blitfill && (blt_info.bltcon0 & (BLTCHD | BLTCHC)) == BLTCHD) {
    shifter_skip_y = false;
}
```

The cycle count for one word of output is then:

```c
// blitter.cpp:1156
blit_cyclecount = 4 - (shifter_skip_b + shifter_skip_y);
blit_dmacount = ((blt_info.bltcon0 & BLTCHA) ? 1 : 0) +
                ((blt_info.bltcon0 & BLTCHB) ? 1 : 0) +
                ((blt_info.bltcon0 & BLTCHC) ? 1 : 0) +
                (((blt_info.bltcon0 & BLTCHD) && !blitline) ? 1 : 0);
```

So:

| Channels | shifter length | DMA accesses | Free cycles per word |
|---|---|---|---|
| A     (0x8) | 2 | 1 | 1 |
| A+D   (0x9) | 2 | 2 | 0 |
| A+B   (0xC) | 3 | 2 | 1 |
| A+B+D (0xD) | 3 | 3 | 0 |
| A+C   (0xA) | 2 | 2 | 0 |
| A+C+D (0xB) | 3 | 3 | 0 |
| A+B+C (0xE) | 3 | 3 | 0 |
| A+B+C+D (0xF) | 4 | 4 | 0 |
| Fill A+B+D  | 4 (y-stage re-enabled) | 3 | 1 |

"Free cycles" are cycles in the shifter sequence that the blitter doesn't
use for DMA and that fall back to CPU/Copper if they're free.

### 7.2 Channel priority (`get_current_channel`)

```c
// blitter.cpp:1182
static int get_current_channel(void)
{
    if (blitline) { ... }     // see 7.4
    else {
        int nreg = 0x1fe;
        if (shifter[0] && (blt_info.bltcon0 & BLTCHA)) nreg &= 0x74; // A
        if (shifter[1] && (blt_info.bltcon0 & BLTCHB)) nreg &= 0x72; // B
        if (shifter[2] && (blt_info.bltcon0 & BLTCHC)) nreg &= 0x70; // C
        if (nreg == 0x70) return 3; // C
        if (nreg == 0x72) return 2; // B
        if (nreg == 0x74) return 1; // A
        if (nreg != 0x1fe) return 0;
        if (shifter_d_armed && !shifter[0] && (blt_info.bltcon0 & BLTCHD))
            return 4; // D
    }
    return 0;
}
```

The priority chain is A > B > C > D, encoded as an AND of the RGA
register addresses (0x74 is BLTAPTL-ish, etc.). Because only one of the
bits 0x04, 0x02, 0x01 is clear in any given stage address, the final
`nreg` unambiguously identifies the channel. If no shifter stage is
active but the D-arm bit is set, return D.

### 7.3 The main pipeline loop

```c
// blitter.cpp:1717
void generate_blitter(void)
{
    if (!blitter_cycle_exact) return;

    if (get_cycles() == blt_info.finishcycle_copper) {
        blitter_done_notify();
    }

    blitter_next_cycle_always();    // tick D1/D2/D_AGA delay chains regardless

    // fully idle?
    if (!shifter_d_armed && blt_info.blit_count_done && ... all shifter[] clear ...) {
        if (blt_info.blit_queued == 1) {
            blitter_end();
            goto end;
        }
    }

    if (!blt_info.blit_count_done) {
        blt_info.blit_queued = BLITTER_MAX_PIPELINED_CYCLES;    // 4
    }

    if (blt_info.blit_queued) {
        bool ena = blitter_cant_access() == 0;
        bool alloc = check_rga_free_slot_in() == false;
        bool pri = (dmacon & DMA_BLITPRI) != 0;
        bool bstreq = blt_info.nasty_cnt >= BLIT_NASTY_CPU_STEAL_CYCLE_COUNT && !pri;

        // CPU steals the cycle if CPU has waited long enough and current cyle is not free.
        if (!ena || alloc || bstreq) {
            blit_misscyclecounter++;
            ...
            goto end;
        }

        int c = get_current_channel();

        blt_info.blit_queued--;
        if (!blt_info.blit_count_done) {
            blit_cyclecounter++;
            if (blit_cyclecounter == (-CYCLECOUNT_START) + 1) {
                shifter_d_armed = false;
                blt_info.blitzero = 1;
                blt_info.got_cycle = 1;
            }
        }

        ... set reg/p/mod based on channel ...

        bool doddat = blitter_next_cycle(blit_cyclecounter == 0);
        if (doddat) v |= BLITTER_PIPELINE_BLIT;
        ...

        struct rgabuf *rga = write_rga(RGA_SLOT_IN, CYCLE_BLITTER, reg, p);
        rga->bltdat = v;
        rga->bltmod = mod;
        rga->bltadd = blit_add;
        if (idlecycle) rga->alloc = -1;       // don't actually occupy the bus
    }
end:
    maybe_load_mods();
}
```

Key points:

* `blit_queued` starts at `BLITTER_MAX_PIPELINED_CYCLES = 4`. Each CCK
  the blitter gets a bus cycle, `blit_queued--`. This is the pipelined
  cycle window — the blitter never "peeks" more than 4 CCKs into its own
  future.
* `blitter_next_cycle()` (at blitter.cpp:1289) advances the 4-stage
  shifter once. It also handles the A→B→C→D word flow, the
  `shifter_skip_*` renormalisation, and emits `shifter_out` bits that
  drive the D write-back.
* The D write is delayed by the `shifter_d1/d2` two-tick chain — this
  is the "blitter D writes are 2 cycles behind the A/B/C reads" rule.
  In AGA there's an extra 2-CCK delay (`shifter_d_aga`) before `blit_main`
  is cleared, which WinUAE implements as:

```c
// blitter.cpp:1279
if (aga_mode) {
    // AGA 2 CCK delay busy fix
    shifter_d_aga <<= 1;
    shifter_d_aga &= 7;
    if ((shifter_d_aga & (1 << 2)) && blt_info.blit_count_done) {
        blitter_done_all(false);
    }
}
```

### 7.4 Line mode

```c
// blitter.cpp:1184
if (blitline) {
    int nreg = 0x1fe;
    bool lastw = blitter_hcounter + 1 == blt_info.hblitsize;
    if (shifter[0]) {
        if (lastw) return 5;                  // last pixel: special
        if (blt_info.bltcon0 & BLTCHA) nreg &= 0x74;
    }
    if (shifter[1] && (blt_info.bltcon0 & BLTCHB)) nreg &= 0x72;
    if (shifter[2] && (blt_info.bltcon0 & BLTCHC) && !lastw) nreg &= 0x70;
    ...
    // D (C in line mode)
    if (shifter[2] && lastw) {
        if (blt_info.bltcon0 & BLTCHC) {
            if (!shifter[0]) {
                if (blitlinepixel2) return 4;  // write
                return 6;                       // skipped (one-dot mode)
            }
        } else {
            return 6;
        }
    }
}
```

Line mode uses the *same* 4-stage shifter but:

1. The A channel reads are disabled in practice (A pointer is frozen,
   the word that would be read is ignored).
2. The "D" writes actually go through the C pointer (`bltcpt` increments,
   `bltdpt` stays). This is the "linedraw D channel is routed through C
   pointer" oddity mentioned at the top of blitter.cpp:

```c
// blitter.cpp:219
/*
    Oddities:
    - first word is written to address pointed by BLTDPT
      but all following writes go to address pointed by BLTCPT!
      (some kind of internal copy because all bus cycles are
      using normal BLTDDAT)
    - BLTDMOD is ignored by blitter (BLTCMOD is used)
    - state of D-channel enable bit does not matter!
    - disabling A-channel freezes the content of BPLAPT
    - C-channel disabled: nothing is written
*/
```

3. The "one-dot mode" (BLTONEDOT in BLTCON1 bit 1) suppresses the D write
   when the pixel is already set, saving a bus cycle per pixel.
4. Line mode is always 4 cycles per pixel (-C-D pattern): one C read, one
   D write, plus two idle "A" phases. That's constant regardless of
   BLTCON0 channel enables.

### 7.5 Fill mode

Fill mode does not change the channel sequencing but may force the
shifter to re-enable its Y stage (`shifter_skip_y = false`) to give the
fill logic one extra CCK per word. This is the "fill-mode idle cycle"
from the HRM and happens when D is enabled but C is not:

```c
// blitter.cpp:1149
if (blitfill && (blt_info.bltcon0 & (BLTCHD | BLTCHC)) == BLTCHD) {
    shifter_skip_y = false;
}
```

### 7.6 The 4-slot pipeline window

`BLITTER_MAX_PIPELINED_CYCLES = 4` is critical. It means the blitter
never tries to allocate more than 4 CCKs worth of bus cycles in advance.
When the pipeline is full, it sits idle for that CCK — even if there's a
free bus slot — because the real blitter's shifter has only 4 stages and
can't be fed faster than they drain.

This is what produces the classic "idle cycle between blitter ops" that
the HRM describes but never explains.

---

## 8. Blitter vs CPU contention

### 8.1 The nasty counter

```c
// blitter.cpp:76
#define BLIT_NASTY_CPU_STEAL_CYCLE_COUNT 3
```

Every CCK the CPU is waiting for chip RAM while the blitter is active,
`blt_info.nasty_cnt` increments. After 3 CCKs of waiting without BLTPRI
(blitter nasty mode), the CPU is allowed to steal *one* bus cycle from
the blitter:

```c
// blitter.cpp:1746
bool ena = blitter_cant_access() == 0;
bool alloc = check_rga_free_slot_in() == false;
bool pri = (dmacon & DMA_BLITPRI) != 0;
bool bstreq = blt_info.nasty_cnt >= BLIT_NASTY_CPU_STEAL_CYCLE_COUNT && !pri;

// CPU steals the cycle if CPU has waited long enough and current cyle is not free.
if (!ena || alloc || bstreq) {
    blit_misscyclecounter++;
    ...
    goto end;
}
```

If `bstreq` is true, the blitter skips this CCK (the pipeline counter
isn't advanced, but the real blitter would mark it as a missed cycle).
The CPU will then consume that cycle in its own `dma_cycle()` loop.

### 8.2 BLTPRI (blitter nasty)

With `DMACON.BLTPRI` set, `pri = true` so `bstreq` is always false. The
blitter will never yield a CCK to a waiting CPU, and the CPU will stall
for as long as the blitter is working. On the real chip this is what
the programmer uses when they absolutely need the blitter to finish in a
fixed number of CCKs.

```c
// blitter.cpp:2095
unset_special (SPCFLAG_BLTNASTY);
if (dmaen(DMA_BLITPRI)) {
    set_special(SPCFLAG_BLTNASTY);
}
```

`SPCFLAG_BLTNASTY` is used outside the blitter to force the CPU to check
for extra stalls in its main instruction dispatch loop.

### 8.3 Blitter vs bitplane DMA

```c
// blitter.cpp:1746
bool alloc = check_rga_free_slot_in() == false;
```

`check_rga_free_slot_in()` returns true if the upcoming RGA slot is
already claimed by someone else (refresh, disk, audio, sprite, bitplane
DMA). If so, `alloc == true` and the blitter skips the CCK. The blitter
can therefore *never* steal a slot that the bitplane sequencer wants —
bitplane has strict priority.

This is why "6 hires planes lores" or "8 lores planes" have almost no
blitter bandwidth during the bitplane region.

### 8.4 Blitter vs Copper contention

Copper uses `generate_copper_cycle_if_free(...)` which calls the same
`check_rga_free_slot_in()`. Copper and blitter both compete for the same
free slots. The tiebreaker is execution order: `generate_dma_requests()`
calls `generate_copper()` *before* `generate_blitter()` (custom.cpp:12340),
so Copper gets the free slot first. Blitter only takes what Copper doesn't.

A common symptom of this: a tight Copper loop (MOVE after MOVE) can
starve the blitter completely, which is why `BLTCHA/B/C/D` blits used for
"background fills" tend to take slightly longer when a large Copper list
is running.

### 8.5 Copper write conflict with blitter pointer register

The Copper can write BLTAPT/BPT/CPT/DPT mid-blit. If it does so at the
exact CCK the blitter is consuming that pointer, you get an OR-style
corruption on the bus. WinUAE models this:

```c
// blitter.cpp:1809
if (c >= 1 && c <= 4) {
    evt_t cycs = get_cycles();
    if (cycs == blt_info.blt_ch_cycles[c - 1]) {
        blt_info.blt_ch_cycles[c - 1] = 0;
        switch (c) {
            case 1: blt_info.bltapt = blt_info.bltapt_prev; break;
            case 2: blt_info.bltbpt = blt_info.bltbpt_prev; break;
            case 3: blt_info.bltcpt = blt_info.bltcpt_prev; break;
            case 4: blt_info.bltdpt = blt_info.bltdpt_prev; break;
        }
    }
}
```

The real chip gives a slightly different result (the two addresses OR
together); WinUAE restores the previous value, which is close enough for
all practical purposes and avoids corrupting the host buffer.

---

## 9. CPU contention model

### 9.1 The bank type table

```c
// include/memory.h:203
#define CE_MEMBANK_FAST32 0
#define CE_MEMBANK_CHIP16 1
#define CE_MEMBANK_CHIP32 2
#define CE_MEMBANK_CIA    3
#define CE_MEMBANK_FAST16 4
```

`ce_banktype[addr >> 16]` classifies each 64 KB of address space into
one of these buckets. `fill_ce_banks()` in memory.cpp:2745 sets it up:

* 0x000000..0x1fffff — CHIP16 (OCS/ECS) or CHIP32 (AGA/A3000)
* 0xa00000..0xbfffff — CIA
* 0xc00000..0xcfffff — CHIP16 (ranger/slow RAM; this is where the classic
  "slow fast ram" lives)
* 0xd00000..0xdfffff — CHIP16 (custom chips)
* Z2 fastmem, Z3 — FAST16 or FAST32
* 0xe00000..0xffffff — ROM, CHIP16 or CHIP32 depending on `cs_romisslow`

### 9.2 The access functions

```c
// newcpu.cpp:8008
uae_u32 mem_access_delay_word_read (uaecptr addr)
{
    uae_u32 v;
    switch (ce_banktype[addr >> 16]) {
    case CE_MEMBANK_CHIP16:
    case CE_MEMBANK_CHIP32:
        v = wait_cpu_cycle_read (addr, 1);
        break;
    case CE_MEMBANK_FAST16:
    case CE_MEMBANK_FAST32:
        v = get_word (addr);
        x_do_cycles_post (4 * cpucycleunit, v);
        break;
    default:
        v = get_word (addr);
        break;
    }
    regs.db = v;
    regs.read_buffer = v;
    return v;
}
```

Three regimes:

1. **Chip RAM / custom chip registers** (CHIP16, CHIP32): go through
   `wait_cpu_cycle_read()` / `wait_cpu_cycle_write()` which run the chip
   DMA arbiter until a free slot is available.
2. **Fast RAM** (FAST16, FAST32): read immediately, then burn 4 CPU
   cycles (`4 * cpucycleunit = 2 * CYCLE_UNIT`, i.e. 2 CCKs = one 68000
   word cycle). This is a "perfect" memory bus with no arbitration.
3. **CIA, ROM, unmapped**: read immediately with whatever wait state the
   individual bank defines (see cia.cpp for CIA; it uses `cia_wait_pre` /
   `cia_wait_post`, see §10).

### 9.3 The chip-bus arbiter: `wait_cpu_cycle_read`

```c
// custom.cpp:12598
uae_u32 wait_cpu_cycle_read(uaecptr addr, int mode)
{
    uae_u32 v = 0, vd = 0;
    int ipl = regs.ipl[0];
    evt_t now = get_cycles();

    sync_cycles();                    // align to CYCLE_UNIT boundary
    x_do_cycles_pre(CYCLE_UNIT);      // pay the "wait up to 1 CCK" pre-cost

    dma_cycle(&mode, &ipl);           // <-- here

    ...
    switch (mode) {
        case -1: v = vd = get_long(addr); break;
        case  1: v = vd = get_word(addr); break;
        case  0: v = vd = get_word(addr & ~1);
                 v >>= (addr & 1) ? 0 : 8; break;
        ...
    }

    x_do_cycles_post(CYCLE_UNIT, 0);
    regs.chipset_latch_rw = regs.chipset_latch_read = v;
    return v;
}
```

And `dma_cycle()`:

```c
// custom.cpp:12556
static int dma_cycle(int *mode, int *ipl)
{
    if (cpu_tracer < 0) return current_hpos_safe();
    if (!currprefs.cpu_memory_cycle_exact) return current_hpos_safe();
    blt_info.nasty_cnt = 0;
    while (currprefs.cpu_memory_cycle_exact) {
        struct rgabuf *r = read_rga_out();
        if (r->alloc <= 0 || quit_program > 0) break;
        blt_info.nasty_cnt++;
        *ipl = regs.ipl_pin;
        do_cck(true);
        /* bus was allocated to dma channel, wait for next cycle.. */
    }
    blt_info.nasty_cnt = 0;
    return agnus_hpos;
}
```

The loop: while the current outgoing slot is allocated (by anything —
refresh, disk, audio, sprite, bitplane, copper, blitter), advance one CCK
and try again. Each trip increments `nasty_cnt`, which is how the blitter
knows the CPU has been starved for `BLIT_NASTY_CPU_STEAL_CYCLE_COUNT` CCKs.

A striking consequence: **even with a full 6-plane lores display consuming
75% of CCKs in the bitplane region, the CPU is never locked out.** It
still gets the free 2 CCKs per 8-CCK block (25%), and if the blitter is
also active, the nasty-cnt mechanism guarantees the CPU gets one cycle
every 3 stalled CCKs (unless BLTPRI is set). Average CPU chip-RAM
bandwidth on a 6-plane lores display is ≈ 0.7 MB/s instead of the ≈ 2 MB/s
you get on a blank line.

### 9.4 Slow RAM ($C00000) contention

Slow RAM (ranger / bogomem at 0xc00000..0xd80000) is marked CHIP16 in the
bank table:

```c
// memory.cpp:2761
for (i = (0xd00000 >> 16); i < (0xe00000 >> 16); i++) {
    ce_banktype[i] = CE_MEMBANK_CHIP16;
}
```

The A501 / A502 "Slow RAM" expansion is wired to the chip bus, so
accesses go through the same `dma_cycle()` arbiter as real chip RAM. The
result: programs that put data in slow RAM suffer the same bitplane
contention as if the data were in chip RAM, even though they cannot be
accessed by DMA. This is why "slow" is slow, and why it's never worth
installing it if you can install real Z2 fastmem instead.

### 9.5 Fast RAM (Zorro II)

```c
case CE_MEMBANK_FAST16:
case CE_MEMBANK_FAST32:
    v = get_word (addr);
    x_do_cycles_post (4 * cpucycleunit, v);
    break;
```

Zorro II fast RAM has a fixed 4-cycle word access time — i.e. one
68000 bus cycle — with no arbitration. This is modelled by just adding
the 4-CPU-cycle delay and doing the read immediately. There is no
interaction with the Agnus DMA arbiter.

Note that `FAST16` (Zorro II / A500+ on-board expansion / most CDTV slots)
and `FAST32` (Zorro III, A3000 built-in, A4000 built-in) have the *same*
timing in WinUAE's cycle-exact mode. The 32-bit variants only matter for
68030/040/060 where the wide bus halves the cycle count for long reads,
which is handled by the `cpucycleunit` scaling.

### 9.6 AGA 32-bit chip RAM

AGA systems have a 32-bit chip RAM bus (`CHIP32`). The access function is
the same — `wait_cpu_cycle_read()` — but the underlying chipset can
transfer 32 bits per slot, so a 68020/030 longword read from chip RAM
takes one CCK instead of two. This is reflected in `fetchmode_bytes`
being 4 or 8 (§4.6) and in the AGA `chipmem_bank_ce2` variants at
memory.cpp:582.

### 9.7 68020/030 `do_cycles_ce020`

```c
// custom.cpp:12825
void do_cycles_ce020(int cycles)
{
    evt_t cc;
    static int extra;

    cycles += extra;
    extra = 0;
    if (!cycles) return;
    cc = get_cycles();
    while (cycles >= CYCLE_UNIT) {
        do_cck(true);
        cycles -= CYCLE_UNIT;
    }
    ...
}
```

The 68020 CE path is slightly different from 68000 CE in that it has a
"leftover cycles" carryover (`extra`) — the 68020 internal pipeline can
finish instructions in fractional CCKs. The underlying `do_cck()` is the
same.

---

## 10. CIA timing

### 10.1 E-clock fundamentals

The CIA chips (8520) use the 68000 E-clock output as their main clock.
The E-clock runs at CPU / 10 = 709 kHz, and it has a 4-cycles-high /
6-cycles-low duty cycle. Every CIA bus access has to sync to the E-clock
before the data transfer happens, then hold the address for 4 clocks of
E-high and run 6 clocks of E-low for the actual access, for a total of 10
E-clocks worst case (≈ 1.41 μs on a 7.09 MHz system, just over 5 CCKs).

```c
// cia.cpp:105
#define E_CLOCK_SYNC_N  2
#define E_CLOCK_START_N 4
#define E_CLOCK_END_N   6
#define E_CLOCK_TOD_N   -2

#define E_CLOCK_LENGTH 10
#define E_CYCLE_UNIT (CYCLE_UNIT / 2)
#define DIV10 (E_CLOCK_LENGTH * E_CYCLE_UNIT)   /* 50 CYCLE_UNIT ticks == 5 CCKs */
```

One E-clock = `DIV10 = 50 CYCLE_UNIT ticks = 5 CCKs`.

### 10.2 The 4-cycle E-clock sync

```c
// cia.cpp:771
static int get_cia_sync_cycles(int *syncdelay)
{
    evt_t c = get_e_cycles();
    int div10 = c % DIV10;
    int add = 0;
    int synccycle = e_clock_sync * E_CYCLE_UNIT;
    if (div10 < synccycle) {
        add += synccycle - div10;
    } else if (div10 > synccycle) {
        add += DIV10 - div10;
        add += synccycle;
    }
    *syncdelay = add;
    // 4 first cycles of E-clock
    add = e_clock_start * E_CYCLE_UNIT;
    return add;
}
```

The "sync cycle" is position 2 of the 10-slot E-clock cycle. If the CPU
is not at position 2 when it starts a CIA access, it stalls until the
next E-clock sync point, then pays `e_clock_start * E_CYCLE_UNIT = 4 *
E_CYCLE_UNIT = 2 CCKs` to enter the access, then (after the actual data
operation) pays `e_clock_end * E_CYCLE_UNIT = 6 * E_CYCLE_UNIT = 3 CCKs`
for the tail (`cia_wait_post` at cia.cpp:2482).

Net result: a worst-case CIA read from an arbitrary CPU phase takes up to
`9 E-clock cycles = 4.5 CCKs` of sync wait + `2 CCKs` start + `3 CCKs` end
= **9.5 CCKs** (≈ 2.68 μs). Best case (CPU already aligned) is 2+3 = 5
CCKs (≈ 1.41 μs).

The "2.5 E-clocks" figure in the HRM averages this out assuming uniformly
distributed phase alignment.

### 10.3 TOD

The TOD counter is an external 60 Hz or 50 Hz tick fed from the PSU AC
ripple (VSYNC on CIA-B). WinUAE simulates it via
`event_CIA_tod_inc_event` scheduled once per VBL, with a 12-E-clock post
delay to match the 8520 internal pipeline:

```c
// cia.cpp:1063
#define TOD_INC_DELAY (12 * E_CLOCK_LENGTH / 2)

evt_t c = get_e_cycles() + 6 * E_CYCLE_UNIT + hoff * CYCLE_UNIT;
...
int unit = (E_CLOCK_LENGTH * 4) / 2; // 4 E-clocks
```

The TOD has a well-known bug: incrementing from 0x00?F to 0x00?0 takes
extra time through the BCD carry logic. WinUAE's `checkalarm()` replicates
this bug when `cs_ciatodbug` is set.

### 10.4 ICR read interlock

Reading ICR clears pending interrupts. If a new interrupt arrives on the
*same E-clock* as the ICR read, it must still be visible. WinUAE handles
this via `CIA_sync_interrupt()` which checks whether the new interrupt is
arriving in the "forbidden" window and, if so, schedules it via
`event2_newevent_xx(-1, DIV10 + delay, num, event_CIA_synced_interrupt)`
(cia.cpp:811). The event fires at the next E-clock boundary, guaranteeing
the new bit becomes visible on a read *after* the clear.

```c
// cia.cpp:795
static void CIA_sync_interrupt(int num, uae_u8 icr)
{
    struct CIA *c = &cia[num];

    if (acc_mode()) {
        if (!(icr & c->imask)) {
            c->icr1 |= icr;
            return;
        }
        c->icr2 |= icr;
        if ((c->icr1 & ICR_MASK) == (c->icr2 & ICR_MASK)) {
            return;
        }
        int syncdelay = 0;
        int delay = get_cia_sync_cycles(&syncdelay);
        delay += syncdelay;
        event2_newevent_xx(-1, DIV10 + delay, num, event_CIA_synced_interrupt);
    } else {
        c->icr1 |= icr;
        CIA_check_ICR();
    }
}
```

This is the "ICR destructive-read atomic" mentioned in the HRM. Ignoring
it causes games that use `CIAA-ICR` for keyboard handshake to lose keys.

### 10.5 Timer cascade (CRA/CRB INMODE bits)

CIA timer A can be clocked by:

* PHI2 (CNT pin default = CIA input clock = E-clock / 1 but WinUAE
  calls it via `DIV10 = 5 CCKs` on `CIA_update` ticks)
* The CNT pin (external input)
* Timer A underflow (cascade)
* CNT-gated PHI2

Timer B similarly. The mode bits are read from `c->t[N].cr` in
`CIA_update()` and the new timer value is computed from the elapsed E-clocks:

```c
// cia.cpp:398
uae_u32 ciaclocks = (uae_u32)ccount / DIV10;
```

`ccount` is the elapsed cycle count since last update; dividing by
`DIV10` gives whole E-clocks. The timer counts down once per E-clock in
PHI2 mode.

Cascade (timer B input mode = timer A underflow) is tested every time
`CIA_update()` runs. If timer A underflowed during the elapsed interval,
timer B's decrement is adjusted accordingly. This is what makes
`16+16 = 32-bit` 100 Hz timers work for games that use them for music
tempo.

### 10.6 CIA pipe-in delay

The 8520 has a 1-E-clock pipeline delay on writing CRA/CRB force-load,
which WinUAE calls `c->t[N].loaddelay` and checks in the timer update
loop. This is why `LOAD` via CRA causes the timer to reset to the latch
value on the *next* E-clock, not the current one.

---

## 11. Audio DMA pipeline

### 11.1 Channel states

Paula's audio state machine is 6 states numbered 0..5 in the hardware but
the 8364 internally indexes them as:

| HRM name | WinUAE | Description |
|---|---|---|
| IDLE     | 0 | Channel off, waiting for DMA enable or manual AUDxDAT write |
| WAITDMA  | 1 | DMA enabled, waiting for first data word and AUDxLC load |
| (preload)| 5 | First word fetched, load period, enter output phase |
| HIGHDMA  | 2 | Outputting high byte of `dat2`, requesting next DMA low |
| LOWDMA   | 3 | Outputting low byte of `dat2`, requesting next DMA high |

```c
// audio.cpp:1806
switch (cdp->state)
{
case 0:
    if (chan_ena) {
        cdp->evtime = MAX_EV;
        cdp->state = 1;
        setdr(nr, true);                 // "data request" to DMA logic
        cdp->wlen = cdp->len;
        cdp->ptx_written = false;
        if (cdp->wlen > 2) cdp->ptx_tofetch = true;
        cdp->dsr = true;
        if (cdp->intreq2) {
            setirq(nr, 0);
            cdp->intreq2 = false;
        }
        ...
    } else if (cdp->dat_written && !isirq(nr)) {
        cdp->state = 2;                 // CPU-initiated: skip to HIGHDMA
        setirq(nr, 1);
        loaddat(nr);
        ...
    }
    break;

case 1:
    cdp->evtime = MAX_EV;
    if (!chan_ena) { zerostate(nr, false); return true; }
    if (!cdp->dat_written) return true;
    setirq(nr, 10);                     // first interrupt
    setdr(nr, false);
    if (cdp->wlen != 1) cdp->wlen = (cdp->wlen - 1) & 0xffff;
    cdp->state = 5;
    ...
    break;

case 5:
    ...
    if (cdp->ptx_written) {
        cdp->ptx_written = 0;
        cdp->lc = cdp->ptx;
    }
    loaddat(nr);
    if (napnav) setdr(nr, false);
    cdp->state = 2;
    loadper(nr);
    cdp->pbufldl = true;
    cdp->volcnt = 0;
    audio_state_channel2(nr, false);
    break;

case 2:
    if (cdp->pbufldl) {
        newsample(nr, (cdp->dat2 >> 8) & 0xff);
        loadper(nr);
        cdp->pbufldl = false;
    }
    if (!perfin) return true;
    if (audap) loaddat(nr, true);       // attached channel modulation load
    if (chan_ena) {
        if (audap) setdr(nr, false);
        if (cdp->intreq2 && audap) { setirq(nr, 21); cdp->intreq2 = false; }
    } else {
        if (audap) setirq(nr, 22);
    }
    cdp->pbufldl = true;
    cdp->state = 3;
    break;

case 3:
    if (cdp->pbufldl) {
        newsample(nr, (cdp->dat2 >> 0) & 0xff);
        if (chan_ena) loadper(nr); else loadperm1(nr);
        cdp->pbufldl = false;
    }
    if (!perfin) return true;
    if (chan_ena) {
        loaddat(nr);
        if (napnav) setdr(nr, false);
        if (cdp->intreq2 && napnav) { setirq(nr, 31); cdp->intreq2 = false; }
    } else {
        ...
        if (napnav) setirq(nr, 32);
        ...
    }
    cdp->pbufldl = true;
    cdp->state = 2;
    break;
```

### 11.2 The "throw away first word" rule

When DMA starts:

1. State 0 → 1: `setdr()` asserts the DMA request line. The first word
   fetched by the next AUDxDMA slot goes into `dat1`.
2. State 1 → 5: On the next period expiration, move `dat1` → `dat2`,
   start the actual output. The word stored in state 1 was essentially
   "discarded" by the real chip because it never reached `dat2`.

This is why audio.device always writes a `0x0000` at `AUDxLC+0` before
starting — the first sample is a throw-away and if you put real data
there you get a click. The WinUAE code at state 1 does exactly this:
fetches the word, sets IRQ "10" (debug marker), decrements `wlen` but
does *not* call `newsample()`.

### 11.3 Period counter at CCK rate

The period counter runs at CCK rate. Every CCK the audio dispatcher
decrements `cdp->evtime` and if it reaches zero, calls
`audio_state_channel(nr, true /* perfin */)` to advance the state machine.

The minimum period is 124 (PAL, one sample per 124 CCKs ≈ 28.867 kHz
sample rate). Below that, Paula reads data from chip RAM faster than one
DMA slot per line can supply — the documented "period < 124 causes
glitches" behaviour.

### 11.4 AUDxVOL and AUDxPER modulation (ADKCON)

The Amiga supports "attached" channels for simple 2-channel FM / AM
synthesis. If `audav` (bit in ADKCON) is set, channel N+1's samples are
interpreted as **volume** for channel N — `audio_state_channel2()` takes
them from `dat2 & 0xff` and stuffs them into `cdp->vol`. If `audap` is
set, channel N+1 provides **period** words. When `audav` or `audap` is
set for channel N, channel N becomes silent — it's just feeding N-1's
volume or period latch.

WinUAE handles this with the `napnav` flag (`!audav && !audap || audav`):

```c
// audio.cpp:1753
int napnav = (!audav && !audap) || audav;
```

This means: "this channel outputs a sample if it's a normal channel or a
volume-modulation source."

### 11.5 Period underflow re-latch

When the period counter underflows, AUDxPER is re-latched from the
internal period register. But if a write to AUDxPER happened since the
last underflow, the new value is used. WinUAE does this in `loadper()`:

```c
// audio.cpp (via loadper)
cdp->evtime = cdp->per;
```

Where `cdp->per` is the last written period value. The "new period takes
effect at next underflow" rule is automatically enforced because we only
read `cdp->per` at underflow time.

### 11.6 DMA request timing

`setdr(nr, false)` clears the DMA request after the slot fills. This is
why the DMA slot scheduler only processes an audio DMA request when the
channel actually needs another word, rather than on every line. The
slot is free for CPU / blitter otherwise.

---

## 12. Disk DMA

### 12.1 DSKLEN double-write

```c
// disk.cpp:3995
if ((dsklen & 0xc000) == 0x4000) {
    ... start DMA ...
}
```

Writing to DSKLEN with the DMA-start bit (0x8000) clear first is a no-op;
then writing again with the DMA-start bit set actually begins the
transfer. This is the famous "write DSKLEN twice to start" rule. The
real hardware has no such requirement — it's a convention enforced by
trackdisk.device to prevent runaway DMA on a crashed kernel. WinUAE
mimics it for compatibility.

### 12.2 Two DMA slots per line

```c
// custom.cpp:10241
#define DMAL_DSK0 (1 << 5)
#define DMAL_DSK1 (1 << 6)
#define DMAL_DSK2 (1 << 7)
```

There are actually **three** disk DMA slots per line (DSK0, DSK1, DSK2),
but only the first two are used in normal 16-bit MFM mode. DSK2 is the
"high-density" slot and is only populated on HD drives. Disk DMA runs
at one word per slot, so a standard-density floppy track read takes 512
slots × 2 per line × ~280 ms = about 11000 CCKs, or ~45 scan lines, for
the FIFO to drain. In practice the entire track transfer is controlled
by MFM decoding speed rather than DMA bandwidth.

### 12.3 DSKSYNC matching

```c
// disk.cpp:4290
// wordsync interrupt is inhibited if DSKLEN write bit is set
if (!(dsklen & 0x4000)) {
    ...
}
```

DSKSYNC contains the 16-bit MFM pattern to look for (usually 0x4489, the
IBM MFM sector address mark). When the MFM decoder sees this pattern on
the incoming bit stream, it raises the DSKSYNC interrupt (INT7 bit 12).
The bit-by-bit MFM match is done in `disk_update()` — not in the DMA
path. The DMA path only starts writing words to chip RAM once the DMA
enable bit is set and the MFM decoder has synced.

### 12.4 Track read pacing

Disk DMA is entirely paced by the MFM decoder, not by DMA slots:

```c
// disk.cpp:4256
if (dmaen(DMA_DISK) && bitoffset == 15 && dma_enable && dskdmaen == DSKDMA_READ && dsklength >= 0) {
    if (dsklength > 0) {
        ...
        dsklength--;
        ...
    }
}
```

One word is fetched every 16 MFM bit cells — i.e. every 32 μs on a DD
drive. That's well under one word per scan line, so the DMA FIFO never
fills and the disk slot pattern in DMAL is mostly idle. The exception is
write mode where the host has to feed the FIFO fast enough to maintain
the MFM stream.

---

## 13. Refresh DMA

The 4 refresh slots (DMAL_REFRESH0..3) are the first 4 even slots of every
line. They fire at `agnus_hpos` 0x00, 0x02, 0x04, 0x06.

```c
// custom.cpp:10334
if (dmal_shifter & (DMAL_REFRESH0 | DMAL_REFRESH1 | DMAL_REFRESH2 | DMAL_REFRESH3)) {
    if (dmal_shifter & DMAL_REFRESH0) {
        uae_u16 reg = get_strobe_reg(0);        // 0x38 / 0x3a / 0x3c
        ...
    }
    if (dmal_shifter & DMAL_REFRESH1) {
        uae_u16 reg = get_strobe_reg(1);
        ...
    }
    if (dmal_shifter & DMAL_REFRESH2) {
        write_rga(RGA_SLOT_IN, CYCLE_REFRESH, 0x1fe, &refptr);
    }
    if (dmal_shifter & DMAL_REFRESH3) {
        write_rga(RGA_SLOT_IN, CYCLE_REFRESH, 0x1fe, &refptr);
    }
}
```

The refresh pointer `refptr` is a 10-bit RAS counter that increments once
per refresh slot. On OCS it adds 2 per step (`REF_RAS_ADD_OCS = 0x002`),
on ECS 0x200 per step, on AGA 0. The exact value doesn't matter for
software, but the slots are unconditionally consumed — they cannot be
used by *anything* else. Not CPU, not copper, not blitter.

The consequence: out of every line's 227 CCKs, 4 are permanently gone.
Together with the HSYNC / HBLANK period (roughly 51 CCKs from 0x0f through
0x34 in the traditional layout), your maximum "free" bandwidth on a blank
line is about 227 - 4 - 51 = 172 CCKs. On a 6-plane lores line, subtract
another 48 CCKs (6 planes × 8 blocks × 1 slot/block × ... actually, 8
blocks × 6 slots/block = 48), and the free bandwidth is reduced to about
124 CCKs.

Slots REFRESH0 and REFRESH1 also carry strobe register writes for the
Denise line-type latch:

| Line type | `get_strobe_reg(0)` | Notes |
|---|---|---|
| Normal visible | 0x38 (STRHOR) | Horizontal strobe |
| Vertical blank | 0x3a (STREQU) | "Equalisation" line (in VSYNC front/back porch) |
| Long field VSYNC | 0x3c (STRVBL) | The VSYNC line itself |
| Short field VSYNC | 0x38 (STRHOR) | Short VSYNC still uses STRHOR |
| Long line extra | 0x3e (STRLONG) | The extra CCK on a long NTSC line |

The VERTB interrupt is generated when `prev_strobe == 0x3c` and the
current strobe isn't 0x3c — i.e. on the transition out of STRVBL back to
STRHOR:

```c
// custom.cpp:12086
// VERTB = STRHOR -> !STRHOR
if (prev_strobe == 0x3c && r->reg != 0x3c) {
    INTREQ_INT(5, 0);
}
prev_strobe = r->reg;
```

---

## 14. Event queue mechanism

### 14.1 The two event tables

```c
// include/events.h:69
enum {
    ev_sync, ev_cia, ev_misc, ev_audio, ev_max
};
enum {
    ev2_blitter, ev2_misc, ev2_max = 16
};

extern struct ev  eventtab[ev_max];
extern struct ev2 eventtab2[ev2_max];
```

WinUAE has two kinds of events:

* **`eventtab[]`** (4 entries): high-priority events that need a dedicated
  slot — HSYNC (`ev_sync`), CIA timer (`ev_cia`), audio mixer (`ev_audio`),
  misc (`ev_misc`). Scheduled via `event2_newevent`-less direct writes.
* **`eventtab2[]`** (16 entries): generic queue for one-shot events like
  blitter done, AUDxDAT delayed fire, CIA synced interrupt, disk events,
  etc.

### 14.2 The scheduling loop

```c
// events.cpp:54
void events_schedule(void)
{
    evt_t mintime = EVT_MAX;
    for (int i = 0; i < ev_max; i++) {
        if (eventtab[i].active) {
            evt_t eventtime = eventtab[i].evtime - currcycle;
            if (eventtime < mintime)
                mintime = eventtime;
        }
    }
    if (mintime < EVT_MAX) {
        nextevent = currcycle + mintime;
    } else {
        nextevent = EVT_MAX;
    }
}
```

`nextevent` is the absolute cycle at which the soonest scheduled event
will fire. The main execution loop runs `do_cycles()` in chunks up to
`nextevent - currcycle` at a time, then services the expired event.

### 14.3 `event2_newevent_xx` and friends

```c
// events.h:153
extern void event2_newevent_xx(int no, evt_t t, uae_u32 data, evfunc2 func);
extern void event2_newevent_x_replace(evt_t t, uae_u32 data, evfunc2 func);
extern void event2_newevent_x_add_not_exists(evt_t t, uae_u32 data, evfunc2 func);
extern void event2_newevent_x_remove(evfunc2 func);
```

`event2_newevent_xx` is the main "schedule this handler for T cycles from
now" function. It's used for:

* Blitter completion (`ev2_blitter` slot)
* CIA interrupt re-delivery after ICR sync
* Delayed AUDxDAT writes (1-CCK delay)
* Fake HSYNC when chipset is idle

The handlers take a `uae_u32 data` argument so you can pass a small
payload (usually a register ID or channel number) without allocating
anything.

### 14.4 Pros and cons for emulator design

The event-queue approach gives you O(1) dispatch for infrequent events
and allows long idle periods to be collapsed into a single `do_cycles()`
call. The downside is that every per-CCK action (bitplane fetch, sprite
DMA, copper state) still has to be a per-CCK poll, not an event — because
the cost of scheduling 227 events per line is too high.

WinUAE's compromise is: **event-queue for low-frequency things, per-CCK
loop for chipset DMA**. A new emulator should do the same. Trying to be
pure event-driven all the way down means writing special cases for every
possible DMA conflict, which the real chip does not do.

### 14.5 The `currcycle_cck` shortcut

```c
// events.cpp:29
uae_u32 currcycle_cck;

// events.h:119
STATIC_INLINE uae_u32 get_cck_cycles(void)
{
    return currcycle_cck;
}
```

This is a 32-bit CCK-count running alongside `currcycle`. It wraps every
~20 minutes of emulated time but it's used only for per-line comparisons
(`agnus_trigger_cck = get_cck_cycles()`) so the wrap is harmless. Having
both a 64-bit raw-cycle and a 32-bit CCK value avoids a division every
time the code needs to ask "how many CCKs since HSYNC?".

---

## 15. Wait-state table

This is condensed for quick reference. All values are in CCKs (1 CCK =
280 ns PAL, 279.37 ns NTSC).

| Access | Best | Worst (bitplane contention) | BLTPRI (nasty) | Notes |
|---|---|---|---|---|
| Chip RAM word read (0x000000–0x1fffff) | 2 | unbounded | blocked until blitter done | Arbitrated via `dma_cycle()`; CPU gets one CCK every 3 stalls if !BLTPRI |
| Chip RAM long read | 4 | unbounded | blocked | 2 × word read |
| Slow RAM word read (0xc00000–0xd7ffff) | 2 | unbounded | blocked | Same bus as chip RAM; no DMA to/from slow RAM but contention applies |
| Z2 Fast RAM word read (0x200000–0x9fffff) | 2 (4 CPU cycles) | 2 | 2 | Private bus, no contention |
| Z3 Fast RAM word read | 2 | 2 | 2 | Same behaviour in WinUAE; real Z3 can be faster on 68030+ |
| ROM read (0xf80000–0xffffff) | 2 | 2 | 2 | Unless `cs_romisslow` — then CHIP16 |
| Custom chip reg read (0xdff000, e.g. VHPOSR) | 2 | varies | varies | Goes through chip bus; treats as CPU chip access |
| Custom chip reg write (e.g. BPLCON0) | 2 | varies | varies | Many registers are pipelined 1-CCK via `custom_wput_pipelined` |
| CIA read (0xbfe001, 0xbfd000) | 5 | 9.5 | 9.5 | 2 (sync wait avg) + 3 (4 E-clock start) + 2.5 (6 E-clock end / 2) / 2 ≈ 5 avg best, 9.5 worst |
| CIA write | 5 | 9.5 | 9.5 | Same as read |

### 15.1 Notes on "best" and "worst"

"Best" assumes zero arbitration stall — the bus is already free at the
exact CCK the CPU issues the access. "Worst" includes the maximum number
of DMA slots the CPU can be stalled behind: for chip RAM, this can
theoretically be 227 CCKs if the entire next line is full of bitplane DMA
+ blitter-nasty, though in practice the blit-nasty break-even means it
rarely exceeds ~12 CCKs.

### 15.2 CPU instruction timing context

These figures combine with 68000 instruction cycles. A simple
`MOVE.W (A0)+,D0` is:

* 4 CPU cycles to fetch the opcode (chip or fast RAM, word read)
* 4 CPU cycles to fetch the operand (chip RAM word read)
* = 8 CPU cycles = 4 CCKs = ~1.13 μs on a clean line

On a contended 6-plane-lores line the same instruction takes ~6-10 CCKs
because the chip RAM read may be stalled. On a blitter-nasty line with
BLTPRI it can stall for many more CCKs if the blitter is mid-block.

### 15.3 Word vs long access timing

WinUAE's `mem_access_delay_long_read_ce020` (used by 68020+ CE) handles
long accesses via two word accesses for 16-bit chip RAM (CHIP16) or one
long for 32-bit chip RAM (CHIP32). The cost for long read:

| Bus width | Chip bus | Fast bus |
|---|---|---|
| CHIP16 long read | 2 × (2 CCK arbitrated) | n/a |
| CHIP32 long read | 1 × (2 CCK arbitrated) | n/a |
| FAST16 long read | 2 × 4 CPU cycles (= 4 CCKs total) | free of contention |
| FAST32 long read | 1 × 4 CPU cycles (= 2 CCKs total) | free of contention |

So AGA chip RAM is just as fast as AGA fast RAM for 68020 long accesses
when there is no contention, but on a contended bitplane line, AGA chip
RAM becomes much slower than fast RAM.

---

## 16. Implementation notes for a new emulator

If you are writing a new cycle-accurate Amiga emulator, here is the
distilled checklist. Each item maps to a specific section above.

1. **Drive everything from one CCK tick.** Do not try to run the CPU on
   its own clock and sync to the chipset later. WinUAE's `do_cck()` is
   the shape you want: fetch out, decide in, advance, shift. See §1.1.

2. **The RGA pipeline is not optional.** Bitplane DMA has a 1-CCK
   address-latch delay. If you fetch the pointer in the same CCK you
   decide the slot, you will break several demos that rely on a
   Copper-write-to-BPLnPTH landing before or after the slot. WinUAE's
   3-entry pipeline is the minimum faithful model. See §1.1.

3. **Build the `cycle_diagram_table` at init.** Do not compute "plane 4
   fires on CCK N modulo 8" at runtime. Precompute the 32-entry slot map
   for every `[fetchmode][res][planes]` combination and index it. This
   is how the real Agnus works (it's a 512-entry ROM inside the chip)
   and it's the only correct way to handle "5 planes lores = zero
   planes" and similar. See §4.2.

4. **BPL1DAT is the shifter trigger, not BPLCON0.** Denise does not
   latch any of the BPLxDAT values until something (DMA or CPU) writes
   to BPL1DAT. If you decrease the plane count mid-line, don't clear
   the `bplxdat[]` slots — let them keep shifting. See §4.5.

5. **DMAL is a shift register, not a schedule.** The DMAL start-of-line
   "bit walks right, fires each channel in turn" model lets you model
   sprite-vs-bitplane conflicts, audio-channel-idle savings, and disk
   FIFO backpressure without any special-case code. See §2.1.

6. **Blitter cycle count is `4 − skip_b − skip_y`, not a 16-entry
   table.** The HRM tables are correct but incomplete — they don't
   handle A-only blits, line mode, or fill mode's extra idle cycle.
   Model the 4-stage shifter directly. See §7.1.

7. **Blitter D writes are 2 CCKs late.** On AGA they are 2 + 2 = 4 CCKs
   late before `blit_main` clears. This affects the "wait for blitter
   done" loop that Kickstart 1.2+ uses, and if you get it wrong you
   will crash on fast blitter operations. See §7.3.

8. **`BLIT_NASTY_CPU_STEAL_CYCLE_COUNT = 3`.** Without BLTPRI, the CPU
   gets one cycle every time it has waited 3 CCKs. With BLTPRI, never.
   This exact value matters — games that rely on a sync tick between
   the blitter and the CPU work at 3 and break at 2 or 4. See §8.1.

9. **The CPU bank type table drives everything.** Don't special-case
   each chip-RAM read. Look up `ce_banktype[addr >> 16]` and dispatch
   to the arbiter or the fast path accordingly. This is how you
   correctly model slow RAM as "chip-bus contention, no DMA," and how
   you get Zorro III to be truly contention-free. See §9.1.

10. **CIA accesses must sync to the E-clock.** A worst-case CIA read is
    9.5 CCKs. Ignoring this and hard-coding "CIA read = 1.4 μs" will
    break any program that reads CIAA-ICR in a tight timed loop
    (including trackdisk, every keyboard driver, and several demos that
    use CIA-B for 100 Hz music ticks). See §10.2.

11. **ICR read must be atomic w.r.t. pending interrupts.** Use
    `event2_newevent_xx` or equivalent to schedule new interrupts past
    the next E-clock boundary when they arrive during a read window.
    See §10.4.

12. **The audio state machine is per-period, not per-slot.** Do not try
    to model audio DMA as "fire one DMA on each slot." The channel
    runs its state machine on period underflows, and the state machine
    decides whether to request DMA. The DMAL slot simply fires if a
    request is pending. See §11.1.

13. **The first audio word is always discarded.** State 1 fetches it,
    state 5 loads it into `dat2`, and the "discard" happens because
    state 1 never outputs it. This is why audio.device puts a
    16-bit null at `AUDxLC+0`. See §11.2.

14. **DSKLEN write-twice is a safety latch, not a timing thing.** It
    has no cycle implication. But a real program will do it, so do not
    start DMA on the first write. See §12.1.

15. **Refresh slots are never free.** Do not count them in the "free
    slot" budget. Not even for the CPU. If your arbiter thinks they
    are, your "maximum CPU bandwidth" number will be 4/227 too high.
    See §13.

16. **Long lines add one CCK at the end of the line.** NTSC alternates
    227/228 per field; programmed BEAMCON0 modes may use any length.
    The extra slot is never claimed by anything. See §3.

---

## Appendix A — DMA slot allocation table

Typical PAL short line (227 CCKs), BPLCON0 configured for 4-plane lores
display with DDFSTRT=0x38, DDFSTOP=0xd0, all 8 sprites active, audio
channels 0 and 1 playing, disk idle, blitter idle:

```
CCK    Type                 Notes
----   -----------------    --------------------------------------
0x00   REFRESH/STROBE       STRHOR or STREQU/STRVBL on VBL lines
0x01   (free)               CPU/copper/blitter
0x02   REFRESH              NOP register (0x1fe)
0x03   (free)
0x04   REFRESH              NOP (0x1fe)
0x05   (free)
0x06   REFRESH              NOP (0x1fe)
0x07   DISK0                (free if disk idle)
0x08   (free)
0x09   DISK1                (free if disk idle)
0x0a   (free)
0x0b   DISK2                (free if HD disk idle)
0x0c   (free)
0x0d   AUD0                 (free if channel idle)
0x0e   (free)
0x0f   AUD1                 (free if channel idle)
0x10   (free)
0x11   AUD2                 (free if channel idle)
0x12   (free)
0x13   AUD3                 (free if channel idle)
0x14   (free)
0x15   SPR0 POS or DATA
0x16   (free)
0x17   SPR0 CTL or DATB
0x18   (free)               -- DDF hard start: 0x18 in ECS/AGA
0x19   SPR1 POS or DATA
0x1a   (free)
0x1b   SPR1 CTL or DATB
0x1c   (free)
0x1d   SPR2 POS or DATA
0x1e   (free)
0x1f   SPR2 CTL or DATB
0x20   (free)
0x21   SPR3 POS or DATA
0x22   (free)
0x23   SPR3 CTL or DATB
0x24   (free)
0x25   SPR4 POS or DATA
0x26   (free)
0x27   SPR4 CTL or DATB
0x28   (free)
0x29   SPR5 POS or DATA
0x2a   (free)
0x2b   SPR5 CTL or DATB
0x2c   (free)
0x2d   SPR6 POS or DATA
0x2e   (free)
0x2f   SPR6 CTL or DATB
0x30   (free)
0x31   SPR7 POS or DATA
0x32   (free)
0x33   SPR7 CTL or DATB
0x34   (free)
0x35   (free)
0x36   (free)
0x37   (free)
0x38   BPL4 word 0          DDFSTRT = 0x38
0x39   (free, 4-plane lores block)
0x3a   BPL2 word 0
0x3b   (free)
0x3c   BPL3 word 0
0x3d   (free)
0x3e   BPL1 word 0
0x3f   (free)
0x40   BPL4 word 1
... repeating the 4 2 3 1 - - - - pattern per 8 CCKs ...
0xd0   BPL1 word N (last)   DDFSTOP = 0xd0
0xd1   (free, tail of block)
0xd2   (free)
0xd3   (free)
0xd4   (free)
0xd5   (free)
0xd6   (free)
0xd7   (free)               -- DDF hard stop; bprun ends after this block
0xd8+  (all free)
...
0xe2   end of short line (wrap to hpos 0)
```

For 6-plane lores, the free bpl-block slots at cycles 4..7 fill in with
the additional plane sequence entries. For 8-plane hires (requires
FMODE ≥ 1), the block is 16 CCKs and all 8 are used.

For 2-plane lores, only cycles 0 and 2 of each 8-CCK block are used,
leaving 6 free per block — so a 2-plane lores display gives the CPU
roughly 75% of the bitplane region back.

---

## Appendix B — wait-state matrix

All values in 7.09 MHz CPU cycles (CPU cycles; divide by 2 for CCKs).

| Operation | Best | Typical | Worst | BLTPRI |
|---|---|---|---|---|
| MOVE.W chip→chip (dispatch + src + dst) | 16 | 22 | ∞ | blocked |
| MOVE.W chip→fast | 12 | 16 | 24 | blocked |
| MOVE.W fast→fast | 12 | 12 | 12 | 12 |
| MOVE.W fast→CIA | 32 | 40 | 52 | 32 |
| MOVE.W #imm,CIA | 28 | 36 | 48 | 28 |
| BTST #b,(A0) on CIAA-ICR | 24 | 32 | 48 | 24 |
| MOVE.L chip→chip | 24 | 32 | ∞ | blocked |
| JMP (An) chip | 8 | 12 | ∞ | blocked |
| JMP (An) fast | 8 | 8 | 8 | 8 |
| JSR (An) chip | 16 | 24 | ∞ | blocked |
| RTS chip | 16 | 24 | ∞ | blocked |

These are approximate — WinUAE computes them instruction-by-instruction
through `cpuemu_13.cpp` (68000 cycle-exact), but the order of magnitude is
right.

### B.1 Bitplane bandwidth budget

Free CCKs available on a line to CPU/blitter/copper (out of 227):

| Display | Free CCKs | CPU/blitter/copper % |
|---|---|---|
| Blank (0 planes) | 223 | 98% |
| 1 plane lores   | 215 | 95% |
| 2 planes lores  | 207 | 91% |
| 4 planes lores  | 191 | 84% |
| 6 planes lores  | 175 | 77% |
| 8 planes lores (impossible: > 4 = 0) | 223 | 98% |
| 1 plane hires   | 207 | 91% |
| 2 planes hires  | 191 | 84% |
| 4 planes hires  | 159 | 70% |
| 8 planes hires (FMODE 1, AGA) | 95 | 42% |
| 8 planes SHires (FMODE 2, AGA) | 31 | 14% |

These assume DDF 0x38..0xd0 (the standard "320 lores" DDF) and subtract
the 4 refresh slots. Sprite DMA slots are assumed idle. Disk/audio are
assumed idle. Actual free bandwidth is lower if any DMA channel is
active.

---

## Appendix C — cycle-exact implementation checklist

Copy this into your own codebase and tick items as you go.

### C.1 Foundations
- [ ] One master cycle counter (`currcycle`), advanced in CCK units
- [ ] Per-line CCK counter `agnus_hpos` 0..maxhpos-1
- [ ] Per-frame line counter `vpos`
- [ ] Long-line toggle flag (`lol`), toggled every line in NTSC
- [ ] Long-field toggle flag (`lof_store`), toggled every frame in PAL
- [ ] Event queue with O(log N) insert and O(1) peek

### C.2 Slot arbiter
- [ ] `rga_pipe` 3+1 entries with rotating slot offsets
- [ ] `CYCLE_*` bitmask for each pipe entry
- [ ] `write_rga()` / `read_rga_out()` / `shift_rga()` primitives
- [ ] `check_rga_free_slot_in()` for arbiter queries
- [ ] Bitplane decided 1 CCK earlier than anyone else (`RGA_SLOT_BPL` →
      `RGA_SLOT_IN` rotation)

### C.3 DMAL shifter
- [ ] 32-bit `dmal_shifter`, `start_dmal()` sets bit 1
- [ ] Shifted on even→odd `agnus_hpos` transitions
- [ ] Refresh slots 0..3 at positions 1,2,3,4 (bits 1..4)
- [ ] Disk slots at bits 5,6,7
- [ ] Audio slots at bits 8,9,10,11
- [ ] Sprite A slots at bits 11,13,15,17,19,21,23,25 (reused sprite/audio
      bit 11)
- [ ] Sprite B slots at bits 12,14,16,18,20,22,24,26

### C.4 Bitplane sequencer
- [ ] `cycle_diagram_table[3][3][9][32]` precomputed at init
- [ ] `fetchunits[]`, `fetchstarts[]`, `fm_maxplanes[]` tables
- [ ] DDFSTRT / DDFSTOP quantisation to fetch unit
- [ ] Hard start 0x18, hard stop 0xd7 (clear with HARDDIS)
- [ ] BPRUN state machine: latched on (dma && diw && ddf), cleared on
      DDFSTOP or end of DIW
- [ ] Modulo add on last fetch slot of last block (`ddf_stopping == 2`)
- [ ] OCS: BPLEN latch is 1 CCK later than ECS/AGA
- [ ] `real_bitplane_number[][][]` forcing 5-plane lores → 0 planes

### C.5 Sprite sequencer
- [ ] Per-sprite `dmastate`, `dmacycle`, `vstart`, `vstop`
- [ ] `generate_sprites(nr, slot)` called from DMAL on odd hpos
- [ ] Conflict with last bitplane slot when `bprun && ddf_stopping == 2`
- [ ] POS/CTL vs DATA/DATB register selection based on `dmastate`
- [ ] Sprite DMA switch-on edge case: slot decided but not allocated

### C.6 Copper
- [ ] Copper state machine: read1/read2/wait1/wait2/wait/skip1/skip2/
      skip/strobe_delay1/strobe_delay2
- [ ] Odd-hpos polarity (COPPER_CYCLE_POLARITY = 1)
- [ ] WAIT idle + idle + wake-up (3 slots)
- [ ] SKIP idle + idle + dummy fetch (3 slots)
- [ ] COPJMP: 1fe, 8c, RGA, 8c sequence
- [ ] Starvation when bitplane/sprite claims the free slot first

### C.7 Blitter
- [ ] 4-stage shifter (`shifter[0..3]`, `shifter_out`)
- [ ] `shifter_skip_b`, `shifter_skip_y` from BLTCON0
- [ ] `blit_cyclecount = 4 - skip_b - skip_y` for normal blits
- [ ] Line mode: constant 4 cycles/pixel
- [ ] Fill mode: extra idle when D without C
- [ ] D write delayed by `shifter_d1/d2` (2-CCK chain)
- [ ] AGA `shifter_d_aga` extra 2-CCK delay before busy clear
- [ ] `blit_queued` = 4 pipelined cycles max
- [ ] `BLIT_NASTY_CPU_STEAL_CYCLE_COUNT = 3`
- [ ] BLTPRI disables CPU stealing
- [ ] Copper-write-to-BLTxPT mid-blit conflict handling

### C.8 CPU
- [ ] `ce_banktype[]` lookup table
- [ ] `wait_cpu_cycle_read/write` chip-bus arbiter
- [ ] Fast RAM: 4 CPU cycles word access, no arbitration
- [ ] Slow RAM: goes through chip arbiter
- [ ] ROM: fast unless `cs_romisslow`
- [ ] CIA: E-clock sync (see C.10)
- [ ] `pissoff` / `cycles_do_special` polling hook for event queue

### C.9 Denise shifter
- [ ] BPL1DAT commit triggers `bpldat_docopy()` and
      `bpl1dat_enable_sprites()`
- [ ] Sprites visible 1 lores pixel earlier than bitplanes
- [ ] BURST mode inhibits BPL1DAT trigger
- [ ] `denise_bplfmode` selects 16/32/64-bit shifter path
- [ ] `denise_hdiw` / `denise_blank_active` gates pixel output

### C.10 CIA
- [ ] E-clock period = 10 CIA cycles = 5 CCKs
- [ ] Sync wait: (E_CLOCK_SYNC - current_phase) × E_CYCLE_UNIT
- [ ] Start: 4 × E_CYCLE_UNIT = 2 CCKs
- [ ] End: 6 × E_CYCLE_UNIT = 3 CCKs
- [ ] Worst-case CIA access ≈ 9.5 CCKs
- [ ] Timer A/B cascade (INMODE bits)
- [ ] TOD increment on VSYNC (CIAB) / 50/60 Hz (CIAA)
- [ ] TODMED bug when counting across 0x?F → 0x?0
- [ ] ICR read-clears-atomic via event queue

### C.11 Audio
- [ ] Per-channel state 0..5
- [ ] Period counter tick every CCK
- [ ] `setdr` / `setirq` primitives
- [ ] "Discard first word" behaviour (state 1 → 5)
- [ ] ADKCON modulation (attached channels)
- [ ] AUDxPER re-latch on underflow
- [ ] Per DMAL bit audio slot at hpos 0x0d/0x0f/0x11/0x13

### C.12 Disk
- [ ] DSKLEN double-write latch
- [ ] MFM sync against DSKSYNC
- [ ] DSKSYNC interrupt (INT7)
- [ ] Per DMAL bit disk slots at hpos 0x07/0x09/0x0b
- [ ] Read/write FIFO 3 words

### C.13 Refresh
- [ ] 4 refresh slots at hpos 0x00/0x02/0x04/0x06
- [ ] Never available to any other user
- [ ] First slot carries STRHOR / STREQU / STRVBL strobe register
- [ ] VERTB interrupt on STRVBL→STRHOR transition

---

## Appendix D — source map

All paths relative to `~/Projects/Emu198x-Unclean/WinUAE/`.

### D.1 Core files

| File | Role | Key functions |
|---|---|---|
| `custom.cpp` | Master DMA scheduler, Copper, sprite sequencer | `do_cck`, `generate_dma_requests`, `decide_bpl`, `generate_copper`, `generate_sprites`, `hsync_handler`, `handle_rga_out`, `handle_dmal` |
| `blitter.cpp` | Blitter state machine and shifter | `generate_blitter`, `get_current_channel`, `blitter_next_cycle`, `blitter_doit`, `actually_do_blit`, `build_blitfilltable` |
| `cia.cpp` | 8520 timer/TOD emulation | `CIA_update`, `cia_wait_pre`, `cia_wait_post`, `get_cia_sync_cycles`, `CIA_sync_interrupt`, `event_CIA_tod_inc_event` |
| `audio.cpp` | Paula audio state machine | `audio_state_channel2`, `audio_state_machine`, `setdr`, `setirq`, `AUDxDAT` |
| `disk.cpp` | Floppy MFM and DMA | `DSKLEN_2`, `disk_dmal`, `DISK_start`, MFM decode loop |
| `memory.cpp` | Bank dispatch, CE bank types | `fill_ce_banks`, `chipmem_wget`, `chipmem_wget_ce2` |
| `newcpu.cpp` | 68000/68020/68030 CE path | `mem_access_delay_word_read`, `mem_access_delay_long_read_ce020`, `fill_icache020` |
| `events.cpp` | Event queue | `events_schedule`, `event2_newevent_xx` |
| `drawing.cpp` | Denise shifter and pixel pipeline | `bpldat_docopy`, `bpl1dat_enable_sprites`, `bpl1dat_enable_bpls` |

### D.2 Headers

| File | Contents |
|---|---|
| `include/custom.h` | `CYCLE_*` bitmasks, RGA structs, MAXHPOS/MAXVPOS constants |
| `include/sysdeps.h` | `CYCLE_UNIT = 512` |
| `include/events.h` | `ev_*`, `ev2_*`, event queue prototypes |
| `include/memory.h` | `CE_MEMBANK_*` |

### D.3 Key line references (for fast jumping)

| Topic | File:line |
|---|---|
| `REFRESH_FIRST_HPOS = 3` | custom.cpp:68 |
| `HARDWIRED_DMA_TRIGGER_HPOS = 1` | custom.cpp:70 |
| `fetchunits[]`/`fetchstarts[]`/`fm_maxplanes[]` | custom.cpp:946 |
| `cycle_sequences[]` | custom.cpp:954 |
| `create_cycle_diagram_table()` | custom.cpp:992 |
| `hw_hpos_table[]` init | custom.cpp:1038 |
| `real_bitplane_number[]` forcing | custom.cpp:1026 |
| `setup_fmodes_delayed()` | custom.cpp:1059 |
| `write_rga()` | custom.cpp:277 |
| `shift_rga()` | custom.cpp:340 |
| `generate_copper_cycle_if_free()` | custom.cpp:3072 |
| `generate_copper()` | custom.cpp:9446 |
| `generate_bpl()` | custom.cpp:9850 |
| `decide_bpl()` | custom.cpp:9892 |
| `generate_sprites()` | custom.cpp:10157 |
| DMAL constants | custom.cpp:10235 |
| `handle_dmal()` | custom.cpp:10283 |
| `process_dmal()` | custom.cpp:10264 |
| `start_dmal()` / `shift_dmal()` | custom.cpp:10273 |
| `do_cck()` | custom.cpp:12350 |
| `handle_rga_out()` | custom.cpp:12043 |
| `dma_cycle()` | custom.cpp:12556 |
| `wait_cpu_cycle_read()` | custom.cpp:12598 |
| `wait_cpu_cycle_write()` | custom.cpp:12671 |
| `hsync_handler()` | custom.cpp:6615 |
| `hsync_handler_pre()` | custom.cpp:5585 |
| `hsync_handler_post()` | custom.cpp:6387 |
| `blitter_cant_access()` | custom.cpp:4390 |
| `BLITTER_MAX_PIPELINED_CYCLES = 4` | blitter.cpp:136 |
| `BLIT_NASTY_CPU_STEAL_CYCLE_COUNT = 3` | blitter.cpp:76 |
| `blit_cycle_diagram[]` (unused reference) | blitter.cpp:156 |
| `blit_cyclecount` formula | blitter.cpp:1156 |
| `get_current_channel()` | blitter.cpp:1182 |
| `blitter_next_cycle_always()` | blitter.cpp:1268 |
| `blitter_next_cycle()` | blitter.cpp:1289 |
| `generate_blitter()` | blitter.cpp:1717 |
| `actually_do_blit()` | blitter.cpp:929 |
| `E_CLOCK_SYNC_N`/`START_N`/`END_N` | cia.cpp:105 |
| `DIV10` | cia.cpp:127 |
| `get_cia_sync_cycles()` | cia.cpp:771 |
| `cia_wait_pre/post` | cia.cpp:2450 |
| `CIA_sync_interrupt()` | cia.cpp:795 |
| `TOD_INC_DELAY` | cia.cpp:1063 |
| `audio_state_channel2()` | audio.cpp:1746 |
| `disk_dmal()` | disk.cpp:5162 |
| `DSKLEN_2()` | disk.cpp:4844 |
| `fill_ce_banks()` | memory.cpp:2745 |
| `mem_access_delay_word_read()` | newcpu.cpp:8008 |
| `bpldat_docopy()` | drawing.cpp:3765 |
| `bpl1dat_enable_sprites()` | drawing.cpp:3738 |

---

## Appendix E — gaps and WinUAE approximations

Areas where WinUAE itself is approximate or chooses a convenient model over
the hardware-true one. These are things a new emulator could choose to do
differently (or leave alone).

### E.1 Blitter pointer OR conflict

When the Copper writes a BLTxPT mid-blit at the same CCK as the blitter
consumes that pointer, the real chip produces an OR of old and new
address bits on the bus. WinUAE restores the previous value instead:

```c
// blitter.cpp:1815
case 1: blt_info.bltapt = blt_info.bltapt_prev; break;
```

This is wrong in detail but right in spirit: no known program depends on
the exact OR-corruption pattern, only on the "write gets lost" outcome.

### E.2 Chip RAM "bus idle" cycles

WinUAE's `dma_cycle()` loop does not model the CAS-before-RAS refresh
timing inside the DRAM chips. It just treats refresh slots as opaque DMA
cycles. The real chip has a 2-CCK "idle" period inside the refresh slot
during which you could theoretically sneak a CPU access — WinUAE doesn't,
because doing so would break the "refresh slots are inviolable" rule for
no benefit. No known program depends on this.

### E.3 68020 prefetch pipeline

The 68020 CE path in WinUAE (`cpuemu_20.cpp`) is more approximate than the
68000 CE path (`cpuemu_13.cpp`). Specifically, the instruction prefetch
unit uses a simplified 2-word buffer model rather than the real chip's
4-word pipe. For most programs this doesn't matter; for demos with very
tight 68020-specific code it can be off by ±4 CPU cycles per loop.

### E.4 CIA TOD drift on PAL vs NTSC

WinUAE's TOD event fires on every VBL, which is 50 Hz PAL / 60 Hz NTSC.
On real hardware, the CIA CNT line is connected to the 50/60 Hz AC mains
via the PSU, so the TOD ticks at 50/60 Hz independent of the display
mode. This matters if a program changes BEAMCON0 to alter the display
refresh — on real hardware, TOD keeps ticking at 50/60 Hz; in WinUAE
it will tick at whatever the new display refresh is. WinUAE has a
hack via `cia_timer_hack_adjust` to correct for this but it isn't
perfect.

### E.5 Sprite attached-mode period

Attached sprites with vastly different vstart/vstop cause a non-obvious
stall that WinUAE may not emulate exactly at hpos-level precision. The
`dblscan` AGA-extension path in `generate_sprites()` has a special case
for this:

```c
// custom.cpp:10202
if (dodma && s->dblscan && (fmode & 0x8000) &&
    (vpos & 1) != (s->vstart & 1) && s->dmastate) {
    dodma = false;
}
```

The comment-free logic is "don't DMA on the wrong parity line" but
doesn't explain *why* — it's a workaround for an AGA hardware quirk that
isn't documented.

### E.6 Audio "dma wait hack"

```c
// audio.cpp:1779
if (usehacks() && (currprefs.cachesize || (regs.instruction_cnt - cdp->dmaofftime_cpu_cnt) >= 60)) {
    ...
    newsample(nr, (cdp->dat2 >> 0) & 0xff);
    zerostate(nr, true);
}
```

WinUAE has an explicit "60-instruction tolerance window" for programs
that disable audio DMA, do a tiny edit, and re-enable DMA expecting the
channel to be reset. On real hardware the channel would *not* reset — it
would resume from whatever phase it was in — and a tracker would get a
stuck note. WinUAE fakes a reset because too many broken trackers rely
on it. A more faithful emulator should disable this by default.

### E.7 CPU memory access before chip bus trigger

```c
// custom.cpp:12606
x_do_cycles_pre(CYCLE_UNIT);
dma_cycle(&mode, &ipl);
```

The order here is "pay 1 CCK, then sync to bus, then do the access."
The real 68000 actually does "sync to bus, do the access, 3 CCKs of
address+data hold." This matters for IPL (interrupt priority) sampling,
which can fire during the bus cycle. WinUAE works around it with the
explicit `regs.ipl_evt` check, but the result is that "IPL fetch delayed
by CIA access" ends up being approximate rather than bit-exact.

### E.8 UHRES bitplane and sprite modes

AGA's UHRES (super-hires × 2) bitplane and sprite modes (`BPLHSTRT`,
`SPRHSTRT`, registers 0x78/0x7a) are emulated but rarely tested because
no commercial program uses them. WinUAE's implementation is best-effort
and may be wrong in detail.

### E.9 CPU prefetch-buffer cross-line IPL latency

IPL fetch within a 68000 instruction, across the HSYNC boundary, has
known-bad behaviour in WinUAE. The workaround is the `regs.ipl_evt`
event that re-samples IPL after each bus cycle, but the precise timing
of the IPL-to-INT6 latch in the 68000 PLA is not modelled.

### E.10 Blitter fill-mode line-mode confusion

The blitter documentation is clear that line mode and fill mode are
mutually exclusive, but the real chip doesn't actually enforce this —
setting both bits at once produces a well-defined but undocumented
behaviour. WinUAE prints a debug warning and executes an approximation:

```c
// blitter.cpp:1132
if ((blt_info.bltcon1 & BLTFILL) == BLTFILL) {
    debugtest(DEBUGTEST_BLITTER, _T("weird fill mode\n"));
    blitife = 0;
}
```

No known program does this, but if you're aiming for 100% accuracy you
need to characterise the real behaviour on real hardware.

---

*End of cycle-accurate reference. If you find an area that's wrong or
under-explained, the cross-reference in Appendix D is the fastest way to
find the authoritative source.*

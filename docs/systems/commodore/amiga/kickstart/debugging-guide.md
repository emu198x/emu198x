# Kickstart Boot Debugging Guide

When a Kickstart ROM fails to boot, three things narrow the problem fast: what
colour is the screen, where is the PC, and which module owns that address. This
guide provides all three.

## COLOR00 Timeline

The Amiga ROM changes the background colour at key stages. A stuck screen
colour is the fastest way to identify the failing boot stage.

### KS 1.3 (256K, OCS)

| Stage | Address | COLOR00 | Visual | Meaning |
|-------|---------|---------|--------|---------|
| 3 | $FC0130 | $0444 | Dark grey | Custom chip reset complete |
| 4 | $FC0258 | $0888 | Medium grey | ExecBase init starting |
| 6+ | — | $0000 | Black | Graphics.library took over display |
| 10 | — | palette | Insert-disk | STRAP display active |
| err | $FC05CA | dynamic | Varies | Alert handler — D0 holds alert colour |

KS 1.3 has only two explicit COLOR00 writes ($0444 and $0888) plus a dynamic
write in the alert handler at $FC05CA. There is no light-grey stage — after
$0888 the screen stays medium grey until graphics.library blanks it to black.

**Reading the colour:** Dark grey ($0444) = still in custom chip reset. Medium
grey ($0888) = ExecBase being built. If the screen stays grey, the PC tells you
which module stalled.

### KS 2.04/2.05 (512K, ECS)

| Stage | Address | COLOR00 | Visual | Meaning |
|-------|---------|---------|--------|---------|
| 2 | $F800D2+ | — | Whatever was left | ROM checksum running |
| 3 | $F80142 | $0444 | Dark grey | Custom chip reset complete |
| 4 | $F802E2 | $0888 | Medium grey | ExecBase pre-init (memory sizing done) |
| 5 | $F80426 | $0AAA | Light grey | exec.library init entry |
| 5+ | $F806FA | $0CCC | Lighter grey | Module scan — about to call module init |
| 6+ | — | $0000 | Black | Graphics.library took over display |
| 10 | — | palette | Insert-disk | STRAP display active |
| err | $F802FE | $0F0F | Bright magenta | ExecBase build failed — dead-end loop at $F80306 |
| err | $F80708 | $0F0F | Bright magenta | Module scan Permit failed — dead-end loop at $F80710 |
| err | $F803C8 | dynamic | Varies | Error handler — D0 holds colour |
| err | $F803F4 | $0000 | Black (flash) | Error handler cleanup — loops writing $0000 |

KS 2.04 has four distinct grey stages ($0444 → $0888 → $0AAA → $0CCC), each
marking a different phase of early init. Two dead-end $0F0F writes catch fatal
failures with infinite loops. Addresses shown are for KS 2.04; KS 2.05 offsets
differ slightly but the sequence is the same.

### KS 3.x (512K, AGA)

| Stage | Address | COLOR00 | Visual | Meaning |
|-------|---------|---------|--------|---------|
| 2 | $F800D2+ | — | Near-black | ROM checksum running |
| 3 | $F8019C | $0111 | Near-black | Custom chip reset complete |
| 6+ | — | $0000 | Black | Graphics.library took over display |
| 10 | — | palette | Insert-disk | STRAP display active |
| err | $F80364 | $0F0F | Bright magenta | Chip RAM verification failed — dead-end |
| err | $F807D2 | $0F0F | Bright magenta | Module scan Permit failed — dead-end |
| err | $F8041A | $0FE5 | Bright pink | ROM checksum failure (D0 loaded at $F80404) |
| err | $F80446 | $0000 | Black | Error handler cleanup |

KS 3.x has no medium-grey or light-grey stages — the screen stays near-black
($0111) from custom chip reset until graphics.library blanks it. The $0FE5
bright pink is specifically a ROM checksum failure indicator. If the screen is
near-black and stays that way, match the PC to the module map below.

**KS 3.x near-black ($0111):** If the screen is almost-but-not-quite black, the
ROM is in its checksum loop or has just finished the custom chip reset. This is
normal for the first ~1 second of boot.

### Colour Quick-Reference

| Colour | Hex | What Failed |
|--------|-----|-------------|
| Near-black | $0111 | Still in checksum or very early init (KS 3.x only) |
| Dark grey | $0444 | Custom chip reset done, overlay clear next |
| Medium grey | $0888 | ExecBase being built — check memory detection |
| Light grey | $0AAA | exec.library init entry (KS 2.04/2.05 only) |
| Lighter grey | $0CCC | Module scan running (KS 2.04/2.05 only) |
| Bright magenta | $0F0F | Dead-end loop — ExecBase build or Permit failed |
| Bright pink | $0FE5 | ROM checksum failed — corrupt ROM image (KS 3.x) |
| Red | $0F00 | Dead-end alert — D7 has the alert code |
| Yellow | $0FF0 | Recoverable alert |
| Green | $00F0 | Nested alert |
| Black (stuck) | $0000 | Graphics.library init or display setup crashed |

## PC-to-Module Address Maps

When the CPU is stuck, match its PC to find which module's code is executing.

### KS 1.3 (A500/A2000, $FC0000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $FC00B6–$FC3276 | exec.library | +120 | $FC00D2 |
| $FC3276–$FC3290 | alert.hook | +5 | $FC3128 |
| $FC3508–$FC445C | audio.device | +40 | $FC354C |
| $FC4574–$FC47F8 | cia.resource | +80 | $FC45E0 |
| $FC47FC–$FC4B5C | disk.resource | +70 | $FC4840 |
| $FC4B64–$FC51D8 | expansion.library | +110 | $FC4BA0 |
| $FC53E4–$FD08B8 | graphics.library | +65 | $FCABA2 |
| $FD3D8C–$FDFF94 | intuition.library | +10 | $FD3DA6 |
| $FE09A4–$FE0A0C | layers.library | +31 | $FE0A2C |
| $FE3DA4–$FE4288 | mathffp.library | +0 | $FE3DEC |
| $FE42D0–$FE43D8 | misc.resource | +70 | $FE4314 |
| $FE43DC–$FE4524 | potgo.resource | +100 | $FE4424 |
| $FE4528–$FE4AC8 | ramlib.library | +70 | $FE4560 |
| $FE4B44–$FE4B8E | keymap.resource | +100 | $FE7F24 |
| $FE4B8E–$FE4BDA | keyboard.device | +60 | $FE4F44 |
| $FE4BDA–$FE4C26 | gameport.device | +60 | $FE53B0 |
| $FE4C26–$FE4C6C | input.device | +40 | $FE5AD0 |
| $FE4C6C–$FE6234 | console.device | +20 | $FE66E4 |
| $FE83E0–$FE841A | strap | -60 | $FE8444 |
| $FE8D6C–$FE9558 | timer.device | +50 | $FE8DF4 |
| $FE9564–$FEB05C | trackdisk.device | +20 | $FE97BE |
| $FEB060–$FEB380 | romboot.library | -40 | $FEB0A8 |
| $FEB47C–$FF310C | workbench.task | +0 | $FEB496 |
| $FF3E62–$FF3E94 | dos.library | +0 | $FF3E94 |

**Gap: $FD08B8–$FD3D8C (13K).** Between graphics.library EndSkip and
intuition.library — contains data tables and font bitmaps.

### KS 3.1 A1200 ($F80000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $F800B6–$F83706 | exec.library | +105 | $F804D4 |
| $F83706–$F837B8 | alert.hook | -55 | $F83664 |
| $F837B8–$F837D2 | expansion.library | +110 | $F8388C |
| $F837D2–$F84290 | diag init | +105 | $F83BD2 |
| $F84290–$F842AA | romboot | -40 | $F842F4 |
| $F842AA–$F851A8 | strap | -60 | $F84514 |
| $F851A8–$F9D7A4 | graphics.library | +65 | $F851EE |
| $F9F2B8–$FA7748 | dos.library | -120 | $F9F2E6 |
| $FA825E–$FA8288 | filesystem | -81 | $FA823C |
| $FAE1D8–$FB1E5C | console.device | +5 | $FAE1F2 |
| $FB1E5C–$FB500C | layers.library | +64 | $FB1E76 |
| $FB5040–$FB79AC | scsi.device | +10 | $FB5082 |
| $FB79B2–$FBA0A8 | con-handler | -121 | $FB85A8 |
| $FBA170–$FBA19A | gameport.device | +60 | $FBA18A |
| $FBA19A–$FBA1C4 | keyboard.device | +45 | $FBA1B4 |
| $FBA1C4–$FBA4AC | input.device | +40 | $FBA1DE |
| $FBB7A4–$FBC72C | audio.device | -120 | $FBB7E4 |
| $FBC844–$FBD428 | card.resource | +48 | $FBC938 |
| $FBD428–$FBDDE4 | utility.library | +103 | $FBD46C |
| $FBDDE8–$FBE74E | battclock.resource | +70 | $FBDE72 |
| $FBE750–$FBF080 | carddisk.device | +15 | $FBE8A0 |
| $FBF080–$FBF4A2 | ramlib | -100 | $FBF0B8 |
| $FBF4A4–$FBFA96 | ramdrive.device | +25 | $FBF4BE |
| $FBFA98–$FBFE86 | cia.resource | +80 | $FBFB00 |
| $FBFE88–$FBFF38 | misc.resource | +70 | $FBFEC8 |
| $FBFF3A–$FBFFFC | workbench.task | -120 | $FBFF54 |
| $FC0008–$FC013E | potgo.resource | +100 | $FC0048 |
| $FC0140–$FC02DE | FileSystem.resource | +80 | $FC01D8 |
| $FC02E0–$FC0626 | disk.resource | +70 | $FC030C |
| $FC0628–$FC0AC4 | mathffp.library | -120 | $FC066C |
| $FC0AC4–$FC18B0 | timer.device | +50 | $FC0B42 |
| $FC18B4–$FC2754 | mathieeesingbas.library | +0 | $FC1AD8 |
| $FC2754–$FC33D4 | keymap.library | +40 | $FC282E |
| $FC33D4–$FC3414 | bootmenu | -50 | $FC4222 |
| $FC3414–$FC46F0 | syscheck | -35 | $FC416A |
| $FC49E4–$FC66EC | trackdisk.device | +20 | $FC4D38 |
| $FC66EC–$FC7E98 | icon.library | -120 | $FC672C |
| $FC8B88–$FCAEE4 | ram-handler | -123 | $FC8B1C |
| $FCAFEC–$FCF218 | shell | -122 | $FCAFE4 |
| $FCF498–$FE8508 | intuition.library | +10 | $FCF4B2 |
| $FE8E14–$FEE9A8 | gadtools.library | -120 | $FE8E5A |
| $FEE9C0–$FFFDAC | workbench.library | -120 | $FEE9DA |
| $FFFDB0–$FFFF94 | battmem.resource | +69 | $FFFE34 |

**Largest modules:** graphics.library (97K), intuition.library (100K),
workbench.library (68K). Most boot stalls happen in graphics.library or
expansion.library init.

### KS 2.04 (A500+, $F80000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $F800B6–$F83BFC | exec.library | +105 | $F80420 |
| $F83BFC–$F83C16 | alert.hook | +5 | $F83AAA |
| $F83C18–$F83C32 | expansion.library | +110 | $F83CF0 |
| $F83C32–$F846A8 | diag init | +105 | $F83FD4 |
| $F846A8–$F85630 | audio.device | −120 | $F846E8 |
| $F8574C–$F86188 | battclock.resource | +45 | $F85804 |
| $F8618C–$F86384 | battmem.resource | +44 | $F86218 |
| $F863C4–$F887E4 | syscheck | −35 | $F87AA8 |
| $F8889C–$F88C94 | cia.resource | +80 | $F88904 |
| $F88C9A–$F8B2E6 | con-handler | −121 | $F89828 |
| $F8B408–$F8F3EC | console.device | +5 | $F8B422 |
| $F8F4B4–$F8F7FA | disk.resource | +70 | $F8F4E0 |
| $F90490–$F98488 | dos.library | −120 | $F904C0 |
| $F98F48–$F990EA | FileSystem.resource | +80 | $F98FE0 |
| $F99112–$F9913C | filesystem | −81 | $F990F0 |
| $F9E718–$F9F394 | keymap.library | +40 | $F9E7EE |
| $F9F46C–$FAFAF4 | graphics.library | +65 | $F9F4B2 |
| $FB79DC–$FBD818 | gadtools.library | −120 | $FBD784 |
| $FC0008–$FC1834 | icon.library | −120 | $FC0048 |
| $FC257C–$FC5FB4 | layers.library | +31 | $FC25B0 |
| $FC5FC8–$FC6468 | mathffp.library | −120 | $FC600C |
| $FC64AC–$FC76BC | mathieeesingbas.library | +0 | $FC6516 |
| $FC76BC–$FC776C | misc.resource | +70 | $FC76FC |
| $FC776C–$FC7E48 | ramdrive.device | +25 | $FC7786 |
| $FC7E48–$FC8312 | ramlib | −100 | $FC7E8E |
| $FC8314–$FC833E | gameport.device | +60 | $FC832E |
| $FC833E–$FC8368 | keyboard.device | +45 | $FC8358 |
| $FC8368–$FC8654 | input.device | +40 | $FC8382 |
| $FC99D8–$FCDC28 | shell | −122 | $FC99D0 |
| $FCDEA4–$FCDEBE | romboot | −40 | $FCDF18 |
| $FCDEBE–$FCEDC0 | strap | −60 | $FCE140 |
| $FCEDC4–$FCFBD8 | timer.device | +50 | $FCEE54 |
| $FCFBD8–$FD190C | trackdisk.device | +20 | $FCFF6A |
| $FD1978–$FD3D84 | ram-handler | −123 | $FD190C |
| $FD3E34–$FD3F6A | potgo.resource | +100 | $FD3E74 |
| $FD3F9A–$FD4940 | utility.library | +103 | $FD3FB4 |
| $FD49A0–$FEBF84 | intuition.library | +10 | $FD49BA |
| $FED818–$FFF700 | workbench.library | −120 | $FED832 |

### KS 2.05 (A600, $F80000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $F800B6–$F83BF8 | exec.library | +105 | $F80460 |
| $F83BF8–$F83C12 | alert.hook | +5 | $F83AA6 |
| $F83C14–$F83C2E | expansion.library | +110 | $F83CEC |
| $F83C2E–$F84694 | diag init | +105 | $F83FCE |
| $F84694–$F858A4 | mathieeesingbas.library | +0 | $F846FE |
| $F858A4–$F8621C | carddisk.device | +15 | $F85A08 |
| $F8625C–$F88B74 | scsi.device | +10 | $F862B0 |
| $F88B74–$F89AFC | audio.device | −120 | $F88BB4 |
| $F89C18–$F8A654 | battclock.resource | +45 | $F89CD0 |
| $F8A658–$F8A850 | battmem.resource | +44 | $F8A6E4 |
| $F8A856–$F8CEA2 | con-handler | −121 | $F8B3E4 |
| $F8CFC4–$F90FA8 | console.device | +5 | $F8CFDE |
| $F91070–$F913B6 | disk.resource | +70 | $F9109C |
| $F9204C–$F9A0C0 | dos.library | −120 | $F9207E |
| $F9AB80–$F9AD22 | FileSystem.resource | +80 | $F9AC18 |
| $F9AD24–$F9AE5A | potgo.resource | +100 | $F9AD64 |
| $F9AE82–$F9AEAC | filesystem | −81 | $F9AE60 |
| $FA04B0–$FA62EC | gadtools.library | −120 | $FA6258 |
| $FA62EC–$FA7B18 | icon.library | −120 | $FA632C |
| $FA879C–$FA8C3C | mathffp.library | −120 | $FA87E0 |
| $FA8C80–$FB930C | graphics.library | +65 | $FA8CC8 |
| $FC0BA8–$FC0BE8 | bootmenu | −50 | $FC0C30 |
| $FC0BE8–$FC3008 | syscheck | −35 | $FC22CC |
| $FC30C0–$FC34B6 | cia.resource | +80 | $FC3128 |
| $FC34B8–$FC402C | card.resource | +48 | $FC35C0 |
| $FC4054–$FDB4D4 | intuition.library | +10 | $FC406E |
| $FDCA38–$FDD6B4 | keymap.library | +40 | $FDCB0E |
| $FDD6B4–$FE0D98 | layers.library | +31 | $FDD6CE |
| $FE0D9C–$FE0E4C | misc.resource | +70 | $FE0DDC |
| $FE0EB8–$FE32C4 | ram-handler | −123 | $FE0E4C |
| $FE3374–$FE3A78 | ramdrive.device | +25 | $FE338E |
| $FE3A78–$FE3F42 | ramlib | −100 | $FE3ABE |
| $FE3F44–$FE3F6E | gameport.device | +60 | $FE3F5E |
| $FE3F6E–$FE3F98 | keyboard.device | +45 | $FE3F88 |
| $FE3F98–$FE4284 | input.device | +40 | $FE3FB2 |
| $FE5608–$FE9858 | shell | −122 | $FE5600 |
| $FE9AD4–$FE9AEE | romboot | −40 | $FE9B4C |
| $FE9AEE–$FEAA98 | strap | −60 | $FE9D70 |
| $FEAA9C–$FEB8B0 | timer.device | +50 | $FEAB2C |
| $FEB8B0–$FED5E4 | trackdisk.device | +20 | $FEBC42 |
| $FED612–$FEDFB8 | utility.library | +103 | $FED62C |
| $FEE0E4–$FFFFCC | workbench.library | −120 | $FEE0FE |

**KS 2.05 additions vs KS 2.04:** scsi.device, card.resource, and
carddisk.device — the A600 has IDE (via Gayle) and PCMCIA support that the
A500+ lacks.

## Known Stall Signatures

These are failure modes encountered during Emu198x development, with the
diagnostic pattern that identifies each one.

### EClock Calibration DIVU #0

| Field | Value |
|-------|-------|
| **Symptom** | CPU exception during STRAP, address error or divide-by-zero |
| **PC** | Inside strap module (KS 1.3: $FE8444+, KS 3.1: $F84514+) |
| **Root cause** | GfxBase EClock field is zero |
| **Why** | graphics.library measures one video frame using CIA-A timer B. If CIA timers don't advance relative to VBLANK, the measurement returns 0. STRAP divides by this value. |
| **Fix** | CIA timer ticks must advance at the correct rate. Check CIA-A CRA/CRB timer enable bits and ensure timer countdown happens in sync with the emulated clock. |
| **History** | Fixed in Emu198x — the battclock force-set corrupted timer.device's EClock calibration. |

### A3000 KS 2.02/3.1 Cold Restart Loop

| Field | Value |
|-------|-------|
| **Symptom** | Boot reaches STRAP-like state then restarts, or cycles between grey screens |
| **PC** | Jumping between ROM and low chip RAM addresses |
| **Root cause** | 68030 RTE not popping format/vector word |
| **Why** | 68010+ exception frames include a format word after SR+PC. RTE must read this word and pop additional data per the format code. If RTE only pops 6 bytes (68000 style), SSP is misaligned by 2. exec's Supervisor() call uses a privilege-violation trick — the RTS after the exception handler pops a garbage return address, executing from unmapped memory. |
| **Fix** | RTE on 68010+ must read the format word at SP+6 and handle format codes ($0=4-word, $2=6-word for 68020 address error, etc.). |
| **History** | Fixed via TAG_RTE_READ_FORMAT in decode.rs. |

### A3000 Device Init Timeout

| Field | Value |
|-------|-------|
| **Symptom** | Boot reaches insert-disk DMACON ($03F0) but display is blank, or stalls before STRAP |
| **PC** | Somewhere in resident module init (use PC-to-module map to identify) |
| **Root cause** | Unknown — likely expansion.library or timer.device calibration on 68030 |
| **Why** | The A3000 has different hardware (RAMSEY, Fat Gary, DMAC) and a faster CPU. Init routines may timeout differently or probe hardware that isn't fully emulated. |
| **Status** | Open issue — DMAC at $DD0000 is never accessed, so the stall is pre-SCSI. |

### Keyboard Handshake Timeout

| Field | Value |
|-------|-------|
| **Symptom** | Boot pauses for ~2 seconds during keyboard.device init, then continues |
| **PC** | Inside keyboard.device (KS 1.3: $FE4F44+, KS 3.1: $FBA1B4+) |
| **Root cause** | Keyboard controller not sending power-up sequence |
| **Why** | keyboard.device expects $FD (init) then $FE (term) via CIA-A serial port. If the keyboard controller doesn't send these within the timeout, the device gives up and continues. Boot succeeds but keyboard won't work. |
| **Fix** | The emulated keyboard must send the power-up sequence ($FD, $FE) via CIA-A SP. Check falling-edge detection on CRA bit 6. |
| **History** | Fixed — keyboard encoding was wrong (missing bit inversion). |

### Slow RAM Crash (KS 1.2+ A500/A2000)

| Field | Value |
|-------|-------|
| **Symptom** | Guru meditation $01000005 (no memory) or crash shortly after ExecBase init |
| **PC** | Inside exec.library early init |
| **Root cause** | No slow RAM configured for A500/A2000 |
| **Why** | KS 1.2+ exec places ExecBase in slow RAM at $C00000 if present. Without it, ExecBase goes at the top of 512K chip RAM, leaving very little free chip RAM for module init allocations. Several modules fail to AllocMem. |
| **Fix** | Configure 512K slow RAM at $C00000 for A500/A2000 with KS 1.2+. |
| **History** | Boot tests use `slow_ram_size: 512 * 1024`. |

### PCMCIA False Detection (KS 2.05+ A600/A1200)

| Field | Value |
|-------|-------|
| **Symptom** | CPU jumps to garbage address early in boot, immediate crash |
| **PC** | $600000+ (PCMCIA common memory area) |
| **Root cause** | $A00000 returns $91 when it shouldn't |
| **Why** | The ROM probes $A00000 for PCMCIA CIS tuples ($91, $05, $23). If the emulator maps garbage or uninitialized memory at $A00000 that happens to start with $91, the ROM reads a 4-byte offset and jumps to $600000+offset. |
| **Fix** | PCMCIA attribute space at $A00000 must return $FF (no card) or $00 (unmapped). |

### AGA Detection Failure (KS 3.0+ A1200)

| Field | Value |
|-------|-------|
| **Symptom** | Insert-disk screen has wrong colours or resolution, or boot stalls in graphics.library |
| **PC** | Inside graphics.library (KS 3.1: $F851A8–$F9D7A4) |
| **Root cause** | DENISEID ($DFF07C) returns wrong value |
| **Why** | graphics.library reads DENISEID to detect OCS ($FF), ECS ($FC), or AGA ($F8). Wrong ID causes it to skip AGA register setup or use wrong palette bank logic. |
| **Fix** | DENISEID must return $F8 for AGA (Lisa), $FC for ECS (Super Denise), $FF for OCS. |

### Overlay Not Clearing

| Field | Value |
|-------|-------|
| **Symptom** | Boot writes to chip RAM but reads back ROM data, memory detection fails |
| **PC** | Inside memory detection ($FC01CE+ for KS 1.3, $F80234+ for KS 3.1) |
| **Root cause** | CIA-A PRA write to $BFE001 doesn't clear the overlay latch |
| **Why** | The ROM writes $02 (KS 1.3) or $00 (KS 3.1) to CIA-A PRA to clear the OVL bit. If the address decoder doesn't respond, ROM stays mapped at $000000 and memory detection writes go nowhere. |
| **Fix** | CIA-A PRA bit 0 must control the overlay latch. Writing 0 to bit 0 must immediately unmap ROM from $000000. |

## Inter-Module Dependencies

Which modules depend on which during init. An arrow means "calls OpenLibrary,
OpenDevice, or OpenResource on" during its init function.

### Dependency Graph

```
exec.library (pri +120/+105)
  <- everything (all modules call exec functions via A6)

expansion.library (pri +110)
  -> exec.library (MakeLibrary, AddLibrary)

graphics.library (pri +65)
  -> exec.library (AllocMem, MakeLibrary)
  -> expansion.library (reads ExpansionBase for board info)
  reads: DENISEID ($DFF07C), VPOSR ($DFF004), CIA-A timers

layers.library (pri +64/+31)
  -> exec.library
  -> graphics.library (OpenLibrary "graphics.library")

timer.device (pri +50)
  -> exec.library
  -> cia.resource (OpenResource "ciaa.resource", "ciab.resource")
  reads: CIA-A/CIA-B timer registers

cia.resource (pri +80)
  -> exec.library
  reads: CIA-A ($BFE001+), CIA-B ($BFD000+)

card.resource (pri +48, KS 2.05+)
  -> exec.library
  reads: Gayle registers ($DA8000+)

keyboard.device (pri +60/+45)
  -> exec.library
  -> cia.resource (uses CIA-A serial port)
  reads: CIA-A SP ($BFEC01), CIA-A CRA ($BFEE01)

input.device (pri +40)
  -> exec.library
  -> keyboard.device (OpenDevice)
  -> gameport.device (OpenDevice)
  -> timer.device (OpenDevice)

intuition.library (pri +50/+10)
  -> exec.library
  -> graphics.library (OpenLibrary)
  -> layers.library (OpenLibrary)
  -> timer.device (OpenDevice)
  -> input.device (OpenDevice)

trackdisk.device (pri +20)
  -> exec.library
  -> cia.resource (uses CIA-B for motor/step)
  -> timer.device (OpenDevice for motor timing)
  reads: CIA-B PRA ($BFD100), custom DSKLEN ($DFF024)

console.device (pri +20/+5)
  -> exec.library
  -> graphics.library (OpenLibrary)
  -> intuition.library (OpenLibrary)
  -> keymap.library (OpenLibrary, KS 2.0+)

dos.library (pri +0/-120)
  -> exec.library
  -> intuition.library (OpenLibrary)
  -> timer.device (OpenDevice)

strap (pri -60)
  -> exec.library (AllocMem for bitmap)
  -> graphics.library (OpenLibrary for display setup)
  -> intuition.library (OpenScreen, OpenWindow)
  reads: GfxBase EClock field (divisor for timing calculations)
```

### Critical Chains

If module X fails, all modules below it in these chains also fail:

```
exec -> expansion -> graphics -> layers -> intuition -> strap
                                        -> console -> dos
                  -> cia.resource -> timer -> keyboard -> input
                                          -> trackdisk
```

**graphics.library** is the single biggest failure point — intuition, layers,
console, dos, and strap all depend on it. If graphics.library init crashes, the
boot produces no visible output beyond the grey screen.

**cia.resource** is the second — timer.device, keyboard.device, and
trackdisk.device all depend on working CIA access.

## Module Init Hardware Traces

What each key module reads and writes during its init function. Use these to
verify that the emulator's hardware responds correctly at each stage.

_Note: addresses shown are for KS 1.3 unless marked otherwise. KS 3.1 addresses
differ but the hardware interactions are structurally similar._

### expansion.library

**Init:** KS 1.3 $FC4BA0, KS 3.1 $F8388C

Scans Zorro II autoconfig space at $E80000:

1. Read manufacturer ID from $E80000 (high nibble) and $E80002 (low nibble)
2. If $E80000 reads $FF or $00, no board present — scan complete
3. If board found, read product ID, serial number, ROM info
4. Write board base address to $E88000 (SHUTUP register)
5. Move to next slot and repeat

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $E80000 | R | Autoconfig manufacturer ID (high) |
| $E80002 | R | Autoconfig manufacturer ID (low) |
| $E80004-$E8003E | R | Board configuration registers |
| $E80048 | W | ec_ShutUp — acknowledge board |

**Emulator requirement:** $E80000 must return $FF (no board) or valid autoconfig
data. Must NOT bus error.

### graphics.library

**Init:** KS 1.3 $FCABA2, KS 3.1 $F851EE

The most complex init — detects chipset, calibrates timing, sets up display:

1. **Chipset detection:** Read DENISEID ($DFF07C)
   - $FF → OCS (no ID register, returns open bus)
   - $FC → ECS (Super Denise)
   - $F8 → AGA (Lisa)
2. **Region detection:** Read VPOSR ($DFF004) bit 12
   - Bit 12 set → PAL
   - Bit 12 clear → NTSC
3. **EClock calibration:**
   - Set CIA-A timer B to $FFFF, one-shot mode
   - Wait for vertical blank (poll VPOSR for line 0)
   - Start timer
   - Wait for next vertical blank
   - Read timer — elapsed ticks = EClock ticks per frame
   - Store at GfxBase+$22 (used as divisor by STRAP and timer.device)
4. **Copper setup:** Build default copper list, set COP1LC, trigger COPJMP1
5. **AGA init (KS 3.0+):**
   - Write BPLCON3 bank select for all 8 palette banks
   - Write BPLCON4 XOR value ($0000)
   - Write FMODE ($0000 — normal fetch)
   - Set 256-entry 24-bit palette to all black

**Hardware reads/writes:**
| Address | R/W | Value | Purpose |
|---------|-----|-------|---------|
| $DFF07C | R | $FF/$FC/$F8 | DENISEID — chipset ID |
| $DFF004 | R | varies | VPOSR — PAL/NTSC bit 12 |
| $BFE801 | W | $FF | CIA-A timer B low (one-shot setup) |
| $BFE901 | W | $FF | CIA-A timer B high — starts timer |
| $BFF001 | W | $09 | CIA-A CRB — one-shot, start |
| $BFE801 | R | varies | CIA-A timer B low (read elapsed) |
| $BFE901 | R | varies | CIA-A timer B high |
| $DFF080 | W | addr | COP1LCH — copper list address |
| $DFF082 | W | addr | COP1LCL |
| $DFF088 | W | any | COPJMP1 — restart copper |
| $DFF096 | W | $83C0 | DMACON — enable BPLEN+COPEN+BLTEN |
| $DFF110 | W | varies | BPLCON3 — AGA palette bank (KS 3.0+) |
| $DFF10C | W | $0000 | BPLCON4 — AGA XOR (KS 3.0+) |
| $DFF1FC | W | $0000 | FMODE — AGA fetch mode (KS 3.0+) |

**Failure modes:**
- Wrong DENISEID → wrong chipset path → AGA registers not initialised
- CIA timer doesn't count → EClock = 0 → DIVU #0 crash in STRAP
- VPOSR bit 12 wrong → PAL/NTSC mismatch → wrong display timing

### timer.device

**Init:** KS 1.3 $FE8DF4, KS 3.1 $FC0B42

1. Open "ciaa.resource" and "ciab.resource" via OpenResource
2. Allocate CIA-A timer A for UNIT_MICROHZ
3. Allocate CIA-B timer A for UNIT_VBLANK
4. Set up interrupt handlers for CIA timer interrupts
5. Start timers

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $BFE401 | R/W | CIA-A timer A low |
| $BFE501 | R/W | CIA-A timer A high |
| $BFE001 | R | CIA-A PRA (check for handshake) |
| $BFD400 | R/W | CIA-B timer A low |
| $BFD500 | R/W | CIA-B timer A high |

**Failure modes:**
- cia.resource not found → timer.device init fails → keyboard, input, trackdisk all fail
- CIA timer registers don't respond → timer hangs during calibration

### cia.resource

**Init:** KS 1.3 $FC45E0, KS 3.1 $FBFB00

Creates two resource instances (ciaa.resource and ciab.resource):

1. Probe CIA-A at $BFE001 (read PRA)
2. Probe CIA-B at $BFD000 (read PRA)
3. Set up ICR (interrupt control) handlers
4. Register as exec resources

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $BFE001 | R | CIA-A PRA |
| $BFE201 | R/W | CIA-A DDRA |
| $BFED01 | W | CIA-A ICR — clear all interrupt sources |
| $BFD000 | R | CIA-B PRA |
| $BFD200 | R/W | CIA-B DDRA |
| $BFDD00 | W | CIA-B ICR — clear all interrupt sources |

### keyboard.device

**Init:** KS 1.3 $FE4F44, KS 3.1 $FBA1B4

1. Open ciaa.resource
2. Allocate CIA-A serial port interrupt
3. Wait for keyboard power-up sequence:
   - Expect $FD (init keycode) then $FE (term keycode) via CIA-A SP
   - Timeout after ~141ms (100K E-clock ticks) — resend request
4. Send handshake acknowledgment (pulse CIA-A CRA bit 6 low then high)

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $BFEC01 | R | CIA-A SP — read keyboard data |
| $BFEE01 | R/W | CIA-A CRA — SP direction bit 6 |
| $BFED01 | R/W | CIA-A ICR — enable SP interrupt |

**Failure mode:** No keyboard power-up → 2-second timeout per attempt, boot
continues but keyboard is dead.

### trackdisk.device

**Init:** KS 1.3 $FE97BE, KS 3.1 $FC4D38

1. Open timer.device (for motor timing)
2. Open disk.resource (for DMA channel allocation)
3. Set up CIA-B for disk control:
   - PRA bits 7-3: motor, side, direction, step, select
4. Step head to track 0 (seek to cylinder 0)
5. Turn motor on, wait for spin-up
6. Read disk ID (detect HD vs DD)

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $BFD100 | R/W | CIA-B PRA — motor, side, direction, step |
| $BFD200 | W | CIA-B DDRA — set control lines as outputs |
| $DFF024 | W | DSKLEN — disk DMA control |
| $DFF020 | W | DSKPTH — DMA buffer pointer high |
| $DFF022 | W | DSKPTL — DMA buffer pointer low |
| $DFF01A | R | DSKBYTR — disk byte and status |
| $DFF07C | R | DENISEID — used for HD detection on AGA |

### strap

**Init:** KS 1.3 $FE8444, KS 3.1 $F84514

1. AllocMem chip RAM for bitmap (MEMF_CHIP | MEMF_CLEAR)
2. OpenLibrary "graphics.library"
3. Read GfxBase EClock value (used for timing calculations)
4. OpenScreen via Intuition (or build copper list directly on KS 1.x)
5. Set up copper list:
   - BPLCON0 = $2302 (KS 1.x) or $8302/$8303 (KS 2.0+)
   - DIWSTRT/DIWSTOP for standard window
   - BPL1PT–BPL3PT for bitmap
   - Palette entries
6. Enable DMA: DMACON = $83C0
7. Draw checkmark icon and text using blitter
8. Enter disk-wait loop

**Failure modes:**
- AllocMem fails → Alert $3003800A (no chip RAM for screen)
- OpenLibrary "graphics.library" fails → crash
- GfxBase EClock = 0 → DIVU #0
- Blitter not working → icon/text missing but screen visible

### intuition.library

**Init:** KS 1.3 $FD3DA6, KS 3.1 $FCF4B2

1. OpenLibrary "graphics.library" (must succeed)
2. OpenLibrary "layers.library" (must succeed)
3. OpenDevice "timer.device" (for double-click timing)
4. OpenDevice "input.device" (for input handler chain)
5. Allocate IntuitionBase
6. Build default display (Workbench screen structure)
7. Set up input handler for mouse/keyboard events

**Dependencies:** graphics.library, layers.library, timer.device, input.device.
If any of these fail, Intuition init fails, which causes STRAP to fail (no
OpenScreen available).

### scsi.device (KS 2.05+ A600/A1200)

**Init:** KS 2.05 $F862B8, KS 3.1 $FB5082

1. Probe for Gayle at $DA0000
2. Read IDE status register at $DA2000 (or $DA3000)
3. If no response within timeout, init completes with no drives
4. If IDE detected, send IDENTIFY command
5. Register as exec device

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $DA0000 | R | Gayle IDE data register |
| $DA2000 | R | Gayle IDE status |
| $DA1000 | W | Gayle IDE command |
| $DA8000 | R/W | Gayle config |

**Failure mode:** If $DA0000 bus-errors instead of returning data, scsi.device
init crashes. Must return $FF or $00 (no drive) without error.

### card.resource (KS 2.05+ A600/A1200)

**Init:** KS 2.05 $FC0BC4, KS 3.1 $FBC938

1. Check for Gayle presence by reading $DA8000
2. If Gayle found, initialise PCMCIA controller
3. Set up card insertion/removal interrupt via Gayle
4. If card present, read CIS tuples from $A00000

**Hardware reads/writes:**
| Address | R/W | Purpose |
|---------|-----|---------|
| $DA8000 | R/W | Gayle PCMCIA config |
| $DA9000 | R/W | Gayle PCMCIA status |
| $DAA000 | R/W | Gayle PCMCIA interrupt |
| $A00000+ | R | PCMCIA attribute memory (CIS tuples) |

## Register State Cheat Sheet

When a boot stalls, reading DMACON ($DFF002), BPLCON0 ($DFF100), and INTENA
($DFF01C) tells you how far the ROM got. DMACON is cumulative — each module
enables its DMA channels with SET writes (bit 15 = 1), so the read value grows
as boot progresses.

### DMACON Progression

| DMACON read | Bits set | Boot stage | What happened last |
|-------------|----------|------------|-------------------|
| $0000 | none | 3 | Custom chip reset — all DMA disabled |
| $0200 | COPEN | 5 | exec init enabled copper DMA |
| $0240 | COPEN+BLTEN | 6 | graphics.library enabled blitter |
| $02E0 | COPEN+BLTEN+BPLEN+SPREN | 6+ | graphics.library enabled display |
| $0180 | BPLEN+COPEN | 10 | STRAP display active (OCS/ECS) |
| $03C0 | BPLEN+COPEN+BLTEN+SPREN | 10 | STRAP display active (AGA) |
| $03F0 | BPLEN+COPEN+BLTEN+SPREN+DSKEN+AUDEN | 10+ | Full boot — disk and audio DMA active |

**Note:** The exact intermediate values vary by KS version and chipset. The final
STRAP values ($0180 for OCS/ECS, $03C0 for AGA) are stable across versions.

### DMACON Bit Reference

| Bit | Hex | Name | Meaning |
|-----|-----|------|---------|
| 9 | $0200 | COPEN | Copper DMA |
| 8 | $0100 | BPLEN | Bitplane DMA |
| 7 | $0080 | BLTEN | Blitter DMA (AGA sets via copper) |
| 6 | $0040 | SPREN | Sprite DMA |
| 5 | $0020 | DSKEN | Disk DMA |
| 4 | $0010 | AUD3EN | Audio channel 3 |
| 3–1 | $000E | AUD2–0EN | Audio channels 2–0 |
| 0 | $0001 | — | (reserved) |

### BPLCON0 Progression

| BPLCON0 | Meaning | Boot stage |
|---------|---------|------------|
| $0200 | All planes off, colour burst on | 3 (custom reset blanked display) |
| $2302 | 3 planes + LACE (KS 1.x lores) | 10 (STRAP active, KS 1.2/1.3) |
| $8302 | 3 planes + HIRES (KS 2.0+ OCS) | 10 (STRAP active, OCS) |
| $8303 | 3 planes + HIRES + ERSY (ECS/AGA) | 10 (STRAP active, ECS/AGA) |

If BPLCON0 is still $0200, graphics.library either hasn't run or crashed during
init. If it shows a display mode value but the screen is blank, check that
bitplane pointers (BPL1PT–BPL3PT) point to valid chip RAM.

### INTENA Key Bits

| Bit | Hex | Name | Set by | Meaning if missing |
|-----|-----|------|--------|-------------------|
| 14 | $4000 | INTEN | exec init | Master enable — if clear, no interrupts work |
| 13 | $2000 | EXTER | cia.resource | CIA external interrupts — needed for timers |
| 5 | $0020 | VERTB | graphics.library | Vertical blank — needed for display updates |
| 3 | $0008 | COPER | copper list | Copper interrupt — used by Intuition |
| 2 | $0004 | SOFT | exec | Software interrupt — needed for task switching |

If INTEN ($4000) is not set, exec init didn't complete — check ExecBase at
$000004. If VERTB ($0020) is missing, graphics.library init failed.

## First Boot Checklist

When bringing up a new Amiga machine variant, these hardware features must work
before the ROM can reach the insert-disk screen. Check them in order — each
stage depends on the ones before it.

### Stage 1: CPU and ROM (no hardware interaction)

- [ ] ROM loaded at correct base ($FC0000 for 256K, $F80000 for 512K)
- [ ] SSP and PC extracted from first 8 bytes of ROM
- [ ] CPU executes from PC after reset
- [ ] 68000 exception vectors work (bus error, address error, illegal instruction)

### Stage 2: Overlay and Chip RAM

- [ ] Overlay latch maps ROM at $000000 on reset
- [ ] Writing 0 to CIA-A PRA bit 0 ($BFE001) clears overlay
- [ ] After overlay clear, $000000–$07FFFF is chip RAM (read/write)
- [ ] Chip RAM read-back works (write pattern, read it back)
- [ ] Unmapped addresses return 0 (not bus error)

### Stage 3: Custom Chip Registers

- [ ] Custom chip register space at $DFF000–$DFF1FF responds to reads and writes
- [ ] DMACON write ($DFF096) accepts SET/CLR bit 15 semantics
- [ ] INTENA write ($DFF09A) accepts SET/CLR bit 15 semantics
- [ ] INTREQ write ($DFF09C) accepts SET/CLR bit 15 semantics
- [ ] BPLCON0 write ($DFF100) controls display mode
- [ ] COLOR00 write ($DFF180) changes background colour
- [ ] VPOSR/VHPOSR ($DFF004/$DFF006) return advancing beam position
- [ ] VPOSR bit 12 reflects PAL (1) or NTSC (0)

### Stage 4: CIA Timers

- [ ] CIA-A at $BFE001 (active odd bytes: $BFE001, $BFE201, ..., $BFEF01)
- [ ] CIA-B at $BFD000 (active even bytes: $BFD000, $BFD100, ..., $BFDF00)
- [ ] Timer A and B countdown when started (CRA/CRB bit 0)
- [ ] Timer high-byte write auto-starts in one-shot mode (HRM Appendix F)
- [ ] ICR ($BFExD01/$BFDxD00) write-1-to-clear and set/clear semantics
- [ ] CIA-A serial port (SP at $BFEC01) receives keyboard data

### Stage 5: Autoconfig Space

- [ ] $E80000 returns $FF (no expansion boards) or valid autoconfig data
- [ ] $E80000 does NOT bus error

### Stage 6: Chipset ID (ECS/AGA only)

- [ ] DENISEID ($DFF07C) returns correct value:
  - $FF for OCS (open bus — no ID register)
  - $FC for ECS (Super Denise)
  - $F8 for AGA (Lisa)

### Stage 7: PCMCIA Space (A600/A1200 only)

- [ ] $A00000 returns $FF (no card) — must not return $91 accidentally
- [ ] PCMCIA attribute space does not bus error

### Stage 8: Gayle (A600/A1200 only)

- [ ] IDE registers at $DA0000–$DA3FFF respond (return $FF for no drive)
- [ ] Gayle config at $DA8000 responds (return $00 for idle)
- [ ] IDE/Gayle space does NOT bus error

### Stage 9: Keyboard

- [ ] Keyboard controller sends power-up sequence ($FD, $FE) via CIA-A SP
- [ ] CIA-A CRA bit 6 handshake (SP direction) works
- [ ] Without keyboard: boot continues after ~2-second timeout (not fatal)

### Stage 10: DMA and Display

- [ ] Copper DMA fetches and executes copper list
- [ ] Bitplane DMA fetches from BPLxPT addresses
- [ ] Blitter operates (BLTCON0/1, BLTAPT/BLTDPT, BLTSIZE triggers)
- [ ] At least one visible frame renders

### Machine-Specific Requirements

| Machine | Additional requirements |
|---------|----------------------|
| A500/A2000 | 512K slow RAM at $C00000 for KS 1.2+ |
| A500+ | ECS Agnus (Super Agnus) — no Gayle, no PCMCIA |
| A600 | Gayle at $DA0000, PCMCIA at $A00000, IDE space |
| A1200 | Gayle, PCMCIA, AGA DENISEID=$F8, 68020 |
| A3000 | RAMSEY at $DE0000, Fat Gary, DMAC at $DD0000, 68030 |
| A4000 | RAMSEY, 68030/040, no PCMCIA, no Gayle |

## PC-to-Module Maps (Remaining ROMs)

### KS 1.0 (A1000, $FC0000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $FC00B0–$FC2B9C | exec.library | +120 | $FC00CE |
| $FC2B9C–$FC2BB6 | alert.hook | +5 | $FC2A76 |
| $FC3748–$FC39D8 | audio.device | +40 | $FC378A |
| $FC481C–$FC485A | strap | −60 | $FC4872 |
| $FC5A80–$FC5D52 | cia.resource | +80 | $FC5AE8 |
| $FC5D54–$FC64EC | clist.library | +100 | $FC63F8 |
| $FC64F0–$FC6828 | disk.resource | +70 | $FC6530 |
| $FC682C–$FCF9D8 | graphics.library | +40 | $FC8976 |
| $FD58E8–$FE17B0 | intuition.library | +10 | $FD8114 |
| $FE2128–$FE4A58 | layers.library | +30 | $FE1FFC |
| $FE5AC4–$FE5BD8 | mathffp.library | +0 | $FE5B0C |
| $FE5F58–$FE605C | misc.resource | +70 | $FE5F98 |
| $FE6060–$FE61A4 | potgo.resource | +100 | $FE60A4 |
| $FE61A8–$FE64A0 | ramlib.library | +70 | $FE61EC |
| $FE6BA4–$FE6BF4 | keyboard.device | +60 | $FE6F6C |
| $FE6BF4–$FE6C44 | gameport.device | +60 | $FE73D8 |
| $FE6C44–$FE6C90 | input.device | +40 | $FE7B20 |
| $FE6C90–$FE81A0 | console.device | +20 | $FE8490 |
| $FEA0E4–$FEA7EC | timer.device | +60 | $FEA19C |
| $FEA7F8–$FEC02C | trackdisk.device | +20 | $FEA9E4 |
| $FEC03A–$FEC096 | workbench.task | +0 | $000000 |
| $FF4BEA–$FF4C1C | dos.library | +0 | $FF4C1C |

**KS 1.0 differences:** No expansion.library (no autoconfig). Has clist.library
(copper list manager, replaced by graphics.library in KS 1.2+). workbench.task
has init=$000000 (init via separate mechanism). 22 modules total vs 23 in KS 1.2.

### KS 1.2 (A500/A1000/A2000, $FC0000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $FC00B6–$FC323A | exec.library | +120 | $FC00D2 |
| $FC323A–$FC3254 | alert.hook | +5 | $FC30EC |
| $FC34CC–$FC43F4 | audio.device | +40 | $FC350E |
| $FC450C–$FC4790 | cia.resource | +80 | $FC4578 |
| $FC4794–$FC4AF4 | disk.resource | +70 | $FC47D8 |
| $FC4AFC–$FC516C | expansion.library | +110 | $FC4B38 |
| $FC5378–$FD0A3C | graphics.library | +65 | $FCABE2 |
| $FD3F5C–$FE0378 | intuition.library | +10 | $FD3F76 |
| $FE0D90–$FE0DF8 | layers.library | +31 | $FE0E18 |
| $FE424C–$FE472C | mathffp.library | +0 | $FE4294 |
| $FE4774–$FE487C | misc.resource | +70 | $FE47B8 |
| $FE4880–$FE49C8 | potgo.resource | +100 | $FE48C8 |
| $FE49CC–$FE4F6C | ramlib.library | +70 | $FE4A04 |
| $FE4FE4–$FE502E | keymap.resource | +100 | $FE83C8 |
| $FE502E–$FE507A | keyboard.device | +60 | $FE53E8 |
| $FE507A–$FE50C6 | gameport.device | +60 | $FE5854 |
| $FE50C6–$FE510E | input.device | +40 | $FE5F74 |
| $FE510E–$FE66D8 | console.device | +20 | $FE6B88 |
| $FE8884–$FE88C0 | strap | −60 | $FE88D6 |
| $FE90EC–$FE98D8 | timer.device | +50 | $FE9174 |
| $FE98E4–$FEB3DC | trackdisk.device | +20 | $FE9B3E |
| $FEB400–$FF34D8 | workbench.task | +0 | $FEB41A |
| $FF425A–$FF4290 | dos.library | +0 | $FF4290 |

**KS 1.2 vs KS 1.0:** Adds expansion.library (+110) and keymap.resource (+100).
Removes clist.library (copper list folded into graphics.library). Module layout
is nearly identical to KS 1.3.

### KS 3.0 A1200 ($F80000 base)

| Address Range | Module | Init Pri | Init Address |
|---------------|--------|----------|--------------|
| $F800B6–$F83272 | exec.library | +105 | $F80444 |
| $F83272–$F8328C | alert.hook | −55 | $F83138 |
| $F83704–$F8371E | expansion.library | +110 | $F837D8 |
| $F8371E–$F84174 | diag init | +105 | $F83AB8 |
| $F84174–$F84F60 | timer.device | +50 | $F841F2 |
| $F84F60–$F85440 | mathffp.library | −120 | $F84FA4 |
| $F85440–$F85A32 | ramdrive.device | +25 | $F8545A |
| $F85A34–$F86530 | utility.library | +103 | $F85A78 |
| $F86530–$F87740 | mathieeesingbas.library | +0 | $F8659A |
| $F87780–$F8A070 | scsi.device | +10 | $F877C2 |
| $F8A070–$F8A9E8 | carddisk.device | +15 | $F8A1D4 |
| $F8A9E8–$F8B55C | card.resource | +48 | $F8AAF0 |
| $F8B55C–$F8C4E4 | audio.device | −120 | $F8B59C |
| $F8C600–$F8CF66 | battclock.resource | +70 | $F8C68A |
| $F8CF68–$F8D14C | battmem.resource | +69 | $F8CFEC |
| $F8D14C–$F8D18C | bootmenu | −50 | $F8E398 |
| $F8D18C–$F8E8A4 | syscheck | −35 | $F8E310 |
| $F8E93C–$F8ED2A | cia.resource | +80 | $F8E9A4 |
| $F8ED32–$F913FC | con-handler | −121 | $F8F928 |
| $F914C4–$F95164 | console.device | +5 | $F914DE |
| $F95164–$F954AA | disk.resource | +70 | $F95190 |
| $F96148–$F9E3DC | dos.library | −120 | $F96178 |
| $F9EECC–$F9F06E | FileSystem.resource | +80 | $F9EF64 |
| $F9F096–$F9F0C0 | filesystem | −81 | $F9F074 |
| $FA4F78–$FBCDBC | graphics.library | +65 | $FA4FBE |
| $FBDB54–$FBF318 | icon.library | −120 | $FBDB94 |
| $FC0008–$FC0C84 | keymap.library | +40 | $FC00DE |
| $FC0C84–$FC3E64 | layers.library | +64 | $FC0C9E |
| $FC3E68–$FC3F18 | misc.resource | +70 | $FC3EA8 |
| $FC3F84–$FC62E0 | ram-handler | −123 | $FC3F18 |
| $FC6390–$FC67F6 | ramlib | −100 | $FC63CA |
| $FC67F8–$FC6822 | gameport.device | +60 | $FC6812 |
| $FC6822–$FC684C | keyboard.device | +45 | $FC683C |
| $FC684C–$FC6B38 | input.device | +40 | $FC6866 |
| $FC7EBC–$FCC1D8 | shell | −122 | $FC7EB4 |
| $FCC434–$FCC44E | romboot | −40 | $FCC498 |
| $FCC44E–$FCD35C | strap | −60 | $FCC6B8 |
| $FCD35C–$FCD492 | potgo.resource | +100 | $FCD39C |
| $FCD494–$FCF1B0 | trackdisk.device | +20 | $FCD7FA |
| $FCF1D8–$FE8074 | intuition.library | +10 | $FCF1F2 |
| $FE8974–$FEE7A0 | gadtools.library | −120 | $FE89BA |
| $FEE7BC–$FFFD8C | workbench.library | −120 | $FEE7D6 |
| $FFFD92–$FFFE54 | workbench.task | −120 | $FFFDAC |

**KS 3.0 vs KS 3.1:** Module layout is nearly identical (KS 3.1 was derived
from KS 3.0). Address ranges shift slightly. 43 modules in 3.0 vs 44 in 3.1
(KS 3.1 adds one extra module). This is the map to use when debugging the
current AGA boot stall — the A1200 boots KS 3.0 through the same code paths.

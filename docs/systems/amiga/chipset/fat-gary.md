# Fat Gary — Enhanced Address Decoder, Bus Timeout, Resource Registers

Fat Gary replaces the discrete Gary PAL on A3000 and A4000 systems. It adds
three features: a bus timeout mechanism for unmapped addresses, motherboard
resource registers at $DE0000, and a 24-bit bus gate that forwards chip-bus
cycles from the 32-bit CPU domain.

## Address Decode

Fat Gary performs the same address decode as Gary (see [gary.md](gary.md)) for
the 24-bit chip-bus address space. The full decode chain is identical — CIA-A,
CIA-B, custom registers, DMAC, resource registers, chip RAM, ROM, autoconfig —
with the same priority rules.

The difference is what happens when no chip-select matches.

## Bus Timeout (TOENB)

On OCS/ECS machines without Fat Gary, an access to an unmapped address floats —
the CPU reads 0 and continues. This is harmless but slow (the CPU waits for a
response that never comes, then eventually continues).

Fat Gary adds a timeout mechanism:

1. When a bus cycle targets an address in Fat Gary's **nonrange** (no device
   will respond), Fat Gary checks its TOENB register.
2. If TOENB bit 7 is set (the default at power-on), Fat Gary asserts a bus
   error, causing a 68030/040 exception.
3. If TOENB bit 7 is clear, the cycle returns 0 (floating bus, as on OCS).

### Nonrange Addresses

Fat Gary defines the following address ranges as nonrange (unmapped). Any
address that falls in a nonrange and is not claimed by an active chip-select
is a candidate for bus timeout:

- $100000–$1FFFFF when chip RAM is less than 2 MB
- $200000–$5FFFFF (fast RAM region, unless expansion boards claim it)
- $600000–$9FFFFF (PCMCIA common — only claimed on A600/A1200)
- $A00000–$BEFFFF (misc expansion / PCMCIA attribute)
- $C00000–$C7FFFF (slow RAM — not present on A3000)
- $C80000–$DBFFFF (additional slow/expansion area)
- $F00000–$F7FFFF (diagnostic ROM — decoded by motherboard, empty returns 0)

The exact nonrange map depends on model configuration. Fat Gary consults the
same peripheral-present flags as Gary — if DMAC is present, $DD0000 is not
nonrange. If resource registers are present, $DE0000 is not nonrange.

## Resource Registers ($DE0000)

Fat Gary and RAMSEY share the $DE0000–$DEFFFF address range. Three registers
are accessed through a sub-address decode based on the low-order address bits:

| Address bits | Register | Access | Default | Purpose |
|-------------|----------|--------|---------|---------|
| addr64=x, addr2=0 | TIMEOUT | R/W | $00 | Timeout status |
| addr64=x, addr2=1 | TOENB | R/W | $80 | Timeout enable (bit 7 = enable) |
| addr64=1, addr2=0 | COLDBOOT | Read | $80 | Cold-boot detection flag |

**TIMEOUT ($DE0000):** Cleared on power-up. The ROM reads this to detect whether
a previous bus error occurred during probe sequences.

**TOENB ($DE0002):** Bit 7 enables bus timeout. At power-on this is $80 (enabled).
Kickstart clears it during early boot to probe expansion space safely, then
re-enables it after probing is complete.

**COLDBOOT ($DE0040):** Read-only. Returns $80 on a fresh cold boot. The ROM reads
this to distinguish cold boots from warm resets (a warm reset may skip memory
tests and ROM checksum verification).

### Interaction with RAMSEY

RAMSEY occupies the same $DE0000 range but responds to different sub-addresses
(see [ramsey.md](ramsey.md)). Fat Gary handles addr2=0 (timeout) and addr2=1
(toenb); RAMSEY handles its own config/revision registers through separate
addr64/addr2 combinations. The bus wrapper routes bytes to the correct chip
based on the sub-address.

## 24-Bit Bus Gate

The A3000/A4000 CPU runs on a 32-bit bus, but the chip bus (Agnus, Denise,
Paula, CIAs) is 16-bit and 24-bit-addressed. Fat Gary determines whether a
32-bit CPU address should be forwarded to the 24-bit chip bus:

```
forwards_to_24bit_bus = address < $01000000
```

Addresses at or above $01000000 are in the 32-bit domain (fast RAM, Zorro III
expansion) and bypass the chip bus entirely. Below $01000000, the upper 8
address bits are stripped and the remaining 24 bits enter Gary's standard
decode chain.

## Emulator Implications

- Fat Gary adds state (toenb and timeout registers) — unlike Gary, it is not
  pure combinational logic.
- On A3000/A4000, the emulator must check `is_nonrange(addr)` before every
  unmapped access and either generate a bus error exception or return 0,
  depending on TOENB.
- The $DE0000 resource register routing must correctly interleave Fat Gary and
  RAMSEY accesses. Getting the sub-address decode wrong causes KS to
  misidentify the RAMSEY revision or fail the timeout probe.
- The 24-bit bus gate means the emulator's memory dispatcher must first check
  whether the address is above $01000000 (fast RAM / Zorro III, no chip bus
  contention) before entering the chip-bus decode path.
- At power-on, TOENB=$80 means bus timeout is active immediately. The first
  thing KS 2.x/3.x does after reset is clear TOENB so it can safely probe
  expansion space.

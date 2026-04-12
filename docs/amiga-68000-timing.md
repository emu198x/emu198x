# Amiga 68000 Timing Reference

**Purpose:** Cycle-accurate 68000 emulation reference for Amiga-family computers.
Covers bus cycles, prefetch, exception handling, instruction timing, and
Amiga-specific quirks that matter for correct emulation.

**Audience:** Emulator authors who need to get timing right, not just functional
correctness.

**Sources:**
- Motorola M68000 Family Reference (1988) -- cited as `(M68000 Family Ref)`
- Musashi 4.60 emulator core by Karl Stenerud -- cited as `(Musashi)`
- WinUAE by Toni Wilen -- cited as `(WinUAE)`
- Amiga hardware documentation and community knowledge

---

## Table of Contents

1. [CPU Variants in Amigas](#1-cpu-variants-in-amigas)
2. [Bus Cycles](#2-bus-cycles)
3. [Prefetch Queue](#3-prefetch-queue)
4. [Exception Stacking](#4-exception-stacking)
5. [Exception Vector Table](#5-exception-vector-table)
6. [Instruction Timing](#6-instruction-timing)
7. [Worst-Case Instructions](#7-worst-case-instructions)
8. [Interrupt Handling Flow](#8-interrupt-handling-flow)
9. [RESET Pin Timing](#9-reset-pin-timing)
10. [Privilege Transitions](#10-privilege-transitions)
11. [Bus/Address Error Recovery](#11-busaddress-error-recovery)
12. [TAS Instruction Ban](#12-tas-instruction-ban)
13. [VBR / MOVEC (68010+)](#13-vbr--movec-68010)
14. [CACR / Cache Control (68020+)](#14-cacr--cache-control-68020)
15. [Musashi Timing Tables](#15-musashi-timing-tables)
16. [WinUAE Cycle-Exact Core](#16-winuae-cycle-exact-core)
17. [Implementation Checklist](#17-implementation-checklist)

**Appendices:**
- [A. Exception Vector Table (Complete)](#appendix-a-exception-vector-table-complete)
- [B. Instruction Timing Quick-Reference](#appendix-b-instruction-timing-quick-reference)
- [C. Bus Cycle Diagrams](#appendix-c-bus-cycle-diagrams)
- [D. Implementation Checklist (Detail)](#appendix-d-implementation-checklist-detail)

---

## 1. CPU Variants in Amigas

Every Amiga model ships with a Motorola 680x0 CPU. The choice of CPU
determines cache behaviour, exception frame format, available control
registers, and -- critically for emulation -- how bus cycles interact
with custom chip DMA.

### 1.1 CPU Summary Table

| CPU       | Data Bus | Addr Bus | Clock (MHz)  | Cache     | MMU  | FPU      | Amiga Models              |
|-----------|----------|----------|-------------- |-----------|------|----------|---------------------------|
| 68000     | 16-bit   | 24-bit   | 7.09/7.16    | None      | None | None     | A1000, A500, A2000, CDTV  |
| 68EC020   | 32-bit   | 24-bit   | 14.18/14.32  | 256B I$   | None | None     | A1200 (stock)             |
| 68020     | 32-bit   | 32-bit   | 14.18/14.32  | 256B I$   | Ext  | Ext      | A2500, accelerators       |
| 68030     | 32-bit   | 32-bit   | 25-50        | 256B I+D$ | On   | Ext      | A3000, A4000/030          |
| 68040     | 32-bit   | 32-bit   | 25            | 4KB I+D$  | On   | On-chip  | A4000/040                 |
| 68060     | 32-bit   | 32-bit   | 50-75        | 8KB I+D$  | On   | On-chip  | Accelerator cards         |

**Notes:**
- The 68010 was never used in production Amigas but some accelerator
  boards used it. It adds the VBR (vector base register), loop mode,
  and recoverable bus/address errors.
- The 68EC020 in the A1200 has a 24-bit address bus, matching the
  original chipset address space. It does have a 256-byte instruction cache.
- The 68LC040 (used in some A4000 models) lacks the FPU.

### 1.2 Clock Rates

The 68000 clock derives from the system master oscillator:

| Standard | Master Oscillator | CPU Clock (CLK/2)  | Colour Clocks |
|----------|-------------------|--------------------|---------------|
| PAL      | 28.37516 MHz      | 7.09379 MHz        | 3.546895 MHz  |
| NTSC     | 28.63636 MHz      | 7.15909 MHz        | 3.579545 MHz  |

One CPU clock cycle is approximately:
- PAL:  ~141.0 ns
- NTSC: ~139.7 ns

One colour clock equals two CPU clocks. The custom chipset DMA operates
on colour-clock boundaries.

### 1.3 OS Compatibility

| Feature              | 68000 | 68010 | 68020 | 68030 | 68040 | 68060 |
|----------------------|-------|-------|-------|-------|-------|-------|
| KS 1.2/1.3           | Yes   | Yes   | Yes   | Yes   | Patch | Patch |
| KS 2.0+              | Yes   | Yes   | Yes   | Yes   | Yes*  | Yes*  |
| MOVE from SR          | User  | Priv  | Priv  | Priv  | Priv  | Priv  |
| VBR                   | No    | Yes   | Yes   | Yes   | Yes   | Yes   |
| I-Cache               | No    | No    | Yes   | Yes   | Yes   | Yes   |
| D-Cache               | No    | No    | No    | Yes   | Yes   | Yes   |
| Copyback cache        | No    | No    | No    | No    | Yes   | Yes   |

`*` Requires 68040.library / 68060.library for FPU emulation of
unimplemented instructions and cache management.

A critical OS compatibility issue: KS 1.x uses `MOVE from SR` in user
mode. The 68010+ traps this as a privilege violation. The 68010 added
`MOVE from CCR` as the user-mode replacement that reads only the
condition code byte. Emulators must handle this distinction or many
games and demos will crash.

---

## 2. Bus Cycles

The 68000 uses an asynchronous bus with a minimum cycle time of 4 CPU
clocks (8 half-clocks, labelled S0 through S7). Every memory access --
instruction fetch, operand read, operand write -- takes at least one
bus cycle.

### 2.1 Bus Cycle Phases

A bus cycle divides into 8 states (S0-S7), each lasting one half-clock:

```
        S0    S1    S2    S3    S4    S5    S6    S7
CLK:  __/--\__/--\__/--\__/--\__/--\__/--\__/--\__/--\__

AS:   --------\______________________________________/---
DS:   ----------------\______________________________/---
R/W:  --------<=========================================>
ADDR: --------<=========================================>
DATA: (read)  ---------------------------<=====>---------
DATA: (write) ----------------<==================>-------
DTACK:----------------------------\__________/-----------
```

| State | What Happens                                             |
|-------|----------------------------------------------------------|
| S0    | Address and function codes begin to be driven            |
| S1    | Address stabilises                                       |
| S2    | AS (address strobe) asserts; R/W valid; DS asserts (read)|
| S3    | DS asserts (write); slave decodes address                |
| S4    | DTACK sampled on falling edge -- if not asserted, wait   |
| S5    | Data from slave must be valid (read) by end of S5/S6     |
| S6    | CPU latches data (read); AS and DS negate                |
| S7    | Bus released; next cycle can begin in S0                 |

(M68000 Family Ref, MC68000 section "Data Transfer Operations")

### 2.2 Wait States

If DTACK is not asserted by the falling edge of S4, the CPU inserts
**wait states** -- additional pairs of half-clocks (W, W) between S4
and S5. Each wait state adds 2 CPU clocks to the bus cycle.

On the Amiga, Chip RAM access through Agnus involves bus arbitration.
When the CPU accesses Chip RAM and Agnus has the bus for DMA, the CPU
must wait. This is the source of "DMA contention" or "bus stealing."

```
S0 S1 S2 S3 S4 W  W  W  W  W  W  S5 S6 S7
                   ^  ^  ^  ^  ^  ^
                   |  Wait states  |
                   DTACK not ready
```

### 2.3 Read Cycle

During a read cycle, the CPU receives data from memory or a peripheral.
The bus master (CPU) places the address on A1-A23 and drives A0
internally to select upper/lower data strobe for byte operations.

- Word access: both UDS and LDS assert simultaneously
- Byte access (even address, A0=0): UDS only (D8-D15)
- Byte access (odd address, A0=1): LDS only (D0-D7)
- Long word: two consecutive word reads (high word first)

(M68000 Family Ref, "Read Cycle")

### 2.4 Write Cycle

During a write cycle, the CPU places data on the bus. The key timing
difference from reads: data is driven in S3 (not S5), giving the slave
more setup time.

- R/W goes low in S2
- Data valid from S3 onwards
- Slave latches data and asserts DTACK

(M68000 Family Ref, "Write Cycle")

### 2.5 Read-Modify-Write Cycle

The read-modify-write (RMW) cycle is used exclusively by the TAS
instruction. AS remains asserted throughout both the read and write
portions, creating an indivisible (atomic) bus cycle.

```
Read phase:  S0 S1 S2 S3 S4 S5 S6 S7
Write phase: S0 S1 S2 S3 S4 S5 S6 S7
                                      ^
                                      AS stays asserted across both phases
```

This is critical on the Amiga -- see Section 12 (TAS Instruction Ban).

(M68000 Family Ref, "Read-Modify-Write Cycle")

### 2.6 Interrupt Acknowledge Cycle

An interrupt acknowledge (IACK) cycle looks like a read cycle but with
function codes FC0-FC2 all high (CPU space, $7) and the interrupt level
encoded on A1-A3. A4-A23 are all high.

The peripheral responds with an 8-bit vector number on D0-D7. If no
peripheral responds and VPA is asserted instead, the CPU uses the
autovector for that interrupt level (vectors 25-31).

If neither DTACK nor VPA is asserted, the CPU waits indefinitely. If
BERR is asserted, the CPU generates a spurious interrupt (vector 24).

### 2.7 Bus Cycle Timing on the Amiga

The Amiga's custom chipset (Agnus) arbitrates bus access between the
CPU and DMA channels. The bus runs on colour-clock boundaries (every
2 CPU clocks). CPU access to Chip RAM is subject to contention:

- **No contention:** CPU gets the bus immediately. 4 clocks minimum.
- **Contention:** CPU must wait for Agnus to release the bus. Each DMA
  slot that blocks the CPU adds 2 clocks.
- **Fast RAM:** No contention. Always 4 clocks (or fewer with 32-bit
  memory on 68020+).

The practical effect: Chip RAM access typically takes 4-6 clocks per
word, depending on DMA activity. During heavy DMA (screen display,
blitter, disk), the CPU can be starved of bus cycles entirely.

---

## 3. Prefetch Queue

The 68000's prefetch mechanism is one of the most commonly
misunderstood aspects of the processor. Getting it wrong breaks
protection schemes, self-modifying code, and timing-sensitive software.

### 3.1 The 68000 Prefetch Pipeline

The 68000 has a two-word prefetch pipeline: **IR** (instruction register)
and **IRC** (instruction register capture). These are not a cache -- they
are a simple pipeline that overlaps instruction fetch with execution.

```
IR:  Contains the opcode currently being decoded/executed
IRC: Contains the next word prefetched from the instruction stream
```

The pipeline works as follows:

1. At the start of instruction execution, IR holds the opcode word.
2. While the CPU executes the instruction, it prefetches the next word
   from (PC+2) into IRC.
3. When the instruction completes, IRC is moved to IR and a new
   prefetch fills IRC from (PC+2).

This means the CPU always reads **one word ahead** of what it is
currently executing.

(WinUAE newcpu.cpp: `regs.ir`, `regs.irc` model this pipeline explicitly)

### 3.2 The RESET + JMP Trick (Prefetch Longword Read)

A well-known consequence of the prefetch: after a hardware reset, the
CPU reads the initial SSP from address $000000 (longword) and the
initial PC from address $000004 (longword). But the first instruction
fetch happens at the PC loaded from $000004, and the CPU prefetches
the next word from PC+2.

The "JMP (An)" trick exploits this: because JMP calculates the
effective address before the prefetch of the next instruction completes,
you can observe which word the CPU has already fetched into IRC. This
matters for copy protection and hardware detection.

More concretely, the prefetch affects timing like this:

```
JMP $xxxx:
  1. Fetch opcode word           -> IR = JMP opcode
  2. Fetch extension word(s)     -> address decoded
  3. Prefetch from new PC        -> IRC filled from target
  4. Prefetch from new PC+2      -> IRC refilled, IR gets previous IRC
```

The key insight: the JMP instruction itself does a prefetch from the
**destination** address before execution truly completes. This prefetch
is visible in the cycle count and can be observed by hardware.

### 3.3 Prefetch and Self-Modifying Code

Because the 68000 prefetches one word ahead, self-modifying code that
writes to the instruction immediately following the current instruction
may not take effect. The word has already been fetched into IRC.

Example:
```asm
    MOVE.W #$4E71, next_instr   ; Write NOP to next instruction
next_instr:
    ILLEGAL                      ; This was already prefetched!
```

If the MOVE writes to the location of `next_instr` while that word
is already in IRC, the CPU executes the old instruction (ILLEGAL), not
the NOP. This is **by design** on the 68000 and is well-documented
behaviour.

On the 68020+ with instruction cache, the situation is different --
the cache must be explicitly invalidated. See Section 14.

### 3.4 The 68010 Loop Mode

The 68010 adds a "loop mode" that acts as a tiny 3-word instruction
cache for DBcc loops. When a DBcc instruction loops back to the
immediately preceding instruction, the 68010 can execute the loop body
without additional instruction fetches, saving bus cycles.

Eligible instructions: most single-word instructions (MOVE, ADD, SUB,
etc.) when followed by DBcc with a displacement of -4 (back to the
previous instruction).

This is not a true cache -- it only works for the specific pattern of
"one instruction + DBcc" and saves the prefetch cycles for the loop
body.

### 3.5 The 68020 Instruction Cache

The 68020 has a 256-byte direct-mapped instruction cache, organised as
64 longword entries. Each entry contains:

- 24-bit tag (upper address bits)
- FC2 bit (user/supervisor)
- Valid bit
- 32 bits of instruction data (two 16-bit words)

The cache is controlled by the CACR register (see Section 14). Key
properties:

- **Instruction-only:** No data cache on the 68020. Data accesses
  always go to the bus.
- **Write-through:** Writes pass through to memory. But the cache
  is invalidated for the written address.
- **32-bit fetch:** The 68020 fetches instructions on longword
  boundaries, getting two instruction words per bus cycle.
- **50% hit rate even when disabled:** The bus controller holds the
  last fetched longword, so even with cache disabled, the second word
  of a longword fetch is available without a bus cycle (up to 50%
  "hit rate" with cache off).

(M68000 Family Ref, MC68020 "On-Chip Instruction Cache")

### 3.6 Emulator Implementation

WinUAE models the prefetch pipeline with three levels of accuracy
(WinUAE newcpu.cpp, lines 1024-1092):

1. **cpu_cycle_exact** (`get_word_ce000_prefetch`): Full cycle-exact
   prefetch with bus cycle simulation. Every bus access takes real
   cycles and interleaves with DMA.
2. **cpu_memory_cycle_exact** (`get_word_000_prefetch`): Prefetch
   pipeline modelled, bus cycles counted, but no interleaving with
   DMA at the sub-instruction level.
3. **cpu_compatible** (`get_word_000_prefetch`): Prefetch pipeline
   modelled for correctness but no cycle-level bus simulation.
4. **Normal** (no prefetch): Direct memory reads. Fast but incorrect
   for timing-sensitive software.

Musashi uses `CPU_PREF_ADDR` and `CPU_PREF_DATA` to model a simple
prefetch buffer (Musashi m68kcpu.h, line 367-368) but does not model
the full IR/IRC pipeline.

---

## 4. Exception Stacking

Exceptions push state onto the supervisor stack. The format depends on
the CPU model and the type of exception.

### 4.1 Exception Groups

Exceptions are classified into three groups that determine priority
and when they are processed:

| Group | Priority | Exceptions                                       | When Detected           |
|-------|----------|--------------------------------------------------|-------------------------|
| 0     | Highest  | Reset, Bus Error, Address Error                  | During bus cycle        |
| 1     | Middle   | Trace, Interrupt, Illegal, Privilege Violation   | Between instructions    |
| 2     | Lowest   | TRAP, TRAPV, CHK, Zero Divide                   | During execution        |

**Group 0** exceptions abort the current bus cycle. Reset is special --
it restarts the processor entirely.

**Group 1** exceptions are detected after instruction completion (or at
instruction boundaries). They stack the address of the next instruction
to be executed.

**Group 2** exceptions are detected during instruction execution. They
stack the address of the next instruction (the one after the trapping
instruction).

### 4.2 68000 Stack Frames

The 68000 uses two stack frame formats:

#### Normal Stack Frame (6 bytes / 3 words)

Used for Group 1 and Group 2 exceptions (except bus/address error):

```
        15                              0
SSP --> |          Status Register       |  +0
        |     Program Counter (high)     |  +2
        |     Program Counter (low)      |  +4
```

The PC pushed is the address of the next instruction to execute after
the exception handler returns (via RTE).

(Musashi m68kcpu.h, `m68ki_stack_frame_3word()`: pushes PC as 32-bit,
then SR as 16-bit)

#### Bus/Address Error Stack Frame (14 bytes / 7 words)

Used for Group 0 exceptions (vector 2 = bus error, vector 3 = address
error):

```
        15                              0
SSP --> |  R/W | I/N |   Function Code  |  +0   (additional info word)
        |     Access Address (high)      |  +2
        |     Access Address (low)       |  +4
        |       Instruction Register     |  +6
        |          Status Register       |  +8
        |     Program Counter (high)     |  +10
        |     Program Counter (low)      |  +12
```

| Field   | Bits   | Meaning                                          |
|---------|--------|--------------------------------------------------|
| R/W     | Bit 4  | 0 = write, 1 = read                              |
| I/N     | Bit 3  | 0 = instruction fetch, 1 = not instruction       |
| FC      | Bits 2-0 | Function code at time of error                 |

**Undocumented:** The upper 11 bits of the additional info word contain
the opcode that was being executed (bits 15-5 of the opcode). WinUAE
replicates this: `mode |= last_op_for_exception_3 & ~31;`
(WinUAE newcpu.cpp, line 2802).

The PC pushed for bus/address errors is **not necessarily** the address
of the faulting instruction. It may point to the instruction being
executed, or it may have advanced due to extension word fetches. This
is why 68000 bus errors are considered **unrecoverable** -- you cannot
reliably determine where to resume.

(Musashi m68kcpu.h, `m68ki_stack_frame_buserr()`)

### 4.3 68010 Stack Frames

The 68010 uses two formats:

#### Format $0 Stack Frame (8 bytes / 4 words)

Used for all exceptions except bus/address error:

```
        15                              0
SSP --> |          Status Register       |  +0
        |     Program Counter (high)     |  +2
        |     Program Counter (low)      |  +4
        | Format($0) | Vector Offset     |  +6
```

The format/vector word encodes:
- Bits 15-12: Frame format ($0)
- Bits 11-0: Vector offset (vector number * 4)

#### Format $8 Stack Frame (58 bytes / 29 words)

Used for bus/address error on the 68010. This is the key improvement
over the 68000 -- it contains enough state to restart the faulting
instruction after the error handler fixes the problem (virtual memory
support).

```
        15                              0
SSP --> |          Status Register       |  +0
        |     Program Counter (high)     |  +2
        |     Program Counter (low)      |  +4
        | Format($8) | Vector Offset     |  +6
        |       Special Status Word      |  +8
        |     Fault Address (high)       |  +10
        |     Fault Address (low)        |  +12
        |       Data Output Buffer       |  +14
        |   (reserved / not written)     |  +16
        |       Data Input Buffer        |  +18
        |   (reserved / not written)     |  +20
        |    Instruction Input Buffer    |  +22
        |     Version / Internal (16)    |  +24
        |           ...                  |
        |   Internal Information (x16)   |  +26 to +56
```

The Special Status Word (SSW) on the 68010:

| Bit   | Name | Meaning                                         |
|-------|------|-------------------------------------------------|
| 15-14 | --   | Reserved                                        |
| 13    | RR   | Rerun flag (1 = rerun the faulting bus cycle)    |
| 12    | DF   | Data fetch (1 = data cycle, 0 = instruction)    |
| 11    | --   | Reserved                                        |
| 10    | HB   | High byte transfer                              |
| 9     | BY   | Byte transfer                                   |
| 8     | RW   | Read/Write (1 = read, 0 = write)                |
| 7-5   | --   | Reserved                                        |
| 4     | --   | Reserved                                        |
| 3     | --   | Reserved                                        |
| 2-0   | FC   | Function code                                   |

(WinUAE newcpu.cpp, `Exception_ce000()`, lines 2814-2838)
(Musashi m68kcpu.h, `m68ki_stack_frame_1000()`)

### 4.4 68020+ Stack Frames

The 68020 introduces several frame formats:

| Format | Size (words) | Used For                                    |
|--------|-------------|---------------------------------------------|
| $0     | 4           | Normal (interrupts, traps, illegal, etc.)   |
| $1     | 4           | Throwaway (interrupt on M-stack switch)      |
| $2     | 6           | Instruction-related exceptions (TRAP, CHK)   |
| $A     | 16          | Short bus fault (at instruction boundary)    |
| $B     | 46          | Long bus fault (mid-instruction)             |

Format $2 adds a 32-bit "instruction address" field -- the address of
the instruction that caused the exception. This allows the exception
handler to examine the faulting instruction.

Formats $A and $B contain enough internal state to fully restart the
faulting instruction, enabling virtual memory implementations.

(Musashi m68kcpu.h: `m68ki_stack_frame_0000()`, `m68ki_stack_frame_0001()`,
`m68ki_stack_frame_0010()`, `m68ki_stack_frame_1010()`,
`m68ki_stack_frame_1011()`)

### 4.5 Exception Processing Steps

Exception processing follows these steps on all 680x0 CPUs:

1. **Save SR internally.** A copy of the current status register is
   made before any changes.
2. **Set S bit.** The processor enters supervisor mode. T0 and T1
   (trace) bits are cleared to prevent tracing the exception handler.
3. **Update interrupt mask** (for interrupts and reset only).
4. **Determine vector number:**
   - Interrupts: IACK cycle reads vector from peripheral
   - Internal exceptions: vector number from CPU logic
   - Autovector: VPA asserted during IACK -> use autovector
5. **Stack the exception frame** on the supervisor stack (SSP or ISP).
6. **Read the exception vector** from the vector table (address =
   VBR + vector_number * 4).
7. **Load PC** from the vector and begin executing the handler.

(M68000 Family Ref, MC68020 "Exception Processing Sequence")
(Musashi m68kcpu.h, `m68ki_init_exception()`)

### 4.6 Exception Timing

Exception processing consumes a fixed number of clock cycles depending
on the exception type and CPU model. These cycles are **in addition to**
any bus cycles required for stack writes and vector reads.

**68000 exception cycle counts:**

| Exception                    | Cycles | Reads | Writes |
|------------------------------|--------|-------|--------|
| Reset (SSP + PC fetch)       | 40     | 6     | 0      |
| Bus Error                    | 50     | 4     | 7      |
| Address Error                | 50     | 4     | 7      |
| Illegal Instruction          | 34     | 4     | 3      |
| Zero Divide                  | 38     | 4     | 3      |
| CHK                          | 40     | 4     | 3      |
| TRAPV                        | 34     | 4     | 3      |
| Privilege Violation          | 34     | 4     | 3      |
| Trace                        | 34     | 4     | 3      |
| Line 1010 Emulator           | 34     | 4     | 3      |
| Line 1111 Emulator           | 34     | 4     | 3      |
| Uninitialized Interrupt       | 44     | 5     | 3      |
| Spurious Interrupt            | 44     | 5     | 3      |
| Interrupt Autovector          | 44     | 5     | 3      |
| TRAP #n                       | 34     | 4     | 3      |

(Musashi m68kcpu.c, `m68ki_exception_cycle_table[0][]`)

**68010 exception cycle counts (differences from 68000):**

| Exception                    | 68010 Cycles |
|------------------------------|-------------|
| Bus Error                    | 126         |
| Address Error                | 126         |
| Illegal Instruction          | 38          |
| Zero Divide                  | 44          |
| CHK                          | 44          |
| Privilege Violation          | 38          |
| Trace                        | 38          |
| Interrupt Autovector          | 46          |
| TRAP #n                       | 38          |

The 68010 bus/address error takes 126 cycles because it must save the
entire 29-word stack frame containing internal state.

(Musashi m68kcpu.c, `m68ki_exception_cycle_table[1][]`)

---

## 5. Exception Vector Table

The exception vector table occupies the first 1024 bytes of memory
(addresses $000000-$0003FF) on the 68000. On the 68010+, the VBR
(vector base register) can relocate it.

Each entry is a 32-bit pointer (address of the exception handler).
Vector number N is at address VBR + (N * 4).

### 5.1 Amiga Usage

The Amiga Kickstart ROM initialises the vector table at boot. Key
vectors used by AmigaOS:

| Vector | Address  | Exception            | Amiga Usage                   |
|--------|----------|----------------------|-------------------------------|
| 0      | $000     | Reset SSP            | Initial supervisor stack      |
| 1      | $004     | Reset PC             | ROM entry point               |
| 2      | $008     | Bus Error            | Guru Meditation               |
| 3      | $00C     | Address Error        | Guru Meditation               |
| 4      | $010     | Illegal Instruction  | Emulation (e.g., 68040.lib)   |
| 5      | $014     | Zero Divide          | trap to Exec                  |
| 6      | $018     | CHK                  | trap to Exec                  |
| 7      | $01C     | TRAPV                | trap to Exec                  |
| 8      | $020     | Privilege Violation  | trap to Exec                  |
| 9      | $024     | Trace                | Debug / Enforcer              |
| 10     | $028     | Line 1010 Emulator   | Line-A dispatch               |
| 11     | $02C     | Line 1111 Emulator   | Line-F (FPU/coprocessor)      |
| 24     | $060     | Spurious Interrupt   | Hardware error                |
| 25     | $064     | Level 1 Autovector   | Software int (TBE, DSKBLK, SOFTINT) |
| 26     | $068     | Level 2 Autovector   | CIA-A / PORTS (I/O)           |
| 27     | $06C     | Level 3 Autovector   | Copper, VBLANK, Blitter       |
| 28     | $070     | Level 4 Autovector   | Audio channels                |
| 29     | $074     | Level 5 Autovector   | Disk / Serial (RBF, DSKSYNC) |
| 30     | $078     | Level 6 Autovector   | CIA-B / EXTER                 |
| 31     | $07C     | Level 7 Autovector   | NMI (active edge only)        |
| 32-47  | $080-$0BC| TRAP #0 - TRAP #15   | OS/library dispatching        |
| 48-63  | $0C0-$0FC| FP / MMU exceptions  | 68881/68882 support           |
| 64-255 | $100-$3FC| User vectors         | Available for peripherals     |

The Amiga interrupt system uses autovectors exclusively. Paula
generates the interrupt request lines (IPL0-IPL2), and the CPU reads
the autovector for the appropriate level. The actual source within
each level is determined by reading Paula's INTREQR register in the
interrupt handler.

See [Appendix A](#appendix-a-exception-vector-table-complete) for the
complete 256-entry table.

---

## 6. Instruction Timing

### 6.1 Cycle Count Notation

Instruction timing is expressed as:

```
T(R/W)
```

Where:
- **T** = total clock cycles
- **R** = number of read bus cycles
- **W** = number of write bus cycles

Example: `8(2/0)` means 8 clock cycles with 2 reads and 0 writes.

On the 68000, each bus cycle takes a minimum of 4 clocks. Internal
processing that does not require bus access runs in parallel with
prefetch and does not add to the total unless it exceeds the bus time.

### 6.2 Effective Address Calculation Timing (68000)

These times are **added** to the base instruction time when the
instruction uses an effective address other than register direct:

| Addressing Mode         | Byte/Word    | Long         |
|-------------------------|------------- |--------------|
| Dn (data register)      | 0(0/0)       | 0(0/0)       |
| An (address register)   | 0(0/0)       | 0(0/0)       |
| (An)                    | 4(1/0)       | 8(2/0)       |
| (An)+                   | 4(1/0)       | 8(2/0)       |
| -(An)                   | 6(1/0)       | 10(2/0)      |
| d16(An)                 | 8(2/0)       | 12(3/0)      |
| d8(An,Xn)               | 10(2/0)      | 14(3/0)      |
| xxx.W                   | 8(2/0)       | 12(3/0)      |
| xxx.L                   | 12(3/0)      | 16(4/0)      |
| d16(PC)                 | 8(2/0)       | 12(3/0)      |
| d8(PC,Xn)               | 10(2/0)      | 14(3/0)      |
| #imm                    | 4(1/0)       | 8(2/0)       |

### 6.3 How to Calculate Total Instruction Time

For most instructions:

```
Total = Base_cycles + EA_source_cycles + EA_dest_cycles
```

But this is a simplification. The Musashi timing table
(m68k_in.c, `M68KMAKE_TABLE_BODY`) gives the base cycle count for
each opcode with each EA mode already factored in. The `USE_CYCLES`
calls in the instruction handlers add data-dependent cycles (shifts,
multiply, MOVEM register count).

### 6.4 Clock Speed and Real Time

At 7.09 MHz (PAL), one clock cycle is ~141 ns. One scanline is 227.5
colour clocks = 455 CPU clocks. One frame (PAL, non-interlaced) is
312.5 scanlines = 142,187.5 CPU clocks per field.

| Event              | Colour Clocks | CPU Clocks | Time (PAL)  |
|--------------------|---------------|------------|-------------|
| One CPU clock      | 0.5           | 1          | ~141 ns     |
| One colour clock   | 1             | 2          | ~281 ns     |
| One DMA slot       | 1             | 2          | ~281 ns     |
| One scanline       | 227.5         | 455        | ~63.6 us    |
| One field (PAL)    | 71,093.75     | 142,187.5  | ~19.98 ms   |
| One frame (PAL)    | 142,187.5     | 284,375    | ~39.97 ms   |

### 6.5 Chip RAM vs Fast RAM Contention

When the CPU accesses Chip RAM, it competes with custom chip DMA for
bus access. Each DMA channel (bitplane, sprite, audio, disk, copper,
blitter) consumes bus slots that the CPU cannot use.

**No contention (Fast RAM):** Instruction timing matches the data
sheet exactly. Every bus cycle takes exactly 4 clocks.

**With contention (Chip RAM):** Each CPU bus cycle may be delayed by
0, 2, 4, or more clocks depending on DMA activity. The worst case
is when every colour clock is consumed by DMA -- the CPU gets zero
bus access and stalls completely.

For emulation, there are two approaches:

1. **Instruction-level accounting:** Count total cycles per instruction
   and add DMA contention penalties. Simpler, approximately correct.
2. **Cycle-exact:** Model every bus cycle independently, checking DMA
   contention for each access. This is what WinUAE does in CE mode.

---

## 7. Worst-Case Instructions

Some instructions have variable execution times that depend on their
operands. These are important for worst-case interrupt latency
calculations and for correct timing of tight loops.

### 7.1 MULS (Signed Multiply)

**Base time:** 38+2n cycles (68000), where n depends on the source
operand.

For MULS, n is the number of **transitions** (0->1 or 1->0) in the
source operand. Each transition adds 2 cycles.

```
Minimum: 38 cycles (all zeros or all ones, no transitions)
Maximum: 70 cycles (alternating 0/1 pattern, 16 transitions)
```

Musashi implements this by counting transitions:
```c
// (Musashi m68k_in.c, M68KMAKE_OP(muls, 16, ., d))
uint c = 0;
for (uint y = x, f = 0; y; y >>= 1) {
    if ((y & 1) != f) {
        c += 2;
        f = 1 - f;
    }
}
USE_CYCLES(c);
```

### 7.2 MULU (Unsigned Multiply)

**Base time:** 38+2n cycles (68000), where n is the number of **set
bits** in the source operand.

```
Minimum: 38 cycles (source = 0, no set bits)
Maximum: 70 cycles (source = $FFFF, 16 set bits)
```

Musashi implementation:
```c
// (Musashi m68k_in.c, M68KMAKE_OP(mulu, 16, ., d))
uint c = 0;
for (uint y = x; y; y >>= 1) {
    if (y & 1) {
        c += 2;
    }
}
USE_CYCLES(c);
```

### 7.3 DIVS (Signed Divide)

**Worst-case timing:** 158 cycles (68000), 122 cycles (68010).

DIVS divides a 32-bit dividend by a 16-bit divisor, producing a
16-bit quotient and 16-bit remainder. The timing is the worst case
listed in the Musashi timing table. The actual time varies with the
operands but is always at least ~120 cycles on the 68000.

Division by zero triggers a Zero Divide exception (vector 5) instead
of completing.

### 7.4 DIVU (Unsigned Divide)

**Worst-case timing:** 140 cycles (68000), 108 cycles (68010).

Like DIVS but unsigned. The variation range is similarly large.

### 7.5 MOVEM (Move Multiple Registers)

MOVEM transfers multiple registers to/from memory in a single
instruction. The cycle count depends on the number of registers
selected in the register mask.

**68000 timing:**
- MOVEM.W to memory: 8 + 4n cycles (n = register count)
- MOVEM.L to memory: 8 + 8n cycles
- MOVEM.W from memory: 12 + 4n cycles
- MOVEM.L from memory: 12 + 8n cycles

Where n is the number of set bits in the register list word.

Worst case: MOVEM.L D0-D7/A0-A6,-(SP) = 8 + (15 * 8) = 128 cycles.

Musashi uses `CYC_MOVEM_W` and `CYC_MOVEM_L` constants (shifted by
register count) for this calculation:

```c
// (Musashi m68k_in.c, M68KMAKE_OP(movem, 32, re, pd))
for (; i < 16; i++)
    if (register_list & (1 << i)) {
        ea -= 4;
        m68ki_write_16(ea + 2, REG_DA[15 - i] & 0xFFFF);
        m68ki_write_16(ea, (REG_DA[15 - i] >> 16) & 0xFFFF);
        count++;
    }
AY = ea;
USE_CYCLES(count << CYC_MOVEM_L);
```

### 7.6 Shift/Rotate Instructions

On the 68000 and 68010, shift and rotate instructions add 2 cycles
per shift position:

```
ASL/ASR/LSL/LSR/ROL/ROR Dn,Dm:  6 + 2*shift cycles (byte/word)
                                  8 + 2*shift cycles (long)
```

Shift count comes from a register (modulo 64), so the worst case is
6 + 2*63 = 132 cycles for byte/word, 8 + 2*63 = 134 cycles for long.

On the 68020+, shift count does not affect timing -- these instructions
execute in a fixed number of cycles regardless of shift distance.

### 7.7 Interrupt Latency

The worst-case interrupt latency is the time from when the interrupt
signal is asserted until the CPU begins executing the interrupt handler.
This includes:

1. **Current instruction completion:** The longest instruction that
   cannot be interrupted. Worst case is DIVS at 158 cycles.
2. **Exception processing:** 44 cycles for autovectored interrupt.
3. **Prefetch cycles:** ~8 cycles for initial handler fetches.

**Total worst case: ~210 cycles** (approximately 30 microseconds at
7.09 MHz).

In practice, the Amiga uses VBLANK (Level 3) interrupts for most OS
timing, and Level 6 (CIA-B) for critical timing. Level 7 (NMI) has
the highest priority and cannot be masked.

---

## 8. Interrupt Handling Flow

The Amiga's interrupt system uses the 68000's autovector mechanism
exclusively. Here is the complete flow from interrupt assertion to
handler entry.

### 8.1 Signal Flow

```
                         +-------+
  Paula INTREQR -------->| Paula |-----> IPL0 \
                         |       |-----> IPL1  }---> 68000 IPL inputs
                         |       |-----> IPL2 /
                         +-------+
```

Paula encodes interrupt requests into a 3-bit priority level (IPL0-IPL2).
The CPU samples IPL on every clock cycle.

### 8.2 Interrupt Detection

The CPU detects a pending interrupt when:

1. The encoded IPL level is **higher** than the current interrupt mask
   (bits I2-I0 in the SR), OR
2. The IPL level is 7 (NMI -- level 7 is non-maskable, always
   recognised regardless of the mask).

The CPU does **not** service interrupts immediately. It waits until the
current instruction completes.

### 8.3 Interrupt Acknowledgement Sequence

After the current instruction finishes:

1. **Make internal copy of SR** (including current interrupt mask).
2. **Set S bit** (enter supervisor mode).
3. **Clear T bits** (disable trace).
4. **Update interrupt mask** to the level being serviced.
5. **Perform IACK bus cycle:**
   - FC0-FC2 = $7 (CPU space)
   - A1-A3 = interrupt level
   - A4-A23 = all high
   - Wait for DTACK or VPA
6. **On Amiga:** VPA is asserted by Gary/Gayle, so the CPU uses the
   **autovector** for that level:
   - Level 1 -> Vector 25 (address $064)
   - Level 2 -> Vector 26 (address $068)
   - Level 3 -> Vector 27 (address $06C)
   - Level 4 -> Vector 28 (address $070)
   - Level 5 -> Vector 29 (address $074)
   - Level 6 -> Vector 30 (address $078)
   - Level 7 -> Vector 31 (address $07C)
7. **Push exception frame** (3 words on 68000, 4 words on 68010+):
   - Push PC (32-bit)
   - Push SR (16-bit)
   - Push format/vector word (68010+ only)
8. **Read handler address** from vector table (32-bit read).
9. **Load PC** with handler address.
10. **Begin executing handler.**

### 8.4 Timing Breakdown (68000, Autovector)

The WinUAE implementation shows the cycle breakdown for CE mode:

```c
// (WinUAE newcpu.cpp, Exception_ce000())
start = 6;  // 6 cycles before SR save (for interrupts)
// Stack writes: 3 words = 3 * 4 = 12 cycles (minimum)
// IACK cycle: 4 cycles minimum (+ E-clock sync for VPA)
// Vector fetch: 2 * 4 = 8 cycles (longword read)
// Prefetch at handler: 4 cycles
```

Total: 44 cycles (as per the exception timing table), plus any DMA
contention on Chip RAM accesses.

### 8.5 IPL Sampling

The 68000 samples the IPL lines once per clock cycle, but the actual
comparison with the interrupt mask happens at a specific point in the
instruction execution. There is a **2-cycle latency** between the IPL
lines changing and the CPU recognising the change.

WinUAE models this with `ipl_fetch_now()` and `ipl_fetch_next()` calls
in the generated CPU code (cpuemu_13.cpp). The IPL is sampled at
specific points during instruction execution:

```c
// (WinUAE cpuemu_13.cpp, typical instruction)
regs.ir = regs.irc;
opcode = regs.ir;
ipl_fetch_next();  // Sample IPL for next instruction boundary
get_word_ce000_prefetch(6);
```

### 8.6 Amiga Interrupt Sources by Level

| IPL Level | Amiga Sources                                    |
|-----------|--------------------------------------------------|
| 1         | TBE (serial transmit), DSKBLK (disk), SOFTINT    |
| 2         | CIA-A (PORTS: keyboard, gameport, drive ID)       |
| 3         | Copper, VBLANK, Blitter finished                  |
| 4         | Audio channel 0-3                                 |
| 5         | RBF (serial receive), DSKSYNC (disk sync)         |
| 6         | CIA-B (EXTER: parallel, TOD alarm)                |
| 7         | NMI (active edge trigger, directly on pin)        |

Paula's INTENA register controls which interrupt sources are enabled.
INTREQ shows which are currently pending. The interrupt handler must
read INTREQR to determine the source and clear it by writing to
INTREQ.

---

## 9. RESET Pin Timing

The RESET pin on the 68000 is **bidirectional**. It can be driven
externally to reset the CPU, or driven by the CPU (via the RESET
instruction) to reset external peripherals.

### 9.1 External Reset (Power-On / Hardware Reset)

Both RESET and HALT must be asserted simultaneously for at least
**10 CPU clocks** (the data sheet specifies a minimum of 10 clocks
for the pulse width, tHRPW).

For power-up, the CPU must be held in reset for at least **100
milliseconds** to allow internal oscillator stabilisation.

(M68000 Family Ref, "HALT/RESET Pulse Width": tHRPW = 10 clocks
minimum. Note 4: "For power-up, the MC68000 must be held in the
reset state for 100 milliseconds.")

When external reset is released, the CPU:

1. Reads the initial SSP from address $000000 (longword, 2 bus cycles)
2. Reads the initial PC from address $000004 (longword, 2 bus cycles)
3. Prefetches the first instruction from the loaded PC
4. Begins execution

The SR is initialised to:
- S = 1 (supervisor mode)
- I2-I0 = 7 (all interrupts masked)
- T = 0 (trace off)

All other registers are undefined after reset.

### 9.2 RESET Instruction (Internal Reset)

The RESET instruction asserts the RESET pin as an **output** for
**124 clock cycles**. This resets all external devices (custom chips,
CIAs, etc.) but does **not** affect the CPU's internal state.

The CPU continues execution after the RESET instruction completes.
No registers, no SR bits, no PC -- nothing internal changes.

```
RESET instruction: 132 cycles total (68000)
  - 124 clocks with RESET asserted
  - 8 clocks overhead (instruction fetch, etc.)
```

The RESET instruction is privileged -- it generates a privilege
violation exception if executed in user mode.

(Musashi m68k_in.c: `USE_CYCLES(CYC_RESET)`)

### 9.3 Amiga Reset Behaviour

On the Amiga, asserting RESET clears:
- All custom chip registers (Agnus, Denise, Paula)
- CIA-A and CIA-B
- Gary/Gayle
- Any Zorro expansion cards

It does **not** clear:
- Chip RAM contents
- Fast RAM contents
- CPU registers (when using RESET instruction)

The Amiga keyboard controller (8520 CIA-A) detects CTRL+Amiga+Amiga
and drives the RESET and HALT lines externally, causing a full CPU
reset.

---

## 10. Privilege Transitions

The 68000 has two privilege levels: **user mode** (S=0) and
**supervisor mode** (S=1). The S bit in the status register
controls the current mode.

### 10.1 Entering Supervisor Mode

Supervisor mode is entered through:
- Exception processing (always sets S=1)
- System reset (S=1 after reset)

There is no instruction that directly sets the S bit from user mode.
The standard technique is to execute a TRAP instruction, which causes
an exception that switches to supervisor mode.

### 10.2 Returning to User Mode

Supervisor-to-user transitions happen via:
- RTE (return from exception) -- restores SR from stack, which may
  clear S
- MOVE to SR -- can directly clear the S bit (privileged instruction)
- ANDI/ORI/EORI to SR -- can modify S bit (privileged instructions)

### 10.3 Stack Pointer Switching

When the privilege level changes:
- **User -> Supervisor:** The current A7 value is saved as USP, and
  A7 is loaded from SSP (or ISP/MSP on 68020+).
- **Supervisor -> User:** The current A7 value is saved as SSP, and
  A7 is loaded from USP.

The 68020+ has three stack pointers:
- USP: User Stack Pointer (A7 in user mode)
- ISP: Interrupt Stack Pointer (A7 in supervisor mode when M=0)
- MSP: Master Stack Pointer (A7 in supervisor mode when M=1)

### 10.4 Privileged Instructions

The following instructions generate a privilege violation exception
(vector 8) when executed in user mode:

| Instruction        | CPU    | Notes                              |
|--------------------|--------|------------------------------------|
| STOP               | 68000+ | Stop and wait for interrupt         |
| RESET              | 68000+ | Reset external devices              |
| RTE                | 68000+ | Return from exception               |
| MOVE to SR         | 68000+ | Write status register               |
| MOVE from SR       | 68010+ | Read SR (user mode on 68000 only!) |
| MOVE USP           | 68000+ | Read/write user stack pointer       |
| ANDI/ORI/EORI SR   | 68000+ | Modify status register              |
| MOVEC              | 68010+ | Move control register               |
| MOVES              | 68010+ | Move with function code             |

**Critical difference:** `MOVE from SR` is a **user-mode instruction**
on the 68000 but becomes **privileged** on the 68010+. The 68010 adds
`MOVE from CCR` as the user-mode replacement that reads only the
condition code byte.

This breaks software written for the 68000 that reads SR in user mode.
Emulators must check the CPU model to determine whether to trap.

### 10.5 68010+ Privilege Restrictions

The 68010 tightens the privilege model:

- MOVE from SR becomes privileged (see above)
- MOVEC instruction added for control register access (VBR, SFC, DFC)
- Bus/address error frames are recoverable (enabling virtual memory)

The 68020 adds:
- Coprocessor interface instructions are partially privileged
- Cache control (CINV, CPUSH on 68040) is privileged
- Module call/return (CALLM/RTM) -- rarely used

---

## 11. Bus/Address Error Recovery

### 11.1 68000: Unrecoverable Errors

On the 68000, bus and address errors are **not recoverable**. The
exception frame does not contain enough information to restart the
faulting instruction:

- The PC value on the stack may have advanced past the faulting
  instruction (extension words were already fetched)
- Internal microcode state is not saved
- The access address may be imprecise

If a bus error occurs during exception processing of another bus error,
the CPU enters the **halted state** and asserts the HALT pin. Only an
external reset can recover from a double bus fault.

```
Bus error during normal execution -> Exception processing
Bus error during exception processing -> HALT (double bus fault)
```

(M68000 Family Ref, "Halted Processing")

### 11.2 68010: Recoverable Errors

The 68010's 29-word (58-byte) bus error stack frame (format $8)
contains the complete internal state needed to restart the faulting
instruction:

- Instruction input buffer (IRC)
- Data input/output buffers
- Fault address
- Special status word (SSW)
- Internal microcode state (16 words)

The RTE instruction on the 68010 can reload this state and restart
the faulting bus cycle. This is the foundation for virtual memory
support.

The SSW tells the OS what kind of access faulted:
- Read vs write
- Instruction fetch vs data access
- Byte vs word access
- Function code (user/supervisor, program/data)

### 11.3 68020+: Full VM Support

The 68020 uses format $A (short, 16 words) and format $B (long, 46
words) bus fault frames. Format $A is used when the error occurs at
an instruction boundary; format $B when it occurs mid-instruction.

The 68030 adds a full MMU, making virtual memory practical. The MMU
can translate addresses, protect pages, and generate bus errors for
page faults -- the CPU handles the rest via format $B frames.

The 68040 streamlines this further with an on-chip MMU and simplified
exception frames.

### 11.4 Address Error Specifics

An address error occurs when the CPU attempts a word or longword
access to an odd address. This includes:

- Instruction fetches (PC must be even)
- Word/long data reads and writes to odd addresses
- Stack operations (SP must be even)

Byte accesses to odd addresses are fine -- they use the correct data
strobe (LDS for odd addresses).

On the Amiga, address errors typically cause a Guru Meditation
(system crash). Enforcer and similar tools can intercept these on
68020+ systems with MMU.

(Musashi m68kcpu.h, `m68ki_check_address_error()`: checks `(ADDR) & 1`)

---

## 12. TAS Instruction Ban

The TAS (Test And Set) instruction is **broken on the Amiga** when
accessing Chip RAM. This is one of the most important Amiga-specific
quirks for emulation.

### 12.1 What TAS Does

TAS reads a byte from memory, tests it (sets N and Z flags), and
then writes it back with bit 7 set -- all in a single indivisible
read-modify-write bus cycle. The AS (address strobe) remains asserted
throughout both the read and write phases.

```
TAS <ea>:
  1. Read byte from EA          (AS held low throughout)
  2. Test byte (set N, Z, clear V, C)
  3. Write byte | $80 back to EA (AS still held low)
```

### 12.2 Why It Fails on the Amiga

Agnus (the bus controller chip) expects the CPU to release AS between
bus cycles. The RMW cycle of TAS keeps AS asserted across two bus
phases, which Agnus cannot arbitrate correctly.

The result: Agnus does not recognise the write phase as a separate bus
cycle, and the write-back is **lost**. The byte reads correctly, flags
are set correctly, but the $80 bit is never written to memory.

This only affects Chip RAM accesses. TAS to Fast RAM works correctly
because Fast RAM access does not go through Agnus.

TAS to a data register (no memory access) works correctly on all
systems -- it is only the memory-referencing form that breaks.

### 12.3 Software Impact

The Genesis/Mega Drive has the same issue (different bus controller,
same RMW problem). Several games rely on TAS working incorrectly:
- Gargoyles
- Ex-Mutants

On the Amiga, most software avoids TAS entirely. The few programs
that use it for multiprocessor synchronisation (accelerator cards
with separate bus masters) must use Fast RAM.

### 12.4 Emulator Implementation

Musashi provides a callback mechanism to control TAS write-back:

```c
// (Musashi m68k_in.c, M68KMAKE_OP(tas, 8, ., .))
allow_writeback = m68ki_tas_callback();
if (allow_writeback == 1)
    m68ki_write_8(ea, dst | 0x80);
```

WinUAE also has configurable TAS behaviour. For correct Amiga
emulation:

- **TAS to Chip RAM:** Read-test-no-write (suppress write-back)
- **TAS to Fast RAM:** Read-test-write (normal behaviour)
- **TAS to data register:** Always works (no bus cycle)

---

## 13. VBR / MOVEC (68010+)

### 13.1 Vector Base Register

The 68010 introduces the VBR (Vector Base Register), a 32-bit
control register that specifies the base address of the exception
vector table.

On the 68000, the vector table is always at address $000000. On the
68010+, the vector table can be relocated by setting VBR to any
address.

```
Vector address = VBR + (vector_number * 4)
```

VBR is initialised to $00000000 on reset, so the default behaviour
matches the 68000.

### 13.2 MOVEC Instruction

MOVEC (Move Control Register) is the only way to access VBR and other
control registers. It is a privileged instruction (supervisor only).

```asm
MOVEC VBR,Dn    ; Read VBR into data register
MOVEC Dn,VBR    ; Write data register to VBR
```

Available control registers vary by CPU:

| Register | Code  | 68010 | 68020 | 68030 | 68040 | Description          |
|----------|-------|-------|-------|-------|-------|----------------------|
| SFC      | $000  | Yes   | Yes   | Yes   | Yes   | Source Function Code |
| DFC      | $001  | Yes   | Yes   | Yes   | Yes   | Dest Function Code   |
| VBR      | $801  | Yes   | Yes   | Yes   | Yes   | Vector Base Register |
| CACR     | $002  | No    | Yes   | Yes   | Yes   | Cache Control        |
| CAAR     | $802  | No    | Yes   | Yes   | No    | Cache Address        |
| USP      | $800  | No    | Yes   | Yes   | Yes   | User Stack Pointer   |
| MSP      | $803  | No    | Yes   | Yes   | Yes   | Master Stack Pointer |
| ISP      | $804  | No    | Yes   | Yes   | Yes   | Interrupt Stack Ptr  |

(Musashi m68kcpu.h: `REG_VBR`, `REG_CACR`, `REG_CAAR`)

### 13.3 Amiga Usage of VBR

AmigaOS uses VBR relocation on 68010+ systems to:

1. Move the vector table from Chip RAM (slow, contended) to Fast RAM
   (fast, no contention). This significantly improves interrupt
   response time.
2. Allow multitasking OSes to give each task its own vector table
   (though AmigaOS does not do this in practice).

The standard Kickstart 2.0+ startup code on 68010+ systems:
```asm
    ; Allocate memory for vector table
    MOVE.L  #1024,D0
    MOVEQ   #MEMF_PUBLIC,D1
    JSR     _LVOAllocMem(A6)
    ; Copy current vectors
    ; ...
    ; Set VBR
    MOVEC   D0,VBR
```

### 13.4 Emulator Implications

The VBR affects all exception vector lookups. Your exception handler
must compute:

```
handler_address = read_long(VBR + vector_number * 4)
```

Not:
```
handler_address = read_long(vector_number * 4)  // WRONG on 68010+
```

Musashi handles this in `m68ki_jump_vector()`:
```c
// (Musashi m68kcpu.h)
static inline void m68ki_jump_vector(uint vector) {
    REG_PC = (vector << 2) + REG_VBR;
    REG_PC = m68ki_read_data_32(REG_PC);
}
```

---

## 14. CACR / Cache Control (68020+)

### 14.1 68020 Cache Architecture

The 68020 has a 256-byte direct-mapped **instruction-only** cache
(no data cache). It is organised as 64 longword entries, each
containing two 16-bit instruction words.

Cache operation:
- **Fetch:** Instructions are fetched on longword boundaries (32 bits
  per bus cycle). Both words are written to the cache.
- **Hit:** If the tag matches and the valid bit is set, the instruction
  word is returned from cache (2 clocks) instead of from the bus
  (3+ clocks).
- **Miss:** Normal bus fetch, cache entry updated.
- **Write-through:** Data writes invalidate matching cache entries but
  are not cached.

Even with the cache disabled, the bus controller provides a form of
buffering: it holds the last fetched longword, so the second word of
a longword-aligned pair is available without a bus cycle (up to 50%
"hit rate" with cache off).

(M68000 Family Ref, MC68020 "On-Chip Instruction Cache")

### 14.2 CACR Register (68020)

The Cache Control Register on the 68020:

```
Bit 3: CE  - Cache Enable (1 = enabled)
Bit 2: FE  - Freeze Cache (1 = frozen, no replacement)
Bit 1: CLE - Clear Entry (write 1 to clear entry at CAAR)
Bit 0: C   - Clear Cache (write 1 to invalidate all entries)
```

### 14.3 68030 Cache Architecture

The 68030 adds a 256-byte data cache alongside the instruction cache.
Both are direct-mapped with 16 longword entries.

CACR on the 68030:

```
Bit 13: WA  - Write Allocate (data cache)
Bit 12: DBE - Data Burst Enable
Bit 11: CD  - Clear Data Cache
Bit 10: CED - Clear Data Cache Entry
Bit 9:  FD  - Freeze Data Cache
Bit 8:  ED  - Enable Data Cache
Bit 4:  IBE - Instruction Burst Enable
Bit 3:  CI  - Clear Instruction Cache
Bit 2:  CEI - Clear Instruction Cache Entry
Bit 1:  FI  - Freeze Instruction Cache
Bit 0:  EI  - Enable Instruction Cache
```

### 14.4 68040 Cache Architecture

The 68040 has separate 4KB instruction and data caches, both
4-way set-associative. The data cache supports **copyback** mode
(writes go to cache only, flushed to memory later) and
**write-through** mode.

CACR on the 68040 is simplified:
```
Bit 31: DE  - Enable Data Cache
Bit 15: IE  - Enable Instruction Cache
```

Individual page cache mode is controlled through the MMU page
descriptors (cacheable, write-through, copyback, or cache-inhibited).

### 14.5 Self-Modifying Code Issues

The instruction cache creates problems for self-modifying code,
which the Amiga uses extensively:

1. **Library patching (SetFunction):** AmigaOS's SetFunction() patches
   library jump vectors. After patching, the instruction cache must be
   invalidated or the old code will continue to execute from cache.

2. **Copper list generation:** The Copper executes from Chip RAM, not
   CPU cache, so this is not directly affected. But code that writes
   Copper lists and then expects to execute instructions from the same
   area could have issues.

3. **JIT / dynamic code generation:** Code that generates executable
   code at runtime must flush the instruction cache before jumping to
   the generated code.

4. **Overlay switching:** Some Amiga software switches ROM overlays
   at address $000000 (the initial vectors). The cache may hold stale
   entries from the previous overlay.

The 68040.library and 68060.library handle cache management for
AmigaOS. Direct hardware access code (demos, games) must manage
caches manually if running on 68020+ hardware.

### 14.6 Emulator Implications

For instruction-level accuracy:
- **68020:** Model the instruction cache. A cache hit saves one bus
  cycle (2 clocks vs 3 clocks for external access). Cache invalidation
  on writes to instruction space is critical.
- **68030:** Model both I-cache and D-cache. The data cache complicates
  DMA coherency -- custom chip DMA reads/writes bypass the CPU cache.
- **68040:** Copyback cache requires careful flush handling. The
  68040.library's cache flush routines must work correctly.

For most Amiga emulation (A500/A1200 era), the 68020 instruction
cache is the most important to get right.

---

## 15. Musashi Timing Tables

Musashi (version 4.60) by Karl Stenerud is a portable 68000 emulation
engine focused on instruction-level accuracy. It provides timing data
for five CPU types: 68000, 68010, 68020, 68030 (approximated), and
68040 (approximated).

### 15.1 Timing Table Structure

The timing table is in `m68k_in.c` at the `M68KMAKE_TABLE_BODY`
section (starting at line 306). Each entry contains:

```
name  size  spec_proc  spec_ea  bit_pattern  allowed_ea  mode  000 010 020
```

This table defines:
- The opcode handler function
- The mask and match for opcode decoding
- Base cycle counts for each CPU type

The table is compiled by the `m68kmake` tool into a 65536-entry jump
table (`m68ki_instruction_jump_table[0x10000]`) and a per-opcode
cycle table (`m68ki_cycles[NUM_CPU_TYPES][0x10000]`).

### 15.2 What Musashi Gets Right

- **Base instruction timing:** The table values are well-researched
  for the 68000 and 68010. They match the Motorola programmer's
  reference manual.
- **Data-dependent timing:** Shift/rotate (2n extra cycles), MULS
  (transition-dependent), MULU (bit-count dependent), MOVEM (register
  count), and DIVS/DIVU (variable) are all modelled.
- **Exception timing:** The exception cycle table provides accurate
  cycle counts for all 256 exception vectors on each CPU type.
- **EA calculation overhead:** Built into the per-opcode cycle counts.

### 15.3 What Musashi Lacks

- **No prefetch pipeline:** Musashi does not model the IR/IRC pipeline.
  `CPU_PREF_ADDR`/`CPU_PREF_DATA` provide a simple prefetch buffer but
  not the full two-stage pipeline that affects self-modifying code and
  timing.
- **No bus cycle simulation:** Musashi counts cycles but does not model
  individual bus cycles. It cannot interact with a cycle-exact bus
  arbiter (Agnus DMA contention).
- **No RMW cycle modelling:** TAS is handled via callback, but the bus
  cycle behaviour is not simulated.
- **68030/68040 timing approximate:** The table header notes "030 -
  not correct" and "040 - TODO: these values are not correct."
- **No cache simulation:** I-cache hits/misses are not modelled.

### 15.4 Exception Cycle Table Summary

From `m68ki_exception_cycle_table` in `m68kcpu.c`:

| Vector | Exception                     | 68000 | 68010 | 68020 |
|--------|-------------------------------|-------|-------|-------|
| 0      | Reset SSP                     | 40    | 40    | 4     |
| 1      | Reset PC                      | 4     | 4     | 4     |
| 2      | Bus Error                     | 50    | 126   | 50    |
| 3      | Address Error                 | 50    | 126   | 50    |
| 4      | Illegal Instruction           | 34    | 38    | 20    |
| 5      | Zero Divide                   | 38    | 44    | 38    |
| 6      | CHK                           | 40    | 44    | 40    |
| 7      | TRAPV                         | 34    | 34    | 20    |
| 8      | Privilege Violation           | 34    | 38    | 34    |
| 9      | Trace                         | 34    | 38    | 25    |
| 10     | Line 1010                     | 34    | 4     | 20    |
| 11     | Line 1111                     | 34    | 4     | 20    |
| 14     | Format Error                  | 4     | 4     | 4     |
| 15     | Uninitialized Interrupt       | 44    | 44    | 30    |
| 24     | Spurious Interrupt            | 44    | 46    | 30    |
| 25-31  | Autovector Interrupt (L1-L7)  | 44    | 46    | 30    |
| 32-47  | TRAP #0-#15                   | 34    | 38    | 20    |

### 15.5 Key Instruction Timings (68000)

Selected from the Musashi timing table (base cycles, register-to-register
or register-direct where applicable):

| Instruction      | 68000 | 68010 | 68020 | Notes                        |
|------------------|-------|-------|-------|------------------------------|
| MOVE.B Dn,Dn     | 4     | 4     | 2     |                              |
| MOVE.W Dn,Dn     | 4     | 4     | 2     |                              |
| MOVE.L Dn,Dn     | 4     | 4     | 2     |                              |
| MOVE.L (An),Dn   | 12    | 12    | 2     | +8 for long read EA          |
| MOVEQ #imm,Dn    | 4     | 4     | 2     |                              |
| ADD.L Dn,Dn      | 8     | 6     | 2     | Note: 68010 faster           |
| SUB.L Dn,Dn      | 8     | 6     | 2     |                              |
| CMP.L Dn,Dn      | 6     | 6     | 2     |                              |
| AND.L Dn,Dn      | 6     | 6     | 2     | Note: AND faster than ADD    |
| OR.L Dn,Dn       | 6     | 6     | 2     |                              |
| LSL #n,Dn (B/W)  | 6+2n  | 6+2n  | 4     | n=shift count                |
| LSL #n,Dn (L)    | 8+2n  | 8+2n  | 4     |                              |
| MULS Dn,Dn       | 38+2n | 32+2n | 27    | n=transitions in source      |
| MULU Dn,Dn       | 38+2n | 30+2n | 27    | n=set bits in source         |
| DIVS Dn,Dn       | 158   | 122   | 56    | Worst case                   |
| DIVU Dn,Dn       | 140   | 108   | 44    | Worst case                   |
| BRA.B             | 10    | 10    | 6     | Taken                        |
| Bcc.B (not taken) | 8     | 8     | 6     | Varies by condition          |
| DBcc (count)      | 12    | 12    | 6     | Loop not expired             |
| JSR xxx.L         | 20    | 20    | 0+EA  |                              |
| RTS               | 16    | 16    | 10    |                              |
| NOP               | 4     | 4     | 2     |                              |
| RESET             | 132   | 130   | 518   | External devices reset       |
| TAS (An)          | 14    | 14    | 12    | RMW cycle                    |
| MOVEM.L regs,(An) | 8+8n  | 8+8n  | 4+?   | n=register count             |
| MOVEM.L (An),regs | 12+8n | 12+8n | 8+?   |                              |
| TRAP #n           | 34    | 38    | 20    | +exception overhead          |
| RTE               | 20    | 24    | 20    |                              |
| STOP              | 4     | 4     | 8     | Waits for interrupt          |
| LEA (An),An       | 4     | 4     | 2     | No memory access             |
| PEA (An)          | 12    | 12    | 5     |                              |
| LINK A7,#d16      | 16    | 16    | 5     |                              |
| UNLK An           | 12    | 12    | 6     |                              |
| CLR.L Dn          | 6     | 6     | 2     |                              |
| CLR.L (An)        | 12    | 6     | 4     | 68010 optimised (no read)    |
| EXG Dn,Dn         | 6     | 6     | 2     |                              |
| SWAP Dn           | 4     | 4     | 4     |                              |

---

## 16. WinUAE Cycle-Exact Core

WinUAE by Toni Wilen is the most accurate Amiga emulator. Its CPU core
offers multiple accuracy levels, from simple instruction counting to
full cycle-exact bus simulation.

### 16.1 CPU Emulation Modes

WinUAE's CPU code is organised into numbered `cpuemu_XX.cpp` files,
each generated by the `gencpu` tool for a specific accuracy level:

| File          | Mode            | Description                         |
|---------------|-----------------|-------------------------------------|
| cpuemu_0.cpp  | Normal          | No prefetch, no cycle counting       |
| cpuemu_11.cpp | Compatible      | Prefetch pipeline, approximate timing|
| cpuemu_13.cpp | Cycle-Exact CE  | Full CE for 68000/68010              |
| cpuemu_20.cpp | Normal 020+     | No prefetch for 68020+               |
| cpuemu_21.cpp | CE 020+         | Cycle-exact for 68020                |
| cpuemu_22.cpp | Prefetch 020+   | Prefetch pipeline for 68020          |
| cpuemu_23.cpp | CE 030          | Cycle-exact for 68030                |
| cpuemu_24.cpp | CE 030 MMU      | CE 030 with MMU                      |
| cpuemu_31.cpp | CE 040          | Cycle-exact for 68040                |
| cpuemu_32.cpp | CE 040 MMU      | CE 040 with MMU                      |
| cpuemu_33.cpp | Prefetch 040    | Prefetch pipeline for 68040          |
| cpuemu_34.cpp | CE 060          | Cycle-exact for 68060                |
| cpuemu_35.cpp | CE 060          | Alternate 68060 mode                 |
| cpuemu_40.cpp | JIT             | Just-In-Time compilation             |
| cpuemu_50.cpp | JIT             | Alternate JIT mode                   |

### 16.2 The CE (Cycle-Exact) Model

The CE model (cpuemu_13.cpp for 68000/010) is the most accurate. Every
generated instruction handler:

1. **Prefetches** via `get_word_ce000_prefetch()`, which performs a
   real bus cycle taking 4+ clocks and checking DMA contention.
2. **Reads/writes** via `x_get_byte/word/long()` and
   `x_put_byte/word/long()`, each performing bus cycles.
3. **Advances cycles** via `x_do_cycles()` for internal processing.
4. **Samples IPL** via `ipl_fetch_now()` / `ipl_fetch_next()` at the
   correct points during instruction execution.
5. **Handles bus errors** via `hardware_bus_error` checks after every
   bus access.

Example from cpuemu_13.cpp (OR.B #imm,Dn):

```c
// (WinUAE cpuemu_13.cpp)
void op_0000_13_ff(uae_u32 opcode) {
    uae_u32 dstreg = opcode & 7;
    uae_s8 src = (uae_u8)get_word_ce000_prefetch(4);  // Fetch immediate
    if (hardware_bus_error) {
        exception2_fetch(opcode, 4, 0);
        return;
    }
    uae_s8 dst = m68k_dreg(regs, dstreg);
    src |= dst;
    CLEAR_CZNV();
    SET_ZFLG(((uae_s8)(src)) == 0);
    SET_NFLG(((uae_s8)(src)) < 0);
    m68k_dreg(regs, dstreg) = /* ... */;
    regs.ir = regs.irc;                  // Pipeline advance
    opcode = regs.ir;
    ipl_fetch_next();                     // Sample IPL
    get_word_ce000_prefetch(6);           // Prefetch next word
    if (hardware_bus_error) {
        exception2_fetch_opcode(opcode, 6, 0);
        return;
    }
    m68k_incpci(4);
}
/* 8 (2/0) */
```

Key observations:
- `regs.ir = regs.irc;` -- This models the prefetch pipeline advancement.
  IR gets the previously prefetched word; a new prefetch fills IRC.
- `ipl_fetch_next()` -- IPL is sampled at specific points, not every
  clock cycle. This matters for interrupt timing accuracy.
- `hardware_bus_error` checks after every bus access -- bus errors can
  occur on any memory access, and the handler must be called with the
  correct state.

### 16.3 The Prefetch Pipeline in WinUAE

WinUAE models the 68000 prefetch as two registers:

- `regs.ir` -- Instruction Register: holds the currently executing opcode
- `regs.irc` -- Instruction Register Capture: holds the prefetched word

Additional state:
- `regs.read_buffer` -- Last data read from the bus (used in 68010
  bus error frame)
- `regs.write_buffer` -- Last data written to the bus
- `regs.db` -- Data bus buffer (for 68020 prefetch)

The prefetch function `get_word_ce000_prefetch(offset)` reads from
`(PC + offset)` and stores into `regs.irc`. The instruction execution
code then does `regs.ir = regs.irc` at the appropriate point.

### 16.4 Function Pointer Tables

WinUAE uses function pointer tables for all bus accesses, allowing
runtime switching between accuracy modes:

```c
// (WinUAE newcpu.cpp)
uae_u32 (*x_prefetch)(int);        // Instruction prefetch
uae_u32 (*x_get_iword)(int);       // Read instruction word
void (*x_put_long)(uaecptr, uae_u32);  // Write longword
uae_u32 (*x_get_long)(uaecptr);    // Read longword
void (*x_do_cycles)(int);          // Advance clock cycles
void (*x_do_cycles_pre)(int);      // Pre-access cycles
void (*x_do_cycles_post)(int, uae_u32); // Post-access cycles
```

For CE mode (68000):
```c
x_prefetch = get_word_ce000_prefetch;
x_put_long = put_long_ce000;
x_get_long = get_long_ce000;
x_do_cycles = do_cycles_ce;
```

For compatible mode (68000):
```c
x_prefetch = get_word_000_prefetch;
x_put_long = put_long_compatible;
x_get_long = get_long_compatible;
x_do_cycles = do_cycles;
```

This design allows the same generated instruction handlers to work
with different bus models by simply swapping the function pointers.

### 16.5 Exception Handling in CE Mode

WinUAE's `Exception_ce000()` (newcpu.cpp, line 2748) handles all
exceptions in CE mode:

- **Interrupt timing:** 6 cycles of internal processing before SR save.
- **Bus/address error:** Allocates the full frame (7 words for 68000,
  29 words for 68010), writes each word as a separate bus cycle.
- **Double bus fault:** If the stack pointer is odd or another exception
  occurs during frame stacking, the CPU halts.
- **IACK cycle:** For interrupts, `iack_cycle(nr)` performs the interrupt
  acknowledge bus cycle with appropriate E-clock synchronisation for
  autovectored interrupts.

### 16.6 Accuracy Assessment

WinUAE's CE mode is the most accurate 68000 emulation available:

- **Prefetch pipeline:** Fully modelled with IR/IRC
- **Bus cycle timing:** Every access goes through the bus with DMA
  contention
- **Exception timing:** Matches real hardware to the cycle
- **IPL sampling:** Modelled at correct instruction boundaries
- **Bus error recovery:** Full 68010 frame with SSW, buffers, etc.
- **Self-modifying code:** Handled correctly via prefetch pipeline

Known limitations:
- Some undocumented 68000 behaviour may not be perfectly replicated
- 68020+ CE modes are less thoroughly tested than 68000 CE
- Cache simulation for 68020+ is present but may have edge cases

---

## 17. Implementation Checklist

This checklist summarises the critical items an emulator must implement
for accurate 68000 emulation on the Amiga. Items are roughly ordered
by importance for compatibility.

### Priority 1: Must Have

1. **Prefetch pipeline (IR/IRC).** Model the two-word prefetch. Many
   copy protection schemes, demos, and timing-sensitive code depend on
   this. Without it, self-modifying code and JMP-based tricks will fail.

2. **Correct instruction timing.** Use cycle counts from the Motorola
   data sheet or a verified source (Musashi timing table). Include
   data-dependent timing for MULS/MULU/DIVS/DIVU/shifts/MOVEM.

3. **Exception stack frames.** Implement the correct frame format for
   each CPU model (3-word for 68000, 4-word format $0 for 68010+,
   14-byte bus error frame for 68000, 58-byte format $8 for 68010).

4. **Interrupt priority masking.** The CPU only services interrupts with
   priority > current mask. Level 7 is non-maskable.

5. **Address error on odd word/long access.** Trap on word/long accesses
   to odd addresses (68000/68010). Do NOT trap on byte accesses.

6. **MOVE from SR privilege change (68010+).** This must trap in user
   mode on 68010+ but work normally on 68000.

7. **TAS write-back suppression on Chip RAM.** The write-back phase of
   TAS fails through Agnus. Suppress writes to Chip RAM addresses.

### Priority 2: Important for Accuracy

8. **Bus cycle-level DMA contention.** Model Chip RAM access delays
   caused by DMA activity. This affects timing of all code running
   from Chip RAM.

9. **VBR (68010+).** Exception vector lookups must add VBR offset.
   Relocating the vector table to Fast RAM is a common optimisation.

10. **Autovector interrupt acknowledge.** Model the IACK bus cycle and
    E-clock synchronisation for VPA-based autovectors.

11. **Exception processing timing.** Each exception type has a specific
    cycle cost. Use the values from the Motorola data sheet or Musashi's
    exception cycle table.

12. **RESET instruction timing.** 124 clocks with RESET pin asserted,
    plus instruction overhead. Must not affect CPU internal state.

### Priority 3: For Full Accuracy

13. **IPL sampling points.** Sample interrupt priority level at the
    correct point during instruction execution (not every clock, not
    just at instruction boundaries).

14. **68010 bus error recovery.** The full format $8 frame with SSW,
    fault address, data buffers, and internal state must be correct for
    virtual memory and programs that use the 68010's restart capability.

15. **Cache simulation (68020+).** Model the instruction cache for
    correct timing and self-modifying code behaviour. Handle CACR
    writes (enable, clear, freeze).

See [Appendix D](#appendix-d-implementation-checklist-detail) for
expanded detail on each item.

---

## Appendix A: Exception Vector Table (Complete)

All 256 exception vectors. Addresses shown for VBR = $000000 (default).

### Vectors 0-15: Defined by CPU

| Vec# | Address | Type    | Exception                          | SP   |
|------|---------|---------|------------------------------------|------|
| 0    | $000    | Reset   | Initial Supervisor Stack Pointer   | SSP  |
| 1    | $004    | Reset   | Initial Program Counter            | SSP  |
| 2    | $008    | Grp 0   | Bus Error                          | SSP  |
| 3    | $00C    | Grp 0   | Address Error                      | SSP  |
| 4    | $010    | Grp 1   | Illegal Instruction                | SSP  |
| 5    | $014    | Grp 2   | Zero Divide                        | SSP  |
| 6    | $018    | Grp 2   | CHK / CHK2 Instruction             | SSP  |
| 7    | $01C    | Grp 2   | TRAPV / TRAPcc / cpTRAPcc          | SSP  |
| 8    | $020    | Grp 1   | Privilege Violation                | SSP  |
| 9    | $024    | Grp 1   | Trace                              | SSP  |
| 10   | $028    | Grp 1   | Line 1010 Emulator                 | SSP  |
| 11   | $02C    | Grp 1   | Line 1111 Emulator                 | SSP  |
| 12   | $030    | --      | (Reserved by Motorola)             | --   |
| 13   | $034    | Grp 1   | Coprocessor Protocol Violation     | SSP  |
| 14   | $038    | Grp 1   | Format Error (68010+)              | SSP  |
| 15   | $03C    | Grp 1   | Uninitialised Interrupt            | SSP  |

### Vectors 16-23: Reserved

| Vec# | Address | Type    | Exception                          |
|------|---------|---------|------------------------------------|
| 16   | $040    | --      | Reserved                           |
| 17   | $044    | --      | Reserved                           |
| 18   | $048    | --      | Reserved                           |
| 19   | $04C    | --      | Reserved                           |
| 20   | $050    | --      | Reserved                           |
| 21   | $054    | --      | Reserved                           |
| 22   | $058    | --      | Reserved                           |
| 23   | $05C    | --      | Reserved                           |

### Vectors 24-31: Interrupts

| Vec# | Address | Type    | Exception                          | Amiga Use       |
|------|---------|---------|------------------------------------|-----------------|
| 24   | $060    | Grp 1   | Spurious Interrupt                 | Hardware error  |
| 25   | $064    | Grp 1   | Level 1 Interrupt Autovector       | TBE/DSKBLK/SOFT|
| 26   | $068    | Grp 1   | Level 2 Interrupt Autovector       | CIA-A / PORTS   |
| 27   | $06C    | Grp 1   | Level 3 Interrupt Autovector       | VBL/Copper/Blit |
| 28   | $070    | Grp 1   | Level 4 Interrupt Autovector       | Audio           |
| 29   | $074    | Grp 1   | Level 5 Interrupt Autovector       | RBF/DSKSYNC     |
| 30   | $078    | Grp 1   | Level 6 Interrupt Autovector       | CIA-B / EXTER   |
| 31   | $07C    | Grp 1   | Level 7 Interrupt Autovector       | NMI             |

### Vectors 32-47: TRAP Instructions

| Vec# | Address | Type    | Exception                          |
|------|---------|---------|------------------------------------|
| 32   | $080    | Grp 2   | TRAP #0                            |
| 33   | $084    | Grp 2   | TRAP #1                            |
| 34   | $088    | Grp 2   | TRAP #2                            |
| 35   | $08C    | Grp 2   | TRAP #3                            |
| 36   | $090    | Grp 2   | TRAP #4                            |
| 37   | $094    | Grp 2   | TRAP #5                            |
| 38   | $098    | Grp 2   | TRAP #6                            |
| 39   | $09C    | Grp 2   | TRAP #7                            |
| 40   | $0A0    | Grp 2   | TRAP #8                            |
| 41   | $0A4    | Grp 2   | TRAP #9                            |
| 42   | $0A8    | Grp 2   | TRAP #10                           |
| 43   | $0AC    | Grp 2   | TRAP #11                           |
| 44   | $0B0    | Grp 2   | TRAP #12                           |
| 45   | $0B4    | Grp 2   | TRAP #13                           |
| 46   | $0B8    | Grp 2   | TRAP #14                           |
| 47   | $0BC    | Grp 2   | TRAP #15                           |

### Vectors 48-63: FP / MMU Exceptions (68020+)

| Vec# | Address | Type    | Exception                          |
|------|---------|---------|------------------------------------|
| 48   | $0C0    | Grp 2   | FP Branch/Set on Unordered Cond    |
| 49   | $0C4    | Grp 2   | FP Inexact Result                  |
| 50   | $0C8    | Grp 2   | FP Divide by Zero                  |
| 51   | $0CC    | Grp 2   | FP Underflow                       |
| 52   | $0D0    | Grp 2   | FP Operand Error                   |
| 53   | $0D4    | Grp 2   | FP Overflow                        |
| 54   | $0D8    | Grp 2   | FP Signalling NAN                  |
| 55   | $0DC    | Grp 2   | FP Unimplemented Data Type         |
| 56   | $0E0    | --      | MMU Configuration Error            |
| 57   | $0E4    | --      | MMU Illegal Operation              |
| 58   | $0E8    | --      | MMU Access Level Violation         |
| 59   | $0EC    | --      | Reserved                           |
| 60   | $0F0    | --      | Reserved                           |
| 61   | $0F4    | --      | Reserved                           |
| 62   | $0F8    | --      | Reserved                           |
| 63   | $0FC    | --      | Reserved                           |

### Vectors 64-255: User-Defined

Addresses $100-$3FC. Available for use by peripheral devices that
provide their own vector number during IACK cycles. On the Amiga,
Zorro expansion cards may use these.

---

## Appendix B: Instruction Timing Quick-Reference

Extracted from Musashi m68k_in.c `M68KMAKE_TABLE_BODY`. Shows base
clock cycles for common instructions across CPU types. EA overhead
is included where the table specifies an EA mode.

Format: `instruction size EA -- 000 010 020`

### Data Movement

```
MOVE.B  Dn,Dn          --   4   4   2
MOVE.W  Dn,Dn          --   4   4   2
MOVE.L  Dn,Dn          --   4   4   2
MOVE.B  (An),Dn        --   8   8   4     (includes EA time)
MOVE.W  (An),Dn        --   8   8   4
MOVE.L  (An),Dn        --  12  12   4
MOVE.B  Dn,(An)        --   8   8   4
MOVE.W  Dn,(An)        --   8   8   4
MOVE.L  Dn,(An)        --  12  12   4
MOVE.L  Dn,d16(An)     --  16  16   5
MOVE.L  Dn,xxx.L       --  20  20   6
MOVEA.W <ea>,An        --   4   4   2     (register direct)
MOVEA.L <ea>,An        --   4   4   2     (register direct)
MOVEQ   #imm,Dn        --   4   4   2
MOVEM.W regs,-(An)     --   8   8   4     +4 per register (68000)
MOVEM.L regs,-(An)     --   8   8   4     +8 per register (68000)
MOVEM.W (An)+,regs     --  12  12   8     +4 per register (68000)
MOVEM.L (An)+,regs     --  12  12   8     +8 per register (68000)
MOVEP.W Dn,d16(An)     --  16  16  11
MOVEP.L Dn,d16(An)     --  24  24  17
LEA     (An),An        --   4   4   2     (varies by EA mode)
PEA     (An)           --  12  12   5     (varies by EA mode)
EXG     Dn,Dn          --   6   6   2
SWAP    Dn             --   4   4   4
```

### Arithmetic

```
ADD.B   Dn,Dn          --   4   4   2
ADD.W   Dn,Dn          --   4   4   2
ADD.L   Dn,Dn          --   8   6   2     (68010 faster for .L!)
ADDA.W  <ea>,An        --   8   8   2
ADDA.L  <ea>,An        --   6   6   2
ADDI.B  #imm,Dn        --   8   8   2
ADDI.L  #imm,Dn        --  16  14   2
ADDQ.L  #imm,Dn        --   8   8   2
SUB.L   Dn,Dn          --   8   6   2
MULS    Dn,Dn          --  38+ 32+ 27    (+2n, data dependent)
MULU    Dn,Dn          --  38+ 30+ 27    (+2n, data dependent)
DIVS    <ea>,Dn        -- 158 122  56    (worst case)
DIVU    <ea>,Dn        -- 140 108  44    (worst case)
NEG.L   Dn             --   6   6   2
CLR.L   Dn             --   6   6   2
CLR.L   (An)           --  12   6   4    (68010: no dummy read)
CMP.L   Dn,Dn          --   6   6   2
```

### Logic

```
AND.L   Dn,Dn          --   6   6   2
OR.L    Dn,Dn          --   6   6   2
EOR.L   Dn,Dn          --   8   6   2
NOT.L   Dn             --   6   6   2
LSL     #n,Dn (B/W)    --   6   6   4    +2n per shift (68000/010)
LSL     #n,Dn (L)      --   8   8   4    +2n per shift (68000/010)
ROL     #n,Dn (B/W)    --   6   6   8
ASL     #n,Dn (B/W)    --   6   6   8
```

### Program Control

```
BRA.B                   --  10  10  10
BRA.W                   --  10  10  10
Bcc.B   (taken)         --  10  10   6
Bcc.B   (not taken)     --   8   8   4
BSR.B                   --  18  18   7
DBcc    (count--)       --  12  12   6    (loop not expired)
DBcc    (expired)       --  14  14  10    (loop done)
JMP     (An)            --   8   8   0+EA
JSR     (An)            --  16  16   0+EA
RTS                     --  16  16  10
RTE                     --  20  24  20
RTR                     --  20  20  14
LINK    An,#d16         --  16  16   5
UNLK    An              --  12  12   6
NOP                     --   4   4   2
STOP    #imm            --   4   4   8
TRAP    #n              --   4   4   4    (+exception overhead)
TRAPV                   --   4   4   4
```

### Bit Manipulation

```
BTST    Dn,Dn          --   6   6   4
BSET    Dn,Dn          --   8   8   4
BCLR    Dn,Dn          --  10  10   4
BCHG    Dn,Dn          --   8   8   4
BTST    #imm,Dn        --  10  10   4
```

### BCD

```
ABCD    Dn,Dn          --   6   6   4
SBCD    Dn,Dn          --   6   6   4
NBCD    Dn             --   6   6   6
```

### System Control

```
RESET                   --   0   0   0    (+CYC_RESET internally)
TAS     Dn             --   4   4   4
TAS     (An)           --  14  14  12    (RMW cycle)
MOVE    SR,Dn          --   6   4   8    (user on 000, priv on 010+)
MOVE    Dn,SR          --  12  12   8    (privileged)
MOVE    CCR,Dn         --   .   4   4    (68010+ only)
MOVE    Dn,CCR         --  12  12   4
MOVEC   Rc,Rn          --   .  12   6    (68010+ only)
MOVEC   Rn,Rc          --   .  10  12    (68010+ only)
```

---

## Appendix C: Bus Cycle Diagrams

### C.1 Normal Read Cycle (4 clocks, no wait states)

```
Clock:    |  1  |  2  |  3  |  4  |
State:    | S0  | S1  | S2  | S3  | S4  | S5  | S6  | S7  |
          |_____|_____|_____|_____|_____|_____|_____|_____|

CLK:      _/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\

ADDR:     ----<======================================>----
           A1-A23 valid

FC:       ----<======================================>----
           FC0-FC2 valid

AS:       --------\________________________________/------
                    |  asserted                  |

UDS/LDS:  --------\________________________________/------
                    |  asserted                  |

R/W:      ====================================================
           held HIGH for read

DTACK:    --------------------------------\________/------
                                           |      |
                                     slave asserts

DATA:     --------------------------------<=======>-------
           D0-D15                          valid from slave
```

### C.2 Normal Write Cycle (4 clocks, no wait states)

```
Clock:    |  1  |  2  |  3  |  4  |
State:    | S0  | S1  | S2  | S3  | S4  | S5  | S6  | S7  |

CLK:      _/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\

ADDR:     ----<======================================>----

FC:       ----<======================================>----

AS:       --------\________________________________/------

UDS/LDS:  ----------------\________________________/------
                           | asserted later than read

R/W:      --------\________________________________/------
                    | goes LOW for write

DTACK:    --------------------------------\________/------

DATA:     ----------------<========================>------
           D0-D15          CPU drives data from S3
```

### C.3 Read-Modify-Write Cycle (TAS)

```
          |<---------- Read Phase ---------->|<--------- Write Phase --------->|
State:    | S0  S1  S2  S3  S4  S5  S6  S7  | S0  S1  S2  S3  S4  S5  S6  S7 |

AS:       ------\______________________________________________________________/--
                 |                    STAYS ASSERTED THROUGHOUT                 |

R/W:      ==============================================\______________________/--
          HIGH (read)                                    LOW (write)

DATA:     --------------------------<=====>---------<========================>--
          (in from slave)                            (out from CPU, bit 7 set)
```

Note: AS stays asserted across both phases. This is what prevents Agnus
from arbitrating the write phase correctly on the Amiga.

### C.4 Read Cycle with Wait States

```
State:    | S0  S1  S2  S3  S4  W   W   W   W   S5  S6  S7  |

CLK:      _/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\_/--\

AS:       --------\________________________________________________/------

DTACK:    --------------------------------------------------\______/------
                                                             |
                                              finally asserted after wait

DATA:     --------------------------------------------------<=====>-------
```

Each pair of wait states (W, W) adds 2 CPU clocks = 1 colour clock.

### C.5 Interrupt Acknowledge Cycle

```
State:    | S0  S1  S2  S3  S4  (wait for E-clock sync)  S5  S6  S7  |

FC:       ----< 1 1 1 >--------------------------------------------
           CPU space ($7)

ADDR:     ----< A4-A23=1, A1-A3=IPL level, A0=1 >-------------------

AS:       --------\________________________________________________/

R/W:      ================================================================
           HIGH (read)

VPA:      --------------------------------\___________________________/
           Asserted by Gary/Gayle -> autovector

DATA:     ----------------------------------------<=====>---------
           Not used for autovector (CPU uses internal vector)
```

For autovectored interrupts, the IACK cycle synchronises with the
E-clock (a free-running output at CLK/10). This can add up to 9
extra clocks of latency.

---

## Appendix D: Implementation Checklist (Detail)

Expanded checklist with implementation notes for each item.

### D.1 Prefetch Pipeline

**What:** Model two 16-bit registers: IR (current opcode) and IRC
(next prefetched word).

**Why:** Self-modifying code, copy protection, JMP tricks, and
instruction timing all depend on the prefetch being one word ahead.

**How:**
- At instruction start, IR holds the opcode being decoded.
- During execution, prefetch the next word into IRC.
- At instruction end, move IRC -> IR and start a new prefetch.
- For branch/jump: prefetch from the target address, not the
  sequential address.

**Test:** Write to the address of the next instruction. On a real
68000, the old instruction executes (it was already in IRC).

### D.2 Instruction Timing

**What:** Track cycle counts for every instruction, including
data-dependent variations.

**Why:** Scanline-based effects, raster timing, and many demos depend
on precise cycle counts.

**How:**
- Use the Musashi timing table as a baseline.
- Add data-dependent cycles for MULS (transitions), MULU (set bits),
  shifts (2 per position), MOVEM (register count).
- DIVS/DIVU have complex timing -- use worst-case or implement the
  full algorithm.

### D.3 Exception Stack Frames

**What:** Push the correct number of words in the correct format for
each exception type and CPU model.

**Why:** RTE reads the frame back. Wrong frame size = stack corruption
= crash.

**How:**
- 68000: 3-word frame for normal, 7-word frame for bus/address error.
- 68010: 4-word format $0 for normal, 29-word format $8 for bus error.
- 68020: Format $0, $1, $2, $A, $B depending on exception type.

### D.4 Interrupt Priority Masking

**What:** Compare IPL level against SR interrupt mask (bits 10-8).

**Why:** Only higher-priority interrupts are serviced. Level 7 is
always serviced.

**How:**
```
if (ipl_level > (SR >> 8) & 7) || (ipl_level == 7):
    service_interrupt(ipl_level)
```

### D.5 Address Error Detection

**What:** Check alignment on word and longword accesses.

**Why:** Real hardware generates an address error exception.

**How:**
- Check `address & 1` for all word/long reads, writes, and instruction
  fetches.
- Do NOT check byte accesses.
- Do NOT check on 68020+ (they handle misalignment in hardware).

### D.6 MOVE from SR Privilege

**What:** 68000 allows MOVE from SR in user mode; 68010+ traps it.

**Why:** KS 1.x and many games use MOVE from SR in user mode. This
works on 68000 but traps on 68010+.

**How:**
- If cpu_model == 68000: allow in user mode.
- If cpu_model >= 68010: trap privilege violation in user mode.
  Use MOVE from CCR instead.

### D.7 TAS on Chip RAM

**What:** Suppress the write-back phase of TAS when accessing Chip RAM.

**Why:** Agnus cannot handle RMW bus cycles.

**How:**
- Check if the target address is in Chip RAM range ($000000-$1FFFFF
  typically).
- If so, perform the read and flag-set but skip the write.
- If Fast RAM or register: perform normally.

### D.8 DMA Contention

**What:** Model bus arbitration between CPU and DMA channels.

**Why:** Code running from Chip RAM is slowed by DMA activity. Many
programs depend on this for timing.

**How:**
- Maintain a DMA slot allocation table per scanline.
- When the CPU needs a bus cycle to Chip RAM, check if the current
  DMA slot is free.
- If occupied, delay the CPU by 2 clocks per slot.

### D.9 VBR Relocation

**What:** Add VBR offset to all exception vector lookups on 68010+.

**Why:** AmigaOS relocates the vector table to Fast RAM for performance.

**How:**
```
vector_address = VBR + (vector_number * 4)
handler_pc = read_long(vector_address)
```

### D.10 Autovector IACK

**What:** Model the interrupt acknowledge cycle with E-clock sync.

**Why:** Autovectored interrupts on the Amiga synchronise with the
E-clock, adding variable latency (0-9 extra clocks).

**How:**
- During IACK, wait for the next E-clock falling edge.
- The E-clock runs at CLK/10, free-running.
- After sync, use the autovector for the interrupt level.

### D.11 Exception Processing Timing

**What:** Add the correct number of cycles for exception processing.

**Why:** Interrupt latency affects timing-sensitive code.

**How:** Use the cycle counts from Musashi's exception cycle table
(Section 15.4) as the processing overhead, added to the bus cycles
for stack writes and vector fetch.

### D.12 RESET Instruction

**What:** Assert RESET for 124 clocks without affecting CPU state.

**Why:** Software-initiated peripheral reset must not disturb the CPU.

**How:**
- Add 124+ clock cycles to the instruction cost.
- Signal all peripherals (custom chips, CIAs) to reset.
- Do NOT modify any CPU registers, SR, or PC.

### D.13 IPL Sampling

**What:** Sample the interrupt priority level at specific points during
instruction execution, not continuously.

**Why:** The 68000 has a ~2 cycle delay in recognising IPL changes.
Some software depends on this for precise interrupt timing.

**How:**
- Sample IPL once during each instruction, at the point where the
  prefetch for the next instruction occurs.
- Store the sampled value and compare it against the SR mask at the
  instruction boundary.

### D.14 Bus Error Recovery (68010)

**What:** Build a correct format $8 frame with all internal state.

**Why:** Programs that use virtual memory on the 68010 depend on
complete state restoration via RTE.

**How:**
- Save the instruction input buffer (IRC value).
- Save the data input/output buffers.
- Save the fault address.
- Build the SSW with R/W, DF, BY, HB, FC fields.
- Save 16 words of internal state (version-dependent).

### D.15 Cache Control (68020+)

**What:** Model the instruction cache and CACR register.

**Why:** Self-modifying code, library patching, and cache-aware
software depend on correct cache behaviour.

**How:**
- Implement a 256-byte direct-mapped I-cache (68020).
- Check for hits on instruction fetch.
- Invalidate on writes to cached addresses.
- Handle CACR writes: enable (CE), clear (C), freeze (FE).
- On the 68030, add a D-cache with similar logic.

---

## Appendix E: Further Reading

- Motorola M68000 Family Programmer's Reference Manual (M68000PRM/AD,
  1992) -- The definitive instruction set reference.
- Motorola MC68000 User's Manual (MC68000UM/AD) -- Detailed bus timing
  and electrical specifications.
- Amiga Hardware Reference Manual (Addison-Wesley) -- Custom chip
  registers and DMA timing.
- WinUAE source code (github.com/tonioni/WinUAE) -- The gold standard
  for cycle-exact Amiga emulation.
- Musashi source code -- Clean, well-documented 68000 core with
  accurate timing tables.
- yacht.txt (Yet Another 68000 Cycle Timing document) -- Community
  reference for exact cycle counts per bus access.

---

## Appendix F: Complete Musashi Instruction Timing Table

The complete timing table from Musashi m68k_in.c, `M68KMAKE_TABLE_BODY`.
This is the authoritative per-opcode cycle count used by the Musashi
emulator. The "000", "010", and "020" columns show base clock cycles
for the 68000, 68010, and 68020 respectively.

Columns: `name  size  spec_proc  spec_ea  -- 000 010 020`

Where EA cycles are already included in the base count for each
specific EA encoding. Data-dependent additions (shifts, multiply,
MOVEM register count) are applied at runtime by the instruction
handler.

### F.1 Arithmetic and Logic

```
abcd       8  rr    .           --   6   6   4
abcd       8  mm    .           --  18  18  16
add        8  er    d           --   4   4   2
add        8  er    <ea>        --   4   4   2    +EA overhead
add       16  er    d           --   4   4   2
add       16  er    a           --   4   4   2
add       16  er    <ea>        --   4   4   2    +EA overhead
add       32  er    d           --   6   6   2
add       32  er    a           --   6   6   2
add       32  er    <ea>        --   6   6   2    +EA overhead
add        8  re    <ea>        --   8   8   4    +EA overhead
add       16  re    <ea>        --   8   8   4    +EA overhead
add       32  re    <ea>        --  12  12   4    +EA overhead
adda      16  .     d           --   8   8   2
adda      16  .     a           --   8   8   2
adda      16  .     <ea>        --   8   8   2    +EA overhead
adda      32  .     d           --   6   6   2
adda      32  .     a           --   6   6   2
adda      32  .     <ea>        --   6   6   2    +EA overhead
addi       8  .     d           --   8   8   2
addi       8  .     <ea>        --  12  12   4    +EA overhead
addi      16  .     d           --   8   8   2
addi      16  .     <ea>        --  12  12   4    +EA overhead
addi      32  .     d           --  16  14   2
addi      32  .     <ea>        --  20  20   4    +EA overhead
addq       8  .     d           --   4   4   2
addq       8  .     <ea>        --   8   8   4    +EA overhead
addq      16  .     d           --   4   4   2
addq      16  .     a           --   4   4   2    (address register)
addq      16  .     <ea>        --   8   8   4    +EA overhead
addq      32  .     d           --   8   8   2
addq      32  .     a           --   8   8   2
addq      32  .     <ea>        --  12  12   4    +EA overhead
addx       8  rr    .           --   4   4   2
addx      16  rr    .           --   4   4   2
addx      32  rr    .           --   8   6   2
addx       8  mm    .           --  18  18  12
addx      16  mm    .           --  18  18  12
addx      32  mm    .           --  30  30  12
and        8  er    d           --   4   4   2
and        8  er    <ea>        --   4   4   2    +EA overhead
and       16  er    d           --   4   4   2
and       16  er    <ea>        --   4   4   2    +EA overhead
and       32  er    d           --   6   6   2
and       32  er    <ea>        --   6   6   2    +EA overhead
and        8  re    <ea>        --   8   8   4    +EA overhead
and       16  re    <ea>        --   8   8   4    +EA overhead
and       32  re    <ea>        --  12  12   4    +EA overhead
andi      16  toc   .           --  20  16  12    (to CCR)
andi      16  tos   .           --  20  16  12    (to SR, priv)
andi       8  .     d           --   8   8   2
andi       8  .     <ea>        --  12  12   4    +EA overhead
andi      16  .     d           --   8   8   2
andi      16  .     <ea>        --  12  12   4    +EA overhead
andi      32  .     d           --  14  14   2
andi      32  .     <ea>        --  20  20   4    +EA overhead
cmp        8  .     d           --   4   4   2
cmp        8  .     <ea>        --   4   4   2    +EA overhead
cmp       16  .     d           --   4   4   2
cmp       16  .     a           --   4   4   2
cmp       16  .     <ea>        --   4   4   2    +EA overhead
cmp       32  .     d           --   6   6   2
cmp       32  .     a           --   6   6   2
cmp       32  .     <ea>        --   6   6   2    +EA overhead
cmpa      16  .     <any>       --   6   6   4
cmpa      32  .     <any>       --   6   6   4
cmpi       8  .     d           --   8   8   2
cmpi       8  .     <ea>        --   8   8   2    +EA overhead
cmpi      16  .     d           --   8   8   2
cmpi      16  .     <ea>        --   8   8   2    +EA overhead
cmpi      32  .     d           --  14  12   2
cmpi      32  .     <ea>        --  12  12   2    +EA overhead
cmpm       8  .     .           --  12  12   9
cmpm      16  .     .           --  12  12   9
cmpm      32  .     .           --  20  20   9
divs      16  .     d           -- 158 122  56    (worst case)
divs      16  .     <ea>        -- 158 122  56    +EA overhead
divu      16  .     d           -- 140 108  44    (worst case)
divu      16  .     <ea>        -- 140 108  44    +EA overhead
eor        8  .     d           --   4   4   2
eor        8  .     <ea>        --   8   8   4    +EA overhead
eor       16  .     d           --   4   4   2
eor       16  .     <ea>        --   8   8   4    +EA overhead
eor       32  .     d           --   8   6   2
eor       32  .     <ea>        --  12  12   4    +EA overhead
eori      16  toc   .           --  20  16  12
eori      16  tos   .           --  20  16  12    (priv)
eori       8  .     d           --   8   8   2
eori       8  .     <ea>        --  12  12   4    +EA overhead
eori      16  .     d           --   8   8   2
eori      16  .     <ea>        --  12  12   4    +EA overhead
eori      32  .     d           --  16  14   2
eori      32  .     <ea>        --  20  20   4    +EA overhead
muls      16  .     d           --  38  32  27    (+2n transitions)
muls      16  .     <ea>        --  38  32  27    +EA
mulu      16  .     d           --  38  30  27    (+2n set bits)
mulu      16  .     <ea>        --  38  30  27    +EA
neg        8  .     d           --   4   4   2
neg        8  .     <ea>        --   8   8   4    +EA
neg       16  .     d           --   4   4   2
neg       16  .     <ea>        --   8   8   4    +EA
neg       32  .     d           --   6   6   2
neg       32  .     <ea>        --  12  12   4    +EA
negx       8  .     d           --   4   4   2
negx       8  .     <ea>        --   8   8   4    +EA
negx      16  .     d           --   4   4   2
negx      16  .     <ea>        --   8   8   4    +EA
negx      32  .     d           --   6   6   2
negx      32  .     <ea>        --  12  12   4    +EA
not        8  .     d           --   4   4   2
not        8  .     <ea>        --   8   8   4    +EA
not       16  .     d           --   4   4   2
not       16  .     <ea>        --   8   8   4    +EA
not       32  .     d           --   6   6   2
not       32  .     <ea>        --  12  12   4    +EA
or         8  er    d           --   4   4   2
or         8  er    <ea>        --   4   4   2    +EA
or        16  er    d           --   4   4   2
or        16  er    <ea>        --   4   4   2    +EA
or        32  er    d           --   6   6   2
or        32  er    <ea>        --   6   6   2    +EA
or         8  re    <ea>        --   8   8   4    +EA
or        16  re    <ea>        --   8   8   4    +EA
or        32  re    <ea>        --  12  12   4    +EA
ori       16  toc   .           --  20  16  12
ori       16  tos   .           --  20  16  12    (priv)
ori        8  .     d           --   8   8   2
ori        8  .     <ea>        --  12  12   4    +EA
ori       16  .     d           --   8   8   2
ori       16  .     <ea>        --  12  12   4    +EA
ori       32  .     d           --  16  14   2
ori       32  .     <ea>        --  20  20   4    +EA
sub        8  er    d           --   4   4   2
sub       16  er    d           --   4   4   2
sub       16  er    a           --   4   4   2
sub       32  er    d           --   6   6   2
sub       32  er    a           --   6   6   2
sub        8  re    <ea>        --   8   8   4    +EA
sub       16  re    <ea>        --   8   8   4    +EA
sub       32  re    <ea>        --  12  12   4    +EA
suba      16  .     <any>       --   8   8   2
suba      32  .     <any>       --   6   6   2
subi       8  .     d           --   8   8   2
subi      16  .     d           --   8   8   2
subi      32  .     d           --  16  14   2
subi       8  .     <ea>        --  12  12   4    +EA
subi      16  .     <ea>        --  12  12   4    +EA
subi      32  .     <ea>        --  20  20   4    +EA
subq       8  .     d           --   4   4   2
subq      16  .     d           --   4   4   2
subq      16  .     a           --   8   4   2
subq      32  .     d           --   8   8   2
subq      32  .     a           --   8   8   2
subx       8  rr    .           --   4   4   2
subx      16  rr    .           --   4   4   2
subx      32  rr    .           --   8   6   2
subx       8  mm    .           --  18  18  12
subx      16  mm    .           --  18  18  12
subx      32  mm    .           --  30  30  12
```

### F.2 Shift and Rotate

All shift/rotate instructions on the 68000/68010 add 2 cycles per
shift position when the count is from a register. The table shows
the base time before that addition.

```
asr        8  s     .           --   6   6   6    (+2n shift, 68000/010)
asr       16  s     .           --   6   6   6
asr       32  s     .           --   8   8   6
asr        8  r     .           --   6   6   6    (+2n shift, 68000/010)
asr       16  r     .           --   6   6   6
asr       32  r     .           --   8   8   6
asr       16  .     <ea>        --   8   8   5    (memory, shift by 1)
asl        8  s     .           --   6   6   8
asl       16  s     .           --   6   6   8
asl       32  s     .           --   8   8   8
asl        8  r     .           --   6   6   8
asl       16  r     .           --   6   6   8
asl       32  r     .           --   8   8   8
asl       16  .     <ea>        --   8   8   6
lsr        8  s     .           --   6   6   4
lsr       16  s     .           --   6   6   4
lsr       32  s     .           --   8   8   4
lsr        8  r     .           --   6   6   6
lsr       16  r     .           --   6   6   6
lsr       32  r     .           --   8   8   6
lsr       16  .     <ea>        --   8   8   5
lsl        8  s     .           --   6   6   4
lsl       16  s     .           --   6   6   4
lsl       32  s     .           --   8   8   4
lsl        8  r     .           --   6   6   6
lsl       16  r     .           --   6   6   6
lsl       32  r     .           --   8   8   6
lsl       16  .     <ea>        --   8   8   5
ror        8  s     .           --   6   6   8
ror       16  s     .           --   6   6   8
ror       32  s     .           --   8   8   8
ror        8  r     .           --   6   6   8
ror       16  r     .           --   6   6   8
ror       32  r     .           --   8   8   8
ror       16  .     <ea>        --   8   8   7
rol        8  s     .           --   6   6   8
rol       16  s     .           --   6   6   8
rol       32  s     .           --   8   8   8
rol        8  r     .           --   6   6   8
rol       16  r     .           --   6   6   8
rol       32  r     .           --   8   8   8
rol       16  .     <ea>        --   8   8   7
roxr       8  s     .           --   6   6  12
roxr      16  s     .           --   6   6  12
roxr      32  s     .           --   8   8  12
roxr       8  r     .           --   6   6  12
roxr      16  r     .           --   6   6  12
roxr      32  r     .           --   8   8  12
roxr      16  .     <ea>        --   8   8   5
roxl       8  s     .           --   6   6  12
roxl      16  s     .           --   6   6  12
roxl      32  s     .           --   8   8  12
roxl       8  r     .           --   6   6  12
roxl      16  r     .           --   6   6  12
roxl      32  r     .           --   8   8  12
roxl      16  .     <ea>        --   8   8   5
```

### F.3 Bit Manipulation

```
bchg       8  r     <ea>        --   8   8   4    +EA
bchg      32  r     d           --   8   8   4
bchg       8  s     <ea>        --  12  12   4    +EA
bchg      32  s     d           --  12  12   4
bclr       8  r     <ea>        --   8  10   4    +EA (note: 010 = 10!)
bclr      32  r     d           --  10  10   4
bclr       8  s     <ea>        --  12  12   4    +EA
bclr      32  s     d           --  14  14   4
bset      32  r     d           --   8   8   4
bset       8  r     <ea>        --   8   8   4    +EA
bset       8  s     <ea>        --  12  12   4    +EA
bset      32  s     d           --  12  12   4
btst       8  r     <ea>        --   4   4   4    +EA
btst      32  r     d           --   6   6   4
btst       8  s     <ea>        --   8   8   4    +EA
btst      32  s     d           --  10  10   4
```

### F.4 Branch and Program Control

```
bcc        8  .     .           --  10  10   6    (taken)
bcc        8  (not taken)       --   8   8   4
bcc       16  .     .           --  10  10   6
bcc       32  .     .           --  10  10   6    (68020+ only for .L)
bra        8  .     .           --  10  10  10
bra       16  .     .           --  10  10  10
bra       32  .     .           --  10  10  10
bsr        8  .     .           --  18  18   7
bsr       16  .     .           --  18  18   7
bsr       32  .     .           --  18  18   7
dbt       16  .     .           --  12  12   6
dbf       16  .     .           --  12  12   6
dbcc      16  .     .           --  12  12   6
jmp       32  .     <ea>        --   4   4   0+EA
jsr       32  .     <ea>        --  12  12   0+EA
rts       32  .     .           --  16  16  10
rte       32  .     .           --  20  24  20
rtr       32  .     .           --  20  20  14
rtd       32  .     .           --   .  16  10    (68010+ only)
link      16  .     .           --  16  16   5
link      32  .     .           --   .   .   6    (68020+ only)
unlk      32  .     .           --  12  12   6
```

### F.5 BCD Operations

```
abcd       8  rr    .           --   6   6   4
abcd       8  mm    .           --  18  18  16
sbcd       8  rr    .           --   6   6   4
sbcd       8  mm    .           --  18  18  16
nbcd       8  .     d           --   6   6   6
nbcd       8  .     <ea>        --   8   8   6    +EA
```

### F.6 Miscellaneous

```
clr        8  .     d           --   4   4   2
clr        8  .     <ea>        --   8   4   4    +EA (010 optimised)
clr       16  .     d           --   4   4   2
clr       16  .     <ea>        --   8   4   4    +EA
clr       32  .     d           --   6   6   2
clr       32  .     <ea>        --  12   6   4    +EA
chk       16  .     d           --  10   8   8
chk       16  .     <ea>        --  10   8   8    +EA
ext       16  .     .           --   4   4   4
ext       32  .     .           --   4   4   4
extb      32  .     .           --   .   .   4    (68020+ only)
exg       32  dd    .           --   6   6   2
exg       32  aa    .           --   6   6   2
exg       32  da    .           --   6   6   2
illegal    0  .     .           --   4   4   4    (+exception overhead)
lea       32  .     <ea>        --   0   0   2    +EA (no memory access)
pea       32  .     <ea>        --   6   6   5    +EA
nop        0  .     .           --   4   4   2
swap      32  .     .           --   4   4   4
tas        8  .     d           --   4   4   4
tas        8  .     <ea>        --  14  14  12    +EA (RMW cycle!)
trap       0  .     .           --   4   4   4    (+exception overhead)
trapv      0  .     .           --   4   4   4
tst        8  .     d           --   4   4   2
tst        8  .     <ea>        --   4   4   2    +EA
tst       16  .     d           --   4   4   2
tst       16  .     <ea>        --   4   4   2    +EA
tst       32  .     d           --   4   4   2
tst       32  .     <ea>        --   4   4   2    +EA
scc        8  .     d           --   4   4   4    (false)
scc        8  .     d           --   6   4   4    (true, 68000 = 6)
scc        8  .     <ea>        --   8   8   6    +EA
stop       0  .     .           --   4   4   8    (privileged)
reset      0  .     .           --   0   0   0    (+CYC_RESET=132/130/518)
bkpt       0  .     .           --   .  10  10    (68010+ only)
```

### F.7 MOVE Variants (Selected)

```
move       8  d     d           --   4   4   2
move       8  d     <ea>        --   4   4   2    +EA src
move       8  ai    d           --   8   8   4    (An) destination
move       8  pi    d           --   8   8   4    (An)+ destination
move       8  pd    d           --   8   8   5    -(An) destination
move       8  di    d           --  12  12   5    d16(An) destination
move       8  ix    d           --  14  14   7    d8(An,Xn) dest
move       8  aw    d           --  12  12   4    xxx.W destination
move       8  al    d           --  16  16   6    xxx.L destination
move      16  d     d           --   4   4   2    (same pattern)
move      32  d     d           --   4   4   2
move      32  ai    d           --  12  12   4
move      32  pi    d           --  12  12   4
move      32  pd    d           --  12  14   5    (note: 68010 = 14!)
move      32  di    d           --  16  16   5
move      32  ix    d           --  18  18   7
move      32  aw    d           --  16  16   4
move      32  al    d           --  20  20   6
movea     16  .     d           --   4   4   2
movea     16  .     a           --   4   4   2
movea     32  .     d           --   4   4   2
movea     32  .     a           --   4   4   2
moveq     32  .     .           --   4   4   2
move      16  frc   d           --   .   4   4    (from CCR, 68010+)
move      16  frc   <ea>        --   .   8   4
move      16  toc   d           --  12  12   4    (to CCR)
move      16  toc   <ea>        --  12  12   4
move      16  frs   d           --   6   4   8    (from SR; U on 000!)
move      16  frs   <ea>        --   8   8   8
move      16  tos   d           --  12  12   8    (to SR, privileged)
move      16  tos   <ea>        --  12  12   8
move      32  fru   .           --   4   6   2    (from USP, priv)
move      32  tou   .           --   4   6   2    (to USP, priv)
movec     32  cr    .           --   .  12   6    (68010+, priv)
movec     32  rc    .           --   .  10  12
movem     16  re    pd          --   8   8   4    +4n (68000 per reg)
movem     16  re    <ea>        --   8   8   4    +4n
movem     32  re    pd          --   8   8   4    +8n
movem     32  re    <ea>        --   8   8   4    +8n
movem     16  er    pi          --  12  12   8    +4n
movem     16  er    <ea>        --  12  12   8    +4n
movem     32  er    pi          --  12  12   8    +8n
movem     32  er    <ea>        --  12  12   8    +8n
movep     16  er    .           --  16  16  12
movep     32  er    .           --  24  24  18
movep     16  re    .           --  16  16  11
movep     32  re    .           --  24  24  17
moves      8  .     <ea>        --   .  14   5    (68010+, priv)
moves     16  .     <ea>        --   .  14   5
moves     32  .     <ea>        --   .  16   5
```

---

## Appendix G: Undocumented 68000 Behaviour

Several undocumented behaviours of the 68000 affect emulation accuracy.
These have been discovered through testing on real hardware and are
documented by the emulation community.

### G.1 Bus Error Frame Undocumented Bits

The 68000 bus/address error stack frame has undocumented content in the
upper bits of the "additional information" word at SSP+0. These bits
contain parts of the opcode being executed at the time of the error.
WinUAE replicates this:

```c
// (WinUAE newcpu.cpp, line 2802)
mode |= last_op_for_exception_3 & ~31;
```

Some copy protection and anti-debugging code checks these bits.

### G.2 CLR Reads Before Writing (68000 Only)

On the 68000, the CLR instruction to memory performs a **read** of the
target location before writing zeros. This is visible as an extra bus
cycle. The 68010 optimised this away -- CLR on the 68010+ writes
without a preceding read.

This matters for hardware registers where a read has side effects
(e.g., clearing an interrupt flag). The Musashi timing table reflects
this: CLR.L (An) = 12 cycles on 68000 but 6 cycles on 68010.

### G.3 MOVE.W SR on 68000

On the 68000, MOVE from SR is a user-mode instruction. This was
considered a security flaw (user code can read the supervisor bit
and interrupt mask), and the 68010 made it privileged. The 68010 added
MOVE from CCR as the user-mode replacement.

This is not truly "undocumented" -- Motorola documented the change --
but it is the most common source of compatibility issues between
68000 and 68010+ emulation.

### G.4 Address Error During Exception Processing

If an address error occurs while the CPU is stacking an exception
frame (e.g., the SSP is odd), the 68000 enters a double-fault
condition and halts. WinUAE checks for this:

```c
// (WinUAE newcpu.cpp, line 2788)
if ((m68k_areg(regs, 7) & 1) || exception_in_exception < 0) {
    cpu_halt(CPU_HALT_DOUBLE_FAULT);
    return;
}
```

### G.5 Prefetch Timing of Branch Instructions

Branch instructions (BRA, Bcc, BSR) have a subtlety: the "not taken"
path is faster than the "taken" path because the taken path must
flush the prefetch pipeline and fetch from the new PC. On the 68000:

- Bcc taken:     10 cycles
- Bcc not taken:  8 cycles

This 2-cycle difference matters for tight timing loops.

### G.6 DIVS/DIVU Early Overflow Detection

When DIVS or DIVU detects that the quotient will overflow (not fit
in 16 bits), it can terminate early without completing the full
division. The actual cycle count in this case is less than the
worst-case 158/140 cycles.

Musashi does not model the variable timing of division -- it uses the
worst-case value from the timing table. For full accuracy, the
division algorithm timing should be modelled step by step, but this
is rarely important in practice.

### G.7 STOP and Interrupt Timing

The STOP instruction halts the CPU until an interrupt of sufficient
priority arrives. The interrupt is serviced immediately -- there is no
instruction completion delay since STOP itself is the last instruction
executed.

The cycle count listed for STOP (4 cycles on the 68000) is the cost
of executing the instruction itself, not the time spent waiting. The
waiting period consumes zero CPU cycles (the processor is stopped).

### G.8 A7 Alignment

The 68000 forces A7 (the stack pointer) to remain word-aligned by
incrementing/decrementing by 2 instead of 1 for byte operations using
A7 with pre-decrement or post-increment addressing modes. This applies
ONLY to A7, not to A0-A6.

```asm
    MOVE.B D0,-(A7)    ; A7 decremented by 2, not 1
    MOVE.B (A7)+,D0    ; A7 incremented by 2, not 1
    MOVE.B D0,-(A0)    ; A0 decremented by 1 (normal)
```

Musashi models this:
```c
// (Musashi m68kcpu.h)
#define EA_A7_PI_8()   ((REG_A[7] += 2) - 2)
#define EA_A7_PD_8()   (REG_A[7] -= 2)
```

---

*Document generated 2026-04-11. Sources: M68000 Family Reference (1988),
Musashi 4.60, WinUAE (current), Amiga hardware documentation.*

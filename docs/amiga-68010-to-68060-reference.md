# Amiga 68010-68060 CPU Family Reference

**Purpose:** Implementation reference for Amiga emulator authors covering the
MC68010 through MC68060 processors. Focuses on what differs from the 68000
baseline and what matters for correct emulation.

**Audience:** Emulator authors who already understand the 68000 (covered in
the companion [Amiga 68000 Timing Reference](amiga-68000-timing.md)) and
need to implement later CPU models.

**Sources:**
- MC68010 16/32-Bit Virtual Memory Microprocessor Technical Data -- cited as `(MC68010)`
- MC68020 32-Bit Microprocessor User's Manual, First Edition -- cited as `(MC68020UM)`
- MC68030 Enhanced 32-Bit Microprocessor User's Manual, Third Edition (Part 1 only) -- cited as `(MC68030UM)`
- MC68040 Third Generation Microprocessor User's Manual -- cited as `(MC68040UM)`
- MC68060 Microprocessor User's Manual -- cited as `(MC68060UM)`
- Motorola M68000 Family Reference (1988) -- cited as `(M68000 Family Ref)`
- Companion docs: `amiga-68000-timing.md` (68000 baseline), `amiga-fpu-68881-reference.md` (FPU details)

**Conventions:**
- "68000 baseline" refers to the companion 68000 doc -- not duplicated here.
- Register sizes are in bits unless stated otherwise.
- Cycle counts assume cache hits and aligned operands unless stated otherwise.
- Stack frame diagrams use word (16-bit) offsets from SP.

---

## Table of Contents

### Per-CPU Sections
1. [MC68010](#1-mc68010)
2. [MC68020](#2-mc68020)
3. [MC68030](#3-mc68030)
4. [MC68040](#4-mc68040)
5. [MC68060](#5-mc68060)

### Cross-Cutting Sections
6. [Amiga-Specific CPU Quirks](#6-amiga-specific-cpu-quirks)
7. [MOVEC Register Reference](#7-movec-register-reference)
8. [Instruction Encoding: Full Extension Words (68020+)](#8-instruction-encoding-full-extension-words-68020)
9. [Superscalar Considerations (68060)](#9-superscalar-considerations-68060)

### Appendices
- [E0. Implementation Checklist for Emulator Authors](#appendix-e0-implementation-checklist-for-emulator-authors)
- [A. Exception Stack Frame Catalogue](#appendix-a-exception-stack-frame-catalogue)
- [B. Instruction Set Delta Tables](#appendix-b-instruction-set-delta-tables)
- [C. Cache Control Quick Reference](#appendix-c-cache-control-quick-reference)
- [D. Gaps and Source Map](#appendix-d-gaps-and-source-map)
- [E. Detailed Instruction Timing Tables](#appendix-e-detailed-instruction-timing-tables)
- [F. Detailed Register Layouts](#appendix-f-detailed-register-layouts)
- [G. Exception Vector Table (68010-68060)](#appendix-g-exception-vector-table-68010-68060)
- [H. Addressing Mode Comparison](#appendix-h-addressing-mode-comparison)
- [I. Bus Signal Comparison](#appendix-i-bus-signal-comparison)
- [J. Amiga System CPU Clock Configurations](#appendix-j-amiga-system-cpu-clock-configurations)
- [K. CPU Feature Detection](#appendix-k-cpu-feature-detection)
- [L. Performance Scaling Across the Family](#appendix-l-performance-scaling-across-the-family)

---

## 1. MC68010

### 1.1 Overview

The MC68010 is a pin-for-pin compatible upgrade to the MC68000. It was never
used in production Amiga models, but some third-party accelerator boards used
it. Its significance for Amiga emulation is that it introduced several features
that all later CPUs build on: the VBR, MOVEC/MOVES instructions, and
recoverable bus/address errors. `(MC68010 p.3-136)`

| Feature          | Value                                      |
|------------------|--------------------------------------------|
| **Data bus**     | 16-bit external, 32-bit internal registers |
| **Address bus**  | 24-bit (16 MB)                             |
| **Clock**        | 8, 10, or 12.5 MHz                        |
| **Package**      | 64-pin DIP or 68-pin PLCC/PGA             |
| **Caches**       | None                                       |
| **MMU**          | None (supports external via bus error recovery) |
| **FPU**          | None                                       |
| **Amiga usage**  | Rare -- some accelerator boards only       |

### 1.2 Instruction Set Additions

The 68010 adds three instructions not present in the 68000 and changes the
privilege level of one existing instruction.

| Mnemonic       | Description                              | Privilege |
|----------------|------------------------------------------|-----------|
| `MOVEC Rc,Rn`  | Move to/from control register            | Supervisor |
| `MOVES Rn,<ea>`| Move to/from alternate address space (using SFC/DFC) | Supervisor |
| `MOVE from CCR`| Move from CCR (new -- was not in 68000)  | User      |

**MOVE from SR is now privileged.** On the 68000, `MOVE SR,<ea>` is a
user-mode instruction. On the 68010 and all later CPUs, it causes a privilege
violation exception if executed in user mode. This is the single most
important compatibility break in the 68000 family and the reason Kickstart
1.x code needs patching on 68010+ systems. `(MC68010 p.3-142)`

AmigaOS 2.0+ handles this by installing a privilege-violation exception
handler that emulates the old behaviour: if the faulting instruction is
`MOVE SR,<ea>`, the handler executes `MOVE CCR,<ea>` instead and returns.

### 1.3 Virtual Memory Support: Bus/Address Error Recovery

The 68000 cannot recover from bus or address errors -- the exception stacks
a short frame and the instruction is lost. The 68010 changes this
fundamentally. `(MC68010 p.3-143)`

When a bus or address error occurs, the 68010:
1. Saves its complete internal state on the supervisor stack (a "long frame"
   of 29 words / 58 bytes).
2. Vectors to the bus/address error handler.
3. When the handler executes RTE, the CPU reloads all internal state from the
   stack frame and **continues the faulted instruction from where it left off**.

This enables external MMU support (e.g., MC68451) and virtual memory, even
though the 68010 has no on-chip MMU. The OS can page in the missing memory
and RTE back to retry the instruction transparently.

For emulation: you must implement the long stack frame if you want to support
68010 bus error recovery. The frame includes internal pipeline state, the
data output buffer, and the instruction input buffer.

### 1.4 Loop Mode

The 68010 has a loop mode optimisation for tight DBcc loops. When the
instruction stream contains:

1. A "loopable" instruction (most single-operand instructions -- marked with
   `*` in the instruction table)
2. Followed by DBcc with a branch displacement back to the loopable instruction

...the 68010 holds all three words (instruction, DBcc opword, displacement)
internally and stops fetching from memory. Only data accesses are performed.
This provides a significant speedup for block copy/fill operations.
`(MC68010 p.3-143)`

33 instructions support loop mode, including ADD, SUB, MOVE, CMP, AND, OR,
EOR, and their variants.

For emulation: loop mode is a performance optimisation only. The instruction
stream behaves identically whether or not loop mode is active. You can ignore
it for functional correctness and implement it later as a fast-path.

### 1.5 Vector Base Register (VBR)

The VBR is a new 32-bit supervisor register that holds the base address of
the exception vector table. On the 68000, the vector table is always at
address $000000. On the 68010+, the vector table can be relocated to any
address by writing to VBR via `MOVEC`. `(MC68010 p.3-138)`

```
Exception vector address = VBR + (vector number * 4)
```

On reset, VBR = $00000000 (compatible with 68000).

The VBR is important for Amiga emulation because:
- AmigaOS 2.0+ uses VBR to move the vector table into fast RAM on 68010+
  systems, eliminating chip RAM contention for exception handling.
- The "Kickstart ROM in RAM" trick on 68030+ uses VBR relocation.

### 1.6 Alternate Function Code Registers (SFC/DFC)

Two 3-bit registers (SFC and DFC) are added to the supervisor programming
model. They are used with the MOVES instruction to specify the address space
for data transfers. The function codes let supervisor code access user data
space or emulate CPU space cycles. `(MC68010 p.3-138)`

| Register | Width | Description |
|----------|-------|-------------|
| SFC      | 3-bit | Source Function Code -- used as FC for MOVES source |
| DFC      | 3-bit | Destination Function Code -- used as FC for MOVES destination |

### 1.7 Exception Model Changes

The 68010 introduces the **format word** in exception stack frames. Every
exception stack frame now includes a 16-bit format/vector-offset word at
SP+6 that identifies the frame type. This is critical for RTE, which reads
the format word to determine how much data to unstack. `(MC68010)`

**68010 Stack Frame Formats:**

Format $0: Four-word frame (normal exceptions)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  |  0000 | Vector Offset            |
```

Format $8: Long frame (bus/address error -- 29 words)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  |  1000 | Vector Offset            |
SP+$08  |  Special Status Word             |
SP+$0A  |  Fault Address (high)            |
SP+$0C  |  Fault Address (low)             |
SP+$0E  |  (unused / reserved)             |
SP+$10  |  Data Output Buffer (high)       |
SP+$12  |  Data Output Buffer (low)        |
SP+$14  |  (unused / reserved)             |
SP+$16  |  Data Input Buffer (high)        |
SP+$18  |  Data Input Buffer (low)         |
SP+$1A  |  (unused / reserved)             |
SP+$1C  |  Instruction Input Buffer        |
SP+$1E  |  Internal Information (16 words) |
 ...    |  ...                             |
SP+$3C  |  (end of 29-word frame)          |
```

The long frame saves the complete CPU pipeline state, enabling the bus error
handler to fix up memory (e.g., page in from disk) and RTE to resume the
faulted instruction exactly.

**Contrast with 68000:** The 68000 has no format word. Its bus/address error
frame (group 0) is a fixed 7-word frame that saves incomplete state -- the
instruction cannot be restarted. The 68000's RTE always unstacks exactly 3
words (SR + PC). The 68010's RTE reads the format word first.

### 1.8 Bus Interface

Identical to the 68000: 16-bit synchronous data bus, 24-bit address bus,
asynchronous handshake via DTACK. Same pin assignments (64-pin DIP).
`(MC68010 p.3-137)`

### 1.9 Timing Differences from 68000

The 68010 has the same microarchitecture as the 68000 but with improved
microcode. Most instructions execute in the same number of cycles, but the
loop mode optimisation can significantly speed up DBcc loops by eliminating
instruction fetch cycles. `(MC68010 p.3-136)`

---

## 2. MC68020

### 2.1 Overview

The MC68020 is the first true 32-bit member of the 68000 family. It has a
32-bit data bus, 32-bit address bus, instruction cache, and a coprocessor
interface. The Amiga A1200 uses the MC68EC020 (24-bit address bus variant).
`(MC68020UM §1)`

| Feature          | Value                                      |
|------------------|--------------------------------------------|
| **Data bus**     | 32-bit (EC020: still 32-bit)               |
| **Address bus**  | 32-bit (EC020: 24-bit external, 32-bit internal for cache tags) |
| **Clock**        | 12.5, 16.67, 20, 25, 33 MHz               |
| **Package**      | 114-pin PGA or 100-pin PQFP               |
| **Caches**       | 256-byte instruction cache (64 longword entries, direct-mapped) |
| **MMU**          | External only (MC68851 PMMU)               |
| **FPU**          | External (MC68881/MC68882 via coprocessor interface) |
| **Amiga usage**  | A1200 (68EC020), A2500 (accelerator), many accelerator boards |

### 2.2 Instruction Set Additions

The 68020 adds a substantial number of new instructions. This is the biggest
instruction set expansion in the 68000 family. `(MC68020UM §1.4)`

#### 2.2.1 Bit Field Operations
| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `BFCHG`  | Test Bit Field and Change | User |
| `BFCLR`  | Test Bit Field and Clear | User |
| `BFEXTS` | Extract Bit Field Signed | User |
| `BFEXTU` | Extract Bit Field Unsigned | User |
| `BFFFO`  | Find First One in Bit Field | User |
| `BFINS`  | Insert Bit Field | User |
| `BFSET`  | Test Bit Field and Set | User |
| `BFTST`  | Test Bit Field | User |

Bit field instructions operate on arbitrary bit fields of 1-32 bits at any
bit offset. The field is specified by {offset:width} where offset and width
can be immediate values or data register contents.

#### 2.2.2 BCD Conversion
| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `PACK`   | Pack BCD: adjust and pack two unpacked BCD digits | User |
| `UNPK`   | Unpack BCD: expand packed BCD to unpacked | User |

#### 2.2.3 Compare and Swap (Multiprocessor Primitives)
| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `CAS`    | Compare and Swap with Operand (8/16/32-bit) | User |
| `CAS2`   | Compare and Swap Dual Operand (16/32-bit, two addresses) | User |

CAS performs an atomic compare-and-swap using an indivisible
read-modify-write bus cycle. CAS2 does two simultaneous compare-and-swaps
(useful for atomic linked-list operations in multiprocessor systems).

**Important:** CAS2 is removed on the 68060 (emulated via F-line trap).

#### 2.2.4 Module Call (removed on 68040+)
| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `CALLM`  | Call Module | User |
| `RTM`    | Return from Module | User |

CALLM/RTM implement a call/return mechanism with automatic module descriptor
validation. These instructions were removed starting with the 68040 and
generate an illegal instruction exception on 68040+.

**For emulation:** If your emulator supports 68020 or 68030 mode, implement
these. If targeting 68040+, generate illegal instruction exception.

#### 2.2.5 Extended Multiply and Divide
| Mnemonic  | Description | Privilege |
|-----------|-------------|-----------|
| `MULS.L`  | Signed multiply 32x32->32 or 32x32->64 | User |
| `MULU.L`  | Unsigned multiply 32x32->32 or 32x32->64 | User |
| `DIVS.L`  | Signed divide 32/32->32:32 or 64/32->32:32 | User |
| `DIVU.L`  | Unsigned divide 32/32->32:32 or 64/32->32:32 | User |

The 68000 only supports 16x16->32 multiply and 32/16->16:16 divide. The
68020 extends these to full 32-bit operands and optionally 64-bit results.

#### 2.2.6 Conditional Trap
| Mnemonic  | Description | Privilege |
|-----------|-------------|-----------|
| `TRAPcc`  | Trap on Condition (with optional word/long operand) | User |

Traps if condition code cc is true. The optional operand is passed to the
trap handler but not used by the CPU.

#### 2.2.7 Extended Shift
| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `EXTB.L` | Sign-extend byte to long (new size) | User |

#### 2.2.8 Coprocessor Instructions
| Mnemonic      | Description | Privilege |
|---------------|-------------|-----------|
| `cpBcc`       | Branch on coprocessor condition | User |
| `cpDBcc`      | Test coprocessor condition, decrement, branch | User |
| `cpGEN`       | Coprocessor general instruction | User |
| `cpScc`       | Set on coprocessor condition | User |
| `cpTRAPcc`    | Trap on coprocessor condition | User |
| `cpSAVE`      | Save coprocessor internal state | Supervisor |
| `cpRESTORE`   | Restore coprocessor internal state | Supervisor |

These use the F-line encoding space (bits 15-12 = $F). Bits 11-9 encode the
coprocessor ID (CpID). CpID 001 = MC68881/MC68882 FPU. CpID 000 is reserved
(not a coprocessor instruction). CpIDs 000-101 are reserved for Motorola;
110-111 for user-defined coprocessors. `(MC68020UM §7.1.3)`

**For emulation:** When CpID=001, these become the FPU instructions (FADD,
FSUB, etc.). See the companion FPU reference for details. On systems without
an FPU, these generate F-line exceptions.

### 2.3 Addressing Modes

The 68020 expands the addressing modes from 14 (68000/68010) to 18. The new
modes use a "full extension word" format (see Section 8 for encoding
details). `(MC68020UM §1.3)`

**New addressing modes (68020+):**

| Mode | Syntax | Description |
|------|--------|-------------|
| Address Register Indirect with Index (Base Displacement) | `(bd,An,Xn)` | Base displacement replaces 8-bit d8 with 16/32-bit bd |
| Memory Indirect Postindexed | `([bd,An],Xn,od)` | Indirect through memory with index added after |
| Memory Indirect Preindexed | `([bd,An,Xn],od)` | Indirect through memory with index added before |
| PC Indirect with Index (Base Displacement) | `(bd,PC,Xn)` | PC-relative with base displacement |
| PC Memory Indirect Postindexed | `([bd,PC],Xn,od)` | PC-relative memory indirect postindexed |
| PC Memory Indirect Preindexed | `([bd,PC,Xn],od)` | PC-relative memory indirect preindexed |

**Key features of 68020+ addressing:**
- **Scale factor**: Index register can be scaled by 1, 2, 4, or 8 (encoded
  in the extension word). The 68000 only supports scale=1.
- **Base displacement (bd)**: 0, 16-bit, or 32-bit displacement replaces the
  68000's fixed 8-bit displacement.
- **Outer displacement (od)**: Additional 0, 16-bit, or 32-bit displacement
  after the memory indirect access.
- **Base/index suppression**: Either An or Xn can be suppressed (treated as 0)
  in the full extension word format.

**Emulation impact:** These modes require additional memory reads (for the
memory indirect step) and are significantly slower than the basic modes. The
memory indirect modes add 4-8 cycles for the indirection.

### 2.4 Exception Model Changes

The 68020 has six different stack frame formats. The format code (bits 15-12
of the format/vector-offset word at SP+6) identifies the frame type.
`(MC68020UM §6.4)`

| Format | Size (words) | Use |
|--------|-------------|-----|
| $0 | 4 | Normal: interrupts, traps, privilege violations |
| $1 | 4 | Throwaway: created on interrupt stack during ISP->MSP transition |
| $2 | 6 | Six-word: CHK, CHK2, TRAPcc, trace, zero divide, MMU config, cp post-instruction |
| $9 | 10 | Coprocessor mid-instruction |
| $A | 16 | Short bus fault: bus/address error at instruction boundary |
| $B | 46 | Long bus fault: bus/address error during instruction execution |

**Master/Interrupt Stack Pointer:** The 68020 adds a Master Stack Pointer
(MSP) and changes the meaning of A7 in supervisor mode. When the M bit in
the SR is set, A7 refers to MSP. When an interrupt occurs, the CPU switches
to the Interrupt Stack Pointer (ISP), saves a throwaway frame on ISP, and
saves the real frame on MSP. This separates interrupt stacks from normal
supervisor stacks. `(MC68020UM §1.2)`

**Format $0: Four-word stack frame**
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0000 | Vector Offset             |
```

**Format $2: Six-word stack frame**
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0010 | Vector Offset             |
SP+$08  |  Instruction Address (high)      |
SP+$0A  |  Instruction Address (low)       |
```

**Format $A: Short bus fault (16 words)**
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 1010 | Vector Offset             |
SP+$08  |  Internal Register               |
SP+$0A  |  Special Status Register (SSW)   |
SP+$0C  |  Instruction Pipe Stage C        |
SP+$0E  |  Instruction Pipe Stage B        |
SP+$10  |  Data Cycle Fault Address (high)  |
SP+$12  |  Data Cycle Fault Address (low)   |
SP+$14  |  Internal Register               |
SP+$16  |  Internal Register               |
SP+$18  |  Data Output Buffer (high)       |
SP+$1A  |  Data Output Buffer (low)        |
SP+$1C  |  Internal Register               |
SP+$1E  |  Internal Register               |
```

**Format $B: Long bus fault (46 words)**
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 1011 | Vector Offset             |
SP+$08  |  Internal Register               |
SP+$0A  |  Special Status Register (SSW)   |
SP+$0C  |  Instruction Pipe Stage C        |
SP+$0E  |  Instruction Pipe Stage B        |
SP+$10  |  Data Cycle Fault Address (high)  |
SP+$12  |  Data Cycle Fault Address (low)   |
SP+$14  |  Internal Register               |
SP+$16  |  Internal Register               |
SP+$18  |  Data Output Buffer (high)       |
SP+$1A  |  Data Output Buffer (low)        |
SP+$1C  |  Internal Registers (4 words)    |
 ...    |  ...                             |
SP+$22  |                                  |
SP+$24  |  Stage B Address (high)          |
SP+$26  |  Stage B Address (low)           |
SP+$28  |  Internal Registers (2 words)    |
SP+$2A  |                                  |
SP+$2C  |  Data Input Buffer (high)        |
SP+$2E  |  Data Input Buffer (low)         |
SP+$30  |  Internal Registers (3 words)    |
 ...    |  ...                             |
SP+$36  |                                  |
SP+$38  | Version # | Internal Information |
SP+$3A  |  Internal Registers (18 words)   |
 ...    |  ...                             |
SP+$5A  |  (end of 46-word frame)          |
```

The short bus fault (format $A) is used when the bus error occurs at an
instruction boundary (the execution unit has completed). The long bus fault
(format $B) is used when the bus error occurs during instruction execution.
`(MC68020UM §6.4)`

### 2.5 Cache

The 68020 has a 256-byte instruction-only cache, the first on-chip cache in
the 68000 family. `(MC68020UM §4.1)`

| Property | Value |
|----------|-------|
| Size | 256 bytes (64 longword entries) |
| Type | Instruction only -- data is never cached |
| Organisation | Direct-mapped |
| Line size | 1 longword (4 bytes, 2 words) |
| Tag | A31-A8 + FC2 (24 bits + 1 bit) |
| Index | A7-A2 (6 bits = 64 entries) |
| Word select | A1 |
| Write policy | N/A (instruction only) |

**CACR (Cache Control Register):** `(MC68020UM §4.3.1)`
```
Bit 31-4:  Reserved (read as 0)
Bit 3:     C  -- Clear Cache (write 1 to clear all entries; reads as 0)
Bit 2:     CE -- Clear Entry (clear entry specified by CAAR; reads as 0)
Bit 1:     F  -- Freeze Cache (1 = don't replace on miss)
Bit 0:     E  -- Enable Cache (1 = cache enabled)
```

**CAAR (Cache Address Register):** Contains the address used for the CE
(clear entry) operation. Only the index field (A7-A2) is used. `(MC68020UM §4.3.2)`

**Cache operation:**
- On instruction fetch, if the cache is enabled, the index field selects an
  entry. If the tag matches and the valid bit is set, the word is supplied
  from cache (cache hit). Otherwise, the instruction is fetched from memory
  and the entry is updated (unless freeze is set). `(MC68020UM §4.1)`
- Data accesses are **never** cached regardless of address space.
- Reset clears all valid bits and clears E and F bits.

**For emulation:** The instruction cache is small enough that many emulators
ignore it entirely, since it only affects timing, not functionality. However,
self-modifying code (library patches, JIT code) will execute stale cached
instructions unless `CacheClearU()` flushes the cache. AmigaOS calls
`CacheClearU()` after patching ROM functions.

### 2.6 Bus Interface: Dynamic Bus Sizing

The 68020 introduces **dynamic bus sizing**, a fundamental change from the
68000/68010. The CPU can talk to 8-bit, 16-bit, or 32-bit memory/devices
automatically. `(MC68020UM §5.2.1)`

The external device signals its port size via DSACK1/DSACK0 (replacing DTACK):

| DSACK1 | DSACK0 | Port Size | Meaning |
|--------|--------|-----------|---------|
| 1 | 1 | -- | Wait (insert wait state) |
| 1 | 0 | 8-bit | Byte port |
| 0 | 1 | 16-bit | Word port |
| 0 | 0 | 32-bit | Longword port |

When the CPU requests a 32-bit transfer to a 16-bit port, the bus controller
automatically breaks it into two 16-bit transfers. For an 8-bit port, four
8-bit transfers. This is transparent to the instruction execution.

**Dynamic bus sizing cycle penalty examples:**

| Transfer | 32-bit port | 16-bit port | 8-bit port |
|----------|-------------|-------------|------------|
| Byte read/write | 1 cycle | 1 cycle | 1 cycle |
| Word read/write | 1 cycle | 1 cycle | 2 cycles |
| Long read/write | 1 cycle | 2 cycles | 4 cycles |

On the Amiga:
- Chip RAM is on a 16-bit port: longword accesses take 2 bus cycles
- Custom registers are on a 16-bit port: same penalty
- Fast RAM (on accelerators) is typically 32-bit: no penalty
- Zorro II expansion is 16-bit: longword penalty
- Zorro III expansion is 32-bit: no penalty

**Misalignment penalty (68020/68030):**
When a word or longword operand is not naturally aligned, additional bus
cycles are required:

| Transfer | Aligned | Misaligned across long boundary |
|----------|---------|-------------------------------|
| Word on 32-bit port | 1 cycle | 2 cycles |
| Long on 32-bit port | 1 cycle | 2 cycles |
| Long on 16-bit port | 2 cycles | 3 cycles |

The 68020/68030 handle misalignment transparently -- no exception is
generated (unlike SPARC or early ARM). The penalty is purely in extra
bus cycles.

### 2.7 Privilege Model Changes

**Status Register additions:**
- **T1/T0 (bits 15-14):** Two trace mode bits replace the 68000's single T
  bit. T1=1,T0=0 is trace-on-any-instruction (same as 68000). T1=0,T0=1 is
  trace-on-change-of-flow (only traces branches, jumps, returns, traps).
- **M (bit 12):** Master/Interrupt state. When M=1, A7 = MSP. When an
  interrupt occurs, the CPU sets M=0 (switching A7 to ISP) and saves a
  throwaway frame. `(MC68020UM §1.2)`

```
Status Register layout (68020):
  15  14  13  12  11  10  9   8   7   6   5   4   3   2   1   0
  T1  T0  S   M   0   I2  I1  I0  0   0   0   X   N   Z   V   C
```

### 2.8 Coprocessor Interface

The 68020 defines a general coprocessor interface that uses the F-line
instruction encoding space and memory-mapped coprocessor interface registers
(CIRs). Up to 8 coprocessors can be addressed (CpID 0-7). The interface
uses standard bus cycles to communicate between CPU and coprocessor -- no
special signals. `(MC68020UM §7.1)`

The coprocessor interface supports:
- Concurrent execution (MC68882 can compute while CPU runs integer code)
- cpSAVE/cpRESTORE for context switching coprocessor state
- Exception handling via take-exception primitives

CpID 001 is reserved for the MC68881/MC68882 FPU. The instruction encoding
for FPU instructions is described in the companion FPU reference.

### 2.9 Timing Differences from 68000

The 68020 is pipelined and significantly faster than the 68000 for most
instructions. Exact timing depends on cache hits, operand alignment, and
instruction overlap. `(MC68020UM §8)`

**Key timing improvements over 68000:**

| Instruction | 68000 (cycles) | 68020 (cycles, best case) | Notes |
|-------------|----------------|--------------------------|-------|
| `MOVE.L Dn,Dm` | 4 | 2 | Register-to-register |
| `ADD.L Dn,Dm` | 8 | 2 | Register arithmetic |
| `MULU.W` | 38-70 | 28 (avg) | Major improvement |
| `MULS.W` | 38-70 | 28 (avg) | Major improvement |
| `MULU.L` | N/A | 28-44 | New instruction |
| `DIVS.W` | 158 | 56 | Still slow |
| `DIVU.W` | 140 | 44 | Still slow |

**Important timing concepts for 68020:**

1. **Instruction overlap:** The 68020 can execute the next instruction while
   the bus controller completes a write from the previous instruction. This
   means ADD.L D4,D5 can have an attributed time of 0 clocks when
   overlapped with a preceding MOVE.L that is still writing. `(MC68020UM §8.1.4)`

2. **Cache hits eliminate prefetch delays:** When instructions are in cache,
   no bus cycles are needed for prefetch. Example: a 4-instruction sequence
   takes 16 clocks without cache but 12 clocks with cache. `(MC68020UM §8.1.5)`

3. **Operand misalignment penalty:** A misaligned 32-bit access across a
   longword boundary requires an extra bus cycle. `(MC68020UM §8.1.2)`

---

## 3. MC68030

### 3.1 Overview

The MC68030 extends the 68020 with an on-chip data cache and an on-chip MMU.
It is used in the Amiga A3000 and A4000/030. The instruction set is almost
identical to the 68020 minus CALLM/RTM, plus MMU instructions.
`(MC68030UM §1)`

| Feature          | Value                                      |
|------------------|--------------------------------------------|
| **Data bus**     | 32-bit                                     |
| **Address bus**  | 32-bit                                     |
| **Clock**        | 16, 20, 25, 33, 40, 50 MHz                |
| **Package**      | 128-pin PGA or 132-pin PQFP               |
| **Caches**       | 256-byte I-cache + 256-byte D-cache (both direct-mapped) |
| **MMU**          | On-chip (22-entry ATC)                     |
| **FPU**          | External (MC68881/MC68882)                 |
| **Amiga usage**  | A3000 (25 MHz), A4000/030 (25 MHz), many accelerators |

**Note:** We only have Part 1 of the MC68030 User's Manual, which covers
architecture, instruction set, caches, exception processing, and MMU. Part 2
(bus interface electrical specifications) is not available. Bus timing
information below is therefore limited.

### 3.2 Instruction Set Changes from 68020

| Change | Details |
|--------|---------|
| **Removed** | `CALLM`, `RTM` (module call instructions) |
| **Added** | `PFLUSH`, `PLOAD`, `PMOVE`, `PTEST` (MMU instructions, all supervisor) |

The CALLM/RTM instructions were dropped because they were complex to
implement and rarely used. They generate an unimplemented instruction
exception on the 68030, which can trap to a software emulation handler.

#### 3.2.1 MMU Instructions

| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `PFLUSH` | Flush entries from ATC (various forms) | Supervisor |
| `PLOAD`  | Load an ATC entry from translation tables | Supervisor |
| `PMOVE`  | Move to/from MMU registers (CRP, SRP, TC, TT0, TT1, MMUSR) | Supervisor |
| `PTEST`  | Test a logical address translation | Supervisor |

### 3.3 Addressing Modes

The 68030 supports the same 18 addressing modes as the 68020. No new modes
are added, and none are removed. The only change is the removal of CALLM/RTM
instructions, which does not affect addressing mode availability.

### 3.4 Caches

The 68030 adds a data cache alongside the instruction cache. Both are
256-byte, direct-mapped, with identical organisation. `(MC68030UM §6)`

| Property | I-Cache | D-Cache |
|----------|---------|---------|
| Size | 256 bytes | 256 bytes |
| Type | Instruction only | Data only |
| Organisation | Direct-mapped | Direct-mapped |
| Line size | 1 longword (4 bytes) | 1 longword (4 bytes) |
| Entries | 64 longwords | 64 longwords |
| Write policy | N/A | **Write-through only** |

The data cache is **write-through only** -- every write goes to memory
immediately. There is no copyback mode. This simplifies DMA coherency
because memory is always up-to-date.

**CACR (68030):**
```
Bit 31-14: Reserved
Bit 13:    WA  -- Write Allocate (D-cache: allocate on write miss)
Bit 12:    DBE -- Data Burst Enable
Bit 11:    CD  -- Clear Data Cache
Bit 10:    CED -- Clear Entry in Data Cache
Bit 9:     FD  -- Freeze Data Cache
Bit 8:     ED  -- Enable Data Cache
Bit 7-4:   Reserved
Bit 3:     IBE -- Instruction Burst Enable
Bit 2:     CI  -- Clear Instruction Cache
Bit 1:     CEI -- Clear Entry in Instruction Cache
Bit 0:     FI  -- Freeze Instruction Cache
Bit -1:    EI  -- Enable Instruction Cache
```

**Burst mode:** The 68030 supports burst fills for cache line fills. When
burst is enabled and the memory supports it, the CPU can fill an entire
cache line (4 longwords) in a single burst transfer. `(MC68030UM §6)`

### 3.4 MMU

The 68030's on-chip MMU is significant for Amiga emulation because tools
like Enforcer, MungWall, and SetPatch rely on it. `(MC68030UM §9)`

| Property | Value |
|----------|-------|
| ATC size | 22 entries (fully associative) |
| Page sizes | 256 bytes to 32 KB |
| Translation levels | Up to 4 levels of table lookup |
| Root pointers | CRP (CPU Root Pointer) and SRP (Supervisor Root Pointer) |
| Transparent translation | TT0, TT1 (maps address ranges 1:1) |

**Key MMU registers (accessed via PMOVE):**

| Register | Width | Description |
|----------|-------|-------------|
| TC | 32-bit | Translation Control: enables MMU, sets page size, table levels |
| TT0, TT1 | 32-bit | Transparent Translation: defines address ranges that bypass translation |
| CRP | 64-bit | CPU Root Pointer: base of translation table |
| SRP | 64-bit | Supervisor Root Pointer: alternate root for supervisor space |
| MMUSR | 16-bit | MMU Status Register: result of PTEST |

**Transparent translation** maps a contiguous block of logical addresses
directly to the same physical addresses, bypassing table lookup. This is
used for I/O space and ROM regions that should not be translated.

**For emulation:** The MMU is critical for Enforcer (memory protection tool).
You need to implement the full translation table walk, ATC, and PFLUSH.
The "Kickstart ROM in RAM" trick uses the MMU to map ROM addresses to
fast RAM copies for faster execution.

**CACR (68030) detailed bit layout:**
```
Bit 31:   Reserved      Bit 30:   Reserved
Bit 29:   Reserved      Bit 28:   Reserved
Bit 27:   Reserved      Bit 26:   Reserved
Bit 25:   Reserved      Bit 24:   Reserved
Bit 23:   Reserved      Bit 22:   Reserved
Bit 21:   Reserved      Bit 20:   Reserved
Bit 19:   Reserved      Bit 18:   Reserved
Bit 17:   Reserved      Bit 16:   Reserved
Bit 15:   Reserved      Bit 14:   Reserved
Bit 13:   WA            Bit 12:   DBE
Bit 11:   CD            Bit 10:   CED
Bit 9:    FD            Bit 8:    ED
Bit 7:    Reserved      Bit 6:    Reserved
Bit 5:    Reserved      Bit 4:    Reserved
Bit 3:    IBE           Bit 2:    CI
Bit 1:    CEI           Bit 0:    EI
```

| Bit | Name | Function | Read behaviour |
|-----|------|----------|----------------|
| 13 | WA | Write Allocate: on D-cache write miss, allocate a line | Reads current state |
| 12 | DBE | Data Burst Enable: enable burst fills for D-cache | Reads current state |
| 11 | CD | Clear Data Cache: invalidate all D-cache entries | Always reads 0 |
| 10 | CED | Clear Entry Data: invalidate D-cache entry at CAAR index | Always reads 0 |
| 9 | FD | Freeze Data Cache: prevent D-cache replacement on miss | Reads current state |
| 8 | ED | Enable Data Cache | Reads current state |
| 3 | IBE | Instruction Burst Enable: enable burst fills for I-cache | Reads current state |
| 2 | CI | Clear Instruction Cache: invalidate all I-cache entries | Always reads 0 |
| 1 | CEI | Clear Entry Instruction: invalidate I-cache entry at CAAR index | Always reads 0 |
| 0 | EI | Enable Instruction Cache | Reads current state |

**Write-through behaviour:** The 68030 data cache is strictly write-through.
Every CPU write goes to both the cache (if the address hits a valid line)
and to external memory simultaneously. This means:
- Memory is always up-to-date -- DMA reads always get current data
- CPU writes to addresses not in the cache are never cached (unless WA=1)
- There are no dirty lines and CPUSH-style operations are not needed

**Write-Allocate (WA bit):** When WA=1 and a write misses in the D-cache,
the CPU allocates a new cache line and writes the data into it. When WA=0,
write misses are not cached. For Amiga chip RAM, WA should typically be 0
because chip RAM is shared with DMA devices.

### 3.5 Exception Model

Same as 68020 with one addition:

| Format | Size (words) | Use |
|--------|-------------|-----|
| $7 | ? | MMU fault (ATC fault) |

The format $7 frame is used for MMU-related access faults. The frame
contains the faulting address and MMU status to enable the OS to perform
demand paging.

All other formats ($0, $1, $2, $9, $A, $B) are the same as the 68020.

### 3.6 MMU Table Walk (Detailed)

Understanding the 68030 MMU table walk is critical for implementing Enforcer
and virtual memory. `(MC68030UM §9)`

**Translation process:**
1. The logical address is split into fields defined by TC:
   ```
   | TIA bits | TIB bits | TIC bits | TID bits | Page Offset |
   ```
2. Starting from the root pointer (CRP or SRP depending on mode):
   - Use TIA bits as index into first-level table
   - Read descriptor at that entry
   - If valid, extract next table base address
   - Use TIB bits as index into second-level table
   - Continue for TIC, TID levels as configured
3. Final descriptor is a page descriptor containing:
   - Physical page address
   - Write protect (WP) bit
   - Used (U) and Modified (M) bits
   - Cache mode
4. Physical address = page base + page offset

**Descriptor formats:**

Invalid descriptor:
```
Bit 1-0: 00 (invalid)
```

Page descriptor (short):
```
Bit 31-8:  Page frame address
Bit 7:     Write protect
Bit 6:     Used
Bit 5:     Modified
Bit 4:     Cache mode
Bit 3:     Supervisor only
Bit 2:     Reserved
Bit 1-0:   01 (page descriptor)
```

Table descriptor (short):
```
Bit 31-4:  Table address
Bit 3:     Write protect (applies to all pages in subtree)
Bit 2:     Used
Bit 1-0:   10 (table descriptor, short)
```

**ATC (Address Translation Cache):** The 22-entry fully-associative ATC
caches recent translations. On ATC hit, translation completes in 0 extra
cycles. On ATC miss, a table walk is performed, which requires 1-4 memory
reads depending on the number of table levels configured.

**PFLUSH operations:**
- `PFLUSH #FC,#mask`: Flush ATC entries matching function code
- `PFLUSH #FC,#mask,(An)`: Flush specific logical address
- `PFLUSHA`: Flush all ATC entries

### 3.7 Bus Interface

Similar to 68020 with the addition of **burst mode** for cache line fills.
When burst is enabled, the CPU can request four consecutive longwords in
rapid succession. The memory system acknowledges the burst capability via
CBREQ/CBACK signals. `(MC68030UM §7)`

**Note:** Part 2 of the MC68030 manual (bus interface details) is not
available. Bus timing specifications are therefore not covered here.

### 3.7 Timing Differences

The 68030 has similar instruction timing to the 68020 but benefits from:
1. Data cache hits for operand reads (the 68020 has no data cache)
2. Burst fills reducing cache miss penalty
3. On-chip MMU being faster than external MMU

Most integer instructions have the same clock counts as the 68020 when the
cache is disabled. With both caches enabled, the 68030 is faster due to
fewer external bus cycles.

---

## 4. MC68040

### 4.1 Overview

The MC68040 is Motorola's third-generation 68000 family processor, integrating
an integer unit, FPU, MMU, and large caches on a single chip. It has a 6-stage
pipeline and is significantly faster than the 68030. The A4000/040 uses it.
`(MC68040UM §1)`

| Feature          | Value                                      |
|------------------|--------------------------------------------|
| **Data bus**     | 32-bit                                     |
| **Address bus**  | 32-bit                                     |
| **Clock**        | 25, 33, 40 MHz                             |
| **Package**      | 179-pin PGA or ceramic                     |
| **Caches**       | 4 KB I-cache + 4 KB D-cache (4-way set-associative) |
| **MMU**          | Dual on-chip (separate I and D ATCs, 64 entries each) |
| **FPU**          | On-chip (subset of 68881/68882; LC040 lacks FPU) |
| **Amiga usage**  | A4000/040 (25 MHz), accelerator boards     |

**Variants:**
- **MC68040**: Full FPU + MMU
- **MC68LC040**: MMU but no FPU (some A4000 models)
- **MC68EC040**: No FPU or MMU (access control unit replaces MMU)
- **MC68040V**: 3.3V static version of LC040

### 4.2 Instruction Set Changes from 68030

| Change | Details |
|--------|---------|
| **Removed** | `CALLM`, `RTM` (already removed in 68030) |
| **Added** | `CINV` (cache invalidate), `CPUSH` (cache push/invalidate), `MOVE16` (16-byte aligned block move) |
| **Changed** | PFLUSH simplified (only PFLUSHA and PFLUSH (An) remain) |
| **FPU subset** | Transcendental/logarithmic FPU instructions not in hardware |

#### 4.2.1 Cache Management Instructions

| Mnemonic | Description | Privilege |
|----------|-------------|-----------|
| `CINV`   | Invalidate cache lines (IC/DC/BC, line/page/all) | Supervisor |
| `CPUSH`  | Push and invalidate cache lines (IC/DC/BC, line/page/all) | Supervisor |
| `MOVE16` | Move 16-byte aligned block (memory-to-memory or Ax-to-memory) | User |

**CINV** invalidates cache lines without writing dirty data back to memory.
**CPUSH** writes dirty data back first, then invalidates. Both can operate on:
- A single line (specified by An)
- All lines in a page
- All lines in the cache

The scope is specified by bits in the instruction encoding:
- IC = instruction cache
- DC = data cache
- BC = both caches

**MOVE16** transfers a 16-byte (4-longword) aligned block in a single bus
transaction. It is used for efficient memory-to-memory copies and for
pushing cache lines. The source and destination addresses are aligned to
16-byte boundaries (A3-A0 are zeroed).

#### 4.2.2 Integrated FPU

The 68040 FPU implements a **subset** of the MC68881/68882 instruction set.
Basic arithmetic (FADD, FSUB, FMUL, FDIV, FSQRT, FMOVE, FCMP, FTST, etc.)
is in hardware. The following instruction groups require software emulation
via the 68040 Floating-Point Software Package (FPSP):

**Instructions missing from 68040 FPU hardware:**
```
Transcendentals:  FSIN, FCOS, FSINCOS, FTAN, FASIN, FACOS, FATAN, FATANH
Hyperbolics:      FSINH, FCOSH, FTANH
Exponentials:     FETOX, FETOXM1, FTWOTOX, FTENTOX
Logarithms:       FLOGN, FLOGNP1, FLOG10, FLOG2
Other:            FMOD, FREM, FSGLDIV, FSGLMUL
```

When an unimplemented FPU instruction is encountered, the 68040 generates
an F-line exception. The `68040.library` (part of AmigaOS) installs a handler
that emulates these instructions in software.

The 68040 also adds **single- and double-precision rounding modes** via new
instruction forms (FSADD, FDADD, FSSUB, FDSUB, etc.) that round the result
to single or double precision instead of the default extended precision.

**For emulation:** See the companion FPU reference for detailed coverage of
the 68040 FPU differences.

### 4.3 Pipeline

The 68040 has a 6-stage integer pipeline: `(MC68040UM §2.1)`

```
Instruction Fetch -> Decode -> EA Calculate -> EA Fetch -> Execute -> Write-Back
```

Conditional branches are optimised for the taken case -- both paths are
prefetched and decoded, minimising pipeline refill.

### 4.4 Caches

The 68040 dramatically improves cache performance over the 68030.
`(MC68040UM §4)`

| Property | I-Cache | D-Cache |
|----------|---------|---------|
| Size | 4 KB | 4 KB |
| Organisation | 4-way set-associative | 4-way set-associative |
| Sets | 64 | 64 |
| Lines per set | 4 | 4 |
| Line size | 16 bytes (4 longwords) | 16 bytes (4 longwords) |
| Write policy | N/A | Write-through OR **copyback** (per page) |
| Replacement | Pseudo-random | Pseudo-random |
| Total lines | 256 | 256 |

**Copyback mode:** The 68040 introduces copyback (write-back) caching for the
data cache, configurable on a per-page basis via the page descriptor's CM
(Cache Mode) field. In copyback mode, writes update only the cache and mark
the line as dirty. The dirty data is written to memory only when the line is
replaced or explicitly pushed (CPUSH). `(MC68040UM §4.7.2)`

**Data cache line states:**
- **Invalid**: Line contains no valid data
- **Valid**: Line matches memory (clean)
- **Dirty**: Line has been modified but not written to memory

**CACR (68040):** `(MC68040UM §2.2.2.5)`
```
Bit 31:    DE  -- Data Cache Enable
Bit 30:    Reserved
Bit 29:    Reserved
Bit 28:    Reserved
...
Bit 15:    IE  -- Instruction Cache Enable
Bit 14-0:  Reserved
```

The 68040 CACR is much simpler than the 68020/68030 -- it only has enable
bits. Cache clearing is done via CINV/CPUSH instructions instead of CACR
bits.

**Cache coherency with DMA:**

This is the most important cache consideration for Amiga emulation. The
68040's copyback data cache means that memory visible to DMA devices (chip
RAM) may be **out of date** if the CPU has written to a copyback-cached page
without pushing the data.

The AmigaOS solution:
1. Map chip RAM pages as write-through or cache-inhibited (not copyback)
2. Call `CacheClearU()` / `CacheClearE()` before initiating DMA from memory
   that was recently written by the CPU

`CacheClearU()` maps to CPUSH instructions that push all dirty cache lines
to memory.

### 4.5 MMU

The 68040 has dual, independent MMUs for instruction and data access.
`(MC68040UM §3)`

| Property | Value |
|----------|-------|
| ATCs | Separate I-ATC and D-ATC, 64 entries each (4-way set-associative) |
| Page sizes | 4 KB or 8 KB (fixed, set by TC) |
| Translation levels | 3-level table (URP/SRP -> table -> page descriptor) |
| Root pointers | URP (User Root Pointer), SRP (Supervisor Root Pointer) |
| Transparent translation | ITT0, ITT1 (instruction), DTT0, DTT1 (data) |

**Key differences from 68030 MMU:**
- Fixed page sizes (4 KB or 8 KB) instead of variable (256B-32KB)
- Simplified PFLUSH: only PFLUSHA (flush all) and PFLUSH (An) (flush by address)
- No PMOVE instruction -- use MOVEC to access MMU registers
- Separate transparent translation for instruction and data streams
- Physical tags in cache (no aliasing problems)

**MMU registers (accessed via MOVEC):**

| Register | Rc code | Description |
|----------|---------|-------------|
| URP | $806 | User Root Pointer |
| SRP | $807 | Supervisor Root Pointer |
| TC (TCR) | $003 | Translation Control Register |
| ITT0 | $004 | Instruction Transparent Translation 0 |
| ITT1 | $005 | Instruction Transparent Translation 1 |
| DTT0 | $006 | Data Transparent Translation 0 |
| DTT1 | $007 | Data Transparent Translation 1 |
| MMUSR | $805 | MMU Status Register |

### 4.5.1 68040 MMU Table Walk (Detailed)

The 68040 uses a fixed 3-level table structure (simpler than the 68030's
configurable 1-4 levels). `(MC68040UM §3)`

**Translation for 4 KB pages (TC.P=0):**
```
Logical Address:
| Root Index (7 bits) | Pointer Index (7 bits) | Page Index (6 bits) | Page Offset (12 bits) |
  Bits 31-25              Bits 24-18               Bits 17-12             Bits 11-0
```

**Translation for 8 KB pages (TC.P=1):**
```
Logical Address:
| Root Index (7 bits) | Pointer Index (7 bits) | Page Index (5 bits) | Page Offset (13 bits) |
  Bits 31-25              Bits 24-18               Bits 17-13             Bits 12-0
```

**Table walk process:**
1. Read root descriptor from URP/SRP + (root index * 4)
2. If valid, read pointer descriptor from pointer table + (pointer index * 4)
3. If valid, read page descriptor from page table + (page index * 4)
4. Page descriptor provides physical page base, cache mode, protection

**68040 Page Descriptor format:**
```
Bit 31-13/12: Physical page address (4KB: bits 31-12, 8KB: bits 31-13)
Bit 11/12:    Reserved
Bit 10:       U1 (User page attribute 1 -> UPA1 signal)
Bit 9:        U0 (User page attribute 0 -> UPA0 signal)
Bit 8:        S  (Supervisor only)
Bit 7:        CM1 (Cache mode bit 1)
Bit 6:        CM0 (Cache mode bit 0)
Bit 5:        M  (Modified -- set by hardware on write)
Bit 4:        U  (Used -- set by hardware on access)
Bit 3:        W  (Write protect)
Bit 2:        PDT1 (descriptor type)
Bit 1:        PDT0 (descriptor type)
Bit 0:        G  (Global -- not flushed by PFLUSHA)
```

**Cache Mode (CM) field:**
| CM1 | CM0 | Mode |
|-----|-----|------|
| 0 | 0 | Write-through (cacheable) |
| 0 | 1 | Copyback (cacheable) |
| 1 | 0 | Cache-inhibited, precise |
| 1 | 1 | Cache-inhibited, imprecise |

"Precise" means access errors are reported immediately. "Imprecise" means
write errors may be reported on a later instruction (the write is buffered).

**For Amiga emulation:** Chip RAM should use CM=00 (write-through) or CM=10
(cache-inhibited precise) to maintain coherency with DMA devices. Fast RAM
can safely use CM=01 (copyback) for maximum performance.

### 4.5.2 68040 Write-Back Mechanism

The 68040's pipeline can have up to three pending write operations at the
time of an exception. These writes must be completed by the exception handler
before normal execution can resume. The access error stack frame (format $7)
contains three write-back entries (WB3, WB2, WB1) with status, address, and
data fields. `(MC68040UM §8.4.6)`

**Write-back status word format (WB3S/WB2S/WB1S):**
```
Bit 7:    V   -- Valid (this write-back is valid and must be completed)
Bit 6-5:  SIZ -- Size (00=byte, 01=word, 10=long, 11=line)
Bit 4:    TT1 -- Transfer type bit 1
Bit 3:    TT0 -- Transfer type bit 0
Bit 2:    TM2 -- Transfer modifier bit 2
Bit 1:    TM1 -- Transfer modifier bit 1
Bit 0:    TM0 -- Transfer modifier bit 0
```

If V=1 for a write-back entry, the exception handler must:
1. Translate the write-back address (WBnA) using PTEST if necessary
2. Perform the write of WBnD to WBnA with the specified size and attributes
3. Handle any bus errors that occur during the write-back

The handler typically does this by using MOVES with SFC/DFC set to the
appropriate function code, or by directly accessing the physical address
if the translation is known.

### 4.6 Exception Model Changes

The 68040 simplifies exception processing compared to the 68020/68030.
`(MC68040UM §8)`

| Format | Size (words) | Use |
|--------|-------------|-----|
| $0 | 4 | Normal: interrupts, TRAP #n, format error, etc. |
| $1 | 4 | Throwaway (ISP->MSP transition) |
| $2 | 6 | Six-word: CHK, TRAPcc, trace, zero divide, etc. |
| $3 | 6 | FPU post-instruction exception |
| $4 | 8 | Floating-point unimplemented instruction / FP disabled (LC040/EC040) |
| $7 | 30 | Access error (bus error / MMU fault) |

The important change is the **access error stack frame (format $7)**, which
replaces the 68020's complex short/long bus fault frames ($A/$B) with a
single, cleaner format. `(MC68040UM §8.4.5)`

**Format $7: Access Error Stack Frame (30 words / 60 bytes)**
```
SP+$00  |  Status Register                      |
SP+$02  |  Program Counter (high)               |
SP+$04  |  Program Counter (low)                |
SP+$06  | 0111 | Vector Offset                  |
SP+$08  |  Effective Address (high)             |
SP+$0A  |  Effective Address (low)              |
SP+$0C  |  Special Status Word (SSW)            |
SP+$0E  |  Write-Back 3 Status (WB3S)           |
SP+$10  |  Write-Back 2 Status (WB2S)           |
SP+$12  |  Write-Back 1 Status (WB1S)           |
SP+$14  |  Fault Address (FA) (high)            |
SP+$16  |  Fault Address (FA) (low)             |
SP+$18  |  Write-Back 3 Address (WB3A) (high)   |
SP+$1A  |  Write-Back 3 Address (WB3A) (low)    |
SP+$1C  |  Write-Back 3 Data (WB3D) (high)      |
SP+$1E  |  Write-Back 3 Data (WB3D) (low)       |
SP+$20  |  Write-Back 2 Address (WB2A) (high)   |
SP+$22  |  Write-Back 2 Address (WB2A) (low)    |
SP+$24  |  Write-Back 2 Data (WB2D) (high)      |
SP+$26  |  Write-Back 2 Data (WB2D) (low)       |
SP+$28  |  Write-Back 1 Address (WB1A) (high)   |
SP+$2A  |  Write-Back 1 Address (WB1A) (low)    |
SP+$2C  |  Write-Back 1 Data/Push Data (high)   |
SP+$2E  |  Write-Back 1 Data/Push Data (low)    |
SP+$30  |  Write-Back 1 Data/Push Data (high)   |
SP+$32  |  Write-Back 1 Data/Push Data (low)    |
SP+$34  |  Write-Back 1 Data/Push Data (high)   |
SP+$36  |  Write-Back 1 Data/Push Data (low)    |
SP+$38  |  Write-Back 1 Data/Push Data (high)   |
SP+$3A  |  (end of 30-word frame)               |
```

The write-back fields are critical: the 68040's deep pipeline may have
pending writes that could not complete before the fault. The exception
handler must complete these writes before returning. The SSW tells the
handler what happened.

The 68040 can **always recover from access faults**, enabling true virtual
memory. This is a key improvement over the 68020/68030 where recovery from
mid-instruction bus errors is complex.

### 4.7 Bus Interface

The 68040 has a **synchronous** bus interface, a fundamental change from the
68020/68030's asynchronous bus. `(MC68040UM §7)`

| Property | 68020/68030 | 68040 |
|----------|-------------|-------|
| Bus protocol | Asynchronous (DSACK handshake) | Synchronous (TA acknowledge) |
| Dynamic bus sizing | Yes (8/16/32-bit ports) | **No** (32-bit only) |
| Burst mode | Optional | Mandatory for cache line fills |
| Signals | DSACK0/DSACK1, AS, DS | TA, TEA, TS, TIP |

**No dynamic bus sizing:** The 68040 only supports 32-bit ports natively.
Accessing 16-bit or 8-bit devices requires external logic (a "Buster" or
"Ramsey" chip in the Amiga A4000) to perform the bus width conversion.
This is why the A4000 needs the Buster chip to interface the 68040 with
chip RAM (16-bit). `(MC68040UM §7)`

### 4.8 Timing

The 68040's 6-stage pipeline executes most integer operations in 1-3 cycles.
`(MC68040UM §10)`

| Instruction | 68020 (cycles) | 68040 (cycles) | Notes |
|-------------|----------------|-----------------|-------|
| `MOVE.L Dn,Dm` | 2 | 1 | Pipeline flow-through |
| `ADD.L Dn,Dm` | 2 | 1 | Single-cycle |
| `MULU.L` | 28-44 | 2-3 | Hardware multiplier |
| `MULS.L` | 28-44 | 2-3 | Hardware multiplier |
| `DIVS.L` | 56-68 | 38-56 | Still multi-cycle |
| `DIVU.L` | 44-56 | 38-44 | Still multi-cycle |
| `MOVEM.L (save)` | varies | 3+n | n = number of registers |
| `LEA (d16,An)` | 4 | 1 | Pipeline optimised |

**CINV/CPUSH timing:** `(MC68040UM §10.3)`
- CINVA: 16 cycles (all lines)
- CPUSHA: 16 cycles + bus cycles for dirty lines

---

## 5. MC68060

### 5.1 Overview

The MC68060 is the final and fastest member of the 68000 family. It is a
superscalar processor with dual integer pipelines, branch prediction, and
deep pipelining. Never used in production Amigas but found on accelerator
cards (Blizzard 060, Apollo 060, Cyberstorm 060). `(MC68060UM §1)`

| Feature          | Value                                      |
|------------------|--------------------------------------------|
| **Data bus**     | 32-bit                                     |
| **Address bus**  | 32-bit                                     |
| **Clock**        | 50, 60, 66, 75 MHz                        |
| **Package**      | 206-pin PGA or QFP                         |
| **Transistors**  | 2.5 million                                |
| **Caches**       | 8 KB I-cache + 8 KB D-cache (4-way set-associative) |
| **MMU**          | Dual on-chip (separate I and D ATCs, 64 entries each) |
| **FPU**          | On-chip (further subset vs 68040; LC060 lacks FPU) |
| **Branch cache** | 256-entry, 4-way set-associative BTB       |
| **Store buffer** | 4-entry store buffer + 1-entry push buffer |
| **Amiga usage**  | Accelerator cards only (50-66 MHz)         |

**Variants:**
- **MC68060**: Full FPU + MMU
- **MC68LC060**: MMU but no FPU (pin-compatible)
- **MC68EC060**: No FPU or MMU

### 5.2 Instruction Set Changes from 68040

| Change | Details |
|--------|---------|
| **Removed** | `MOVEP` (Move Peripheral -- F-line trap to emulate) |
| **Removed** | `CAS2` (Compare and Swap 2 -- F-line trap to emulate) |
| **Removed** | Some `PFLUSH` variants (only `PFLUSHA`, `PFLUSH (An)`, `PFLUSHAN`, `PFLUSHN (An)` remain) |
| **Added** | `PLPA` (Physical Load Physical Address) |
| **Changed** | `CAS` with misaligned operands generates exception (must be emulated) |

**MOVEP removal:** The MOVEP instruction (used to access Motorola 6800-style
peripherals) is removed. When executed, it generates an unimplemented
instruction exception (vector 61). The `68060.library` installs a handler
that emulates MOVEP in software. `(MC68060UM §1)`

**CAS2 removal:** CAS2 generates an unimplemented instruction exception and
must be emulated in software. The `68060.library` handles this. Single CAS
with aligned operands works in hardware; CAS with misaligned operands
generates an exception. `(MC68060UM §7.7.6)`

**New exception vectors:**

| Vector | Description |
|--------|-------------|
| 60 | Unimplemented Effective Address |
| 61 | Unimplemented Integer Instruction |

These are new on the 68060. Vector 60 is generated for addressing modes
that the 68060 does not support in hardware (e.g., certain complex memory
indirect modes in some instruction contexts). Vector 61 is for MOVEP and
other removed instructions.

### 5.3 Pipeline Architecture

The 68060 has a decoupled fetch-execute architecture: `(MC68060UM §1.3)`

**Instruction Fetch Pipeline (IFP) -- 4 stages:**
```
IA Calculate -> IC Fetch -> Early Decode -> IED Buffer
```

**Operand Execution Pipeline (OEP) -- 4 stages per pipe:**
```
DS (Dispatch/Decode) -> AG (Address Generate) -> OC (Operand Cache) -> EX (Execute)
```

The 68060 has **two** OEPs that operate in lockstep:
- **pOEP** (primary): Can execute any instruction
- **sOEP** (secondary): Can execute "standard" single-cycle instructions

When both pipes are active, two instructions execute per clock cycle
(superscalar). The 96-byte FIFO instruction buffer decouples the IFP from
the OEPs.

### 5.4 Branch Cache

The 68060 has a 256-entry, 4-way set-associative branch target buffer (BTB)
that predicts branch direction based on past execution history.
`(MC68060UM §5.11)`

**Branch folding:** The branch cache allows the IFP to detect and redirect
the instruction stream before the branch reaches the execution engines.
Most predicted-taken branches execute in **zero cycles** from the OEP
perspective -- the IFP absorbs the cost. `(MC68060UM §1)`

**For emulation:** The branch cache affects timing only, not functional
correctness. But it is the key to the 68060's performance -- without branch
prediction modelling, cycle-accurate emulation is impossible.

### 5.5 Caches

| Property | I-Cache | D-Cache |
|----------|---------|---------|
| Size | 8 KB | 8 KB |
| Organisation | 4-way set-associative | 4-way set-associative |
| Sets | 64 | 64 |
| Lines per set | 4 | 4 |
| Line size | 16 bytes (4 longwords) | 16 bytes (4 longwords) |
| Write policy | N/A | Write-through or copyback (per page) |
| Replacement | Pseudo-random | Pseudo-random |
| Banking | -- | 4-way banked for simultaneous R/W |
| Total lines | 256 | 256 |

The data cache is **4-way banked**, allowing simultaneous read and write
access each clock cycle. `(MC68060UM §1)`

**Store buffer:** 4-entry store buffer + 1-entry push buffer decouple the
pipeline from memory writes. The store buffer allows the pipeline to
continue executing while writes are pending. `(MC68060UM §5.9)`

**CACR (68060):** `(MC68060UM §5.2)`
```
Bit 31:    EDC  -- Enable Data Cache
Bit 30:    NAD  -- No Allocate Mode (D-cache)
Bit 29:    ESB  -- Enable Store Buffer
Bit 28:    DPI  -- Disable CPUSH Invalidation
Bit 27:    FOC  -- 1/2-Cache Operation Mode (D-cache)
Bit 26:    Reserved
...
Bit 23:    EBC  -- Enable Branch Cache
Bit 22:    CABC -- Clear All Branch Cache entries
Bit 21:    CUBC -- Clear User Branch Cache entries
Bit 20-16: Reserved
Bit 15:    EIC  -- Enable Instruction Cache
Bit 14:    NAI  -- No Allocate Mode (I-cache)
Bit 13:    FIC  -- 1/2-Cache Operation Mode (I-cache)
Bit 12-0:  Reserved
```

### 5.6 MMU

Similar to the 68040 MMU with dual ATCs. `(MC68060UM §4)`

| Property | Value |
|----------|-------|
| ATCs | Separate I-ATC and D-ATC, 64 entries each (4-way set-associative) |
| Page sizes | 4 KB or 8 KB |
| Root pointers | URP, SRP |
| Transparent translation | ITT0, ITT1, DTT0, DTT1 |
| PFLUSH variants | PFLUSHA, PFLUSH (An), PFLUSHAN, PFLUSHN (An) |

### 5.7 Exception Model

The 68060 uses a **restart exception processing model**. Exceptions are
detected at the execution stage and force later instructions to be aborted.
`(MC68060UM §8.1)`

| Format | Size (words) | Use |
|--------|-------------|-----|
| $0 | 4 | Normal: interrupts, TRAP #n, illegal instruction, privilege violation, etc. |
| $2 | 6 | Six-word: CHK, TRAPcc, trace, zero divide, address error, etc. |
| $3 | 6 | FPU post-instruction exception |
| $4 | 8 | Access error / FPU unimplemented / FP disabled |

**Format $4: Access Error Stack Frame (8 words / 16 bytes)**
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0100 | Vector Offset             |
SP+$08  |  Fault Address (high)            |
SP+$0A  |  Fault Address (low)             |
SP+$0C  |  Fault Status Long Word (FSLW) (high) |
SP+$0E  |  Fault Status Long Word (FSLW) (low)  |
```

The format $4 frame is much smaller than the 68040's format $7 (8 words vs
30 words) because the 68060's pipeline is simpler to restart -- there are
no pending write-backs to save.

**FSLW (Fault Status Long Word):** Contains detailed information about the
fault including:
- Fault type (instruction/data, read/write)
- Cache mode of the page
- Lock status
- Push buffer / store buffer fault indicators
- Branch prediction error (BPE) bit
- Whether the access was precise or imprecise

**Unique 68060 behaviour:** If an interrupt is pending during exception
processing, the interrupt is deferred until the first instruction of the
current exception handler executes. This guarantees that an exception
handler can mask interrupts with its first instruction (e.g., writing to SR).
`(MC68060UM §8.1)`

### 5.8 Bus Interface

Similar to 68040 -- synchronous, 32-bit only, burst mode for cache fills.
`(MC68060UM §7)`

**BUSCR (Bus Control Register):** The 68060 adds a Bus Control Register
(accessible via MOVEC) that provides control over bus behaviour.

### 5.9 Privilege Model Additions

**PCR (Processor Configuration Register):** `(MC68060UM §11.1.2.1.1)`

A new control register that provides:
- Processor identification (revision, etc.)
- Enable/disable superscalar dispatch (bit 0)
- Enable/disable FPU (bit 1)

### 5.10 Timing

The 68060 achieves sub-cycle throughput for many instructions through
superscalar dispatch. Timing assumes cache hits and no pipeline stalls.
`(MC68060UM §10)`

| Instruction | 68040 (cycles) | 68060 (cycles) | Notes |
|-------------|----------------|-----------------|-------|
| `MOVE.L Dn,Dm` | 1 | 1 (0.5 dual-issue) | Can dual-issue |
| `ADD.L Dn,Dm` | 1 | 1 (0.5 dual-issue) | Can dual-issue |
| `MULU.L` | 2-3 | 2 | Hardware multiplier |
| `MULS.L` | 2-3 | 2 | Hardware multiplier |
| `DIVS.L` | 38-56 | 38 | Still multi-cycle |
| `DIVU.L` | 38-44 | 38 | Still multi-cycle |
| `MOVEM.L` | 3+n | 3+n | **Serialises pipeline** |
| `Bcc (predicted taken)` | 2-3 | **0** | Branch folding |
| `Bcc (mispredicted)` | N/A | 8-10 | Pipeline flush |

**Change/use penalties:** When a register is modified and the next instruction
uses it as an address base, a 2-cycle stall may occur. For index registers,
the penalty can be 2-3 cycles depending on the scale factor.
`(MC68060UM §10.2)`

**Optimised sequences:** The following instructions produce their destination
register with no change/use penalty for subsequent instructions:
`LEA`, `MOVE.L #imm,Rn`, `MOVEQ`, `CLR.L Dn`, `any op (An)+`, `any op -(An)`
`(MC68060UM §10.2)`

---

## 6. Amiga-Specific CPU Quirks

This section covers behaviours that are specific to the Amiga hardware
environment and matter for emulation regardless of which CPU is being
emulated.

### 6.1 MOVE from SR Privilege Violation (68010+)

**The problem:** On the 68000, `MOVE SR,<ea>` is a user-mode instruction. On
the 68010 and all later CPUs, it is supervisor-only and generates a privilege
violation exception when executed in user mode.

**The fix:** Kickstart 2.0+ installs a privilege violation handler that checks
if the faulting instruction is `MOVE SR,<ea>`. If so, it emulates the
instruction using `MOVE CCR,<ea>` (which only provides the user-visible
condition codes, not the system byte) and returns via RTE.

**For emulation:** If you emulate a 68010+ CPU running Kickstart 1.x code
(which does not have this handler), you must either:
1. Patch the ROM to add the handler, or
2. Handle the privilege violation in your emulator and emulate MOVE CCR
   instead

### 6.2 TAS on Chip RAM (All CPUs)

The TAS (Test And Set) instruction uses an indivisible read-modify-write bus
cycle. On the Amiga, this bus cycle conflicts with Agnus/Alice's DMA
arbitration because Agnus does not properly handle the continuous bus
assertion required by TAS. The result is that TAS writes may be lost when
targeting chip RAM.

**This is not a CPU-specific issue** -- it affects all CPUs in Amiga hardware
because it is caused by the chipset's bus controller, not the CPU.

**For emulation:** Implement TAS correctly for fast RAM but make the write
portion a no-op for chip RAM addresses ($000000-$1FFFFF on OCS/ECS,
$000000-$1FFFFF on AGA).

### 6.3 68040 Copyback Cache Coherency with DMA

When the 68040/68060 data cache is in copyback mode for a memory region
that DMA devices (blitter, disk, audio) also access, stale data problems
arise:

1. **CPU writes to copyback-cached chip RAM:** The data sits in the CPU
   cache but has not been written to physical memory. If the blitter then
   reads that address, it gets stale data from memory.

2. **DMA writes to chip RAM:** The CPU cache still holds the old data. If
   the CPU reads the address, it gets the stale cached copy.

**AmigaOS solution:**
- Map chip RAM as write-through or cache-inhibited (not copyback) using the
  MMU page descriptors.
- Call `CacheClearU()` before DMA operations that depend on CPU-written data.
- After DMA completes, `CacheClearE()` can invalidate specific cache ranges.

**CacheControl() CACR mapping:**

| AmigaOS bit | CACR effect | CPU |
|-------------|-------------|-----|
| `CACRF_EnableI` | Enable I-cache | 68020+ |
| `CACRF_EnableD` | Enable D-cache | 68030+ |
| `CACRF_ClearI` | Clear I-cache | 68020+ |
| `CACRF_ClearD` | Clear D-cache | 68030+ |
| `CACRF_IBE` | I-cache burst enable | 68030 |
| `CACRF_DBE` | D-cache burst enable | 68030 |
| `CACRF_WriteAllocate` | Write-allocate mode | 68030 |
| `CACRF_CopyBack` | Enable copyback mode | 68040+ |
| `CACRF_EnableE` | Enable branch cache | 68060 |
| `CACRF_EnableSB` | Enable store buffer | 68060 |

### 6.4 68060 MOVEP Emulation

MOVEP is removed from the 68060 and generates an unimplemented integer
instruction exception (vector 61). The `68060.library` installs a handler
that decodes the MOVEP instruction from the exception stack frame and
emulates it using individual byte accesses to the even or odd addresses.

**For emulation:** If running 68060 mode, trap MOVEP and emulate it.
Alternatively, implement MOVEP in the CPU core and skip the trap.

### 6.5 Kickstart ROM in Fast RAM (68030+)

On 68030+ Amigas, the MMU can be programmed to map the Kickstart ROM
address range ($F80000-$FFFFFF or $FC0000-$FFFFFF) to a fast RAM copy.
This provides a significant speed improvement because:
1. ROM is typically on a slow bus (chip bus or 16-bit expansion bus)
2. Fast RAM is on the CPU's local 32-bit bus with no wait states

The AmigaOS performs this copy during boot:
1. Copy ROM contents to fast RAM
2. Program MMU to map ROM logical addresses to the fast RAM physical addresses
3. All subsequent ROM reads come from fast RAM

### 6.6 68040/68060 Software Packages (FPSP/ISP/M68060SP)

The 68040 and 68060 require software packages to handle instructions that
are not implemented in hardware. These packages are installed by the
respective `.library` files in AmigaOS.

**68040 FPSP (Floating-Point Software Package):**
- Handles F-line exceptions for unimplemented FPU instructions
- Emulates: FSIN, FCOS, FSINCOS, FTAN, FASIN, FACOS, FATAN, FATANH,
  FSINH, FCOSH, FTANH, FETOX, FETOXM1, FTWOTOX, FTENTOX, FLOGN,
  FLOGNP1, FLOG10, FLOG2, FMOD, FREM, FSGLDIV, FSGLMUL, FMOVECR
- Installed by `68040.library` during AmigaOS boot
- Penalty: each emulated instruction takes 100-1000+ cycles (exception
  entry, decode, software computation, exception return)

**68060 ISP (Integer Software Package):**
- Handles vector 61 (unimplemented integer instruction) for:
  - MOVEP (all forms)
  - CAS2 (all forms)
  - CAS with misaligned operand
- Handles vector 60 (unimplemented effective address) for certain complex
  addressing mode / instruction combinations
- Installed by `68060.library`

**68060 FPSP (Floating-Point Software Package):**
- Same as 68040 FPSP but also handles:
  - FSCALE, FGETEXP, FGETMAN (which the 68040 implements in hardware but
    the 68060 does not)
  - FMOVECR
- Installed by `68060.library`

**For emulation:** You have two choices:
1. **Implement all instructions in your CPU core** -- this is the correct
   approach for performance. The software packages add significant overhead.
2. **Generate the exceptions and let the ROM handle them** -- this works but
   is slow for FPU-heavy code. You must correctly implement the exception
   stack frame formats.

Most production emulators (WinUAE, vAmiga, FS-UAE) implement all instructions
in the CPU core and skip the software package entirely.

### 6.7 CPU Clock vs Chip Clock Asynchrony

On the A3000 and A4000, the CPU runs at a different clock rate than the
chipset. The chipset always runs at ~7.09 MHz (PAL) or ~7.16 MHz (NTSC).
The CPU runs at 25 MHz (A3000/A4000-030) or 25-75 MHz (accelerators).

This means:
- Chip RAM accesses must be synchronised to the chip clock, adding wait
  states from the CPU's perspective.
- Custom register accesses are always at chip speed.
- Fast RAM accesses run at full CPU speed.

**For emulation:** You need to model the wait states when the CPU accesses
chip RAM or custom registers. The exact number of wait states depends on
the phase relationship between the CPU clock and the chip clock at the time
of the access.

---

## 7. MOVEC Register Reference

The MOVEC instruction moves data between a general register and a control
register. It is supervisor-only on all CPUs. The control register is
identified by a 12-bit register code (Rc). `(MC68020UM §1.2)`

### 7.1 Complete MOVEC Register Table

| Rc (hex) | Register | Width | 68010 | 68020 | 68030 | 68040 | 68060 | Description |
|----------|----------|-------|-------|-------|-------|-------|-------|-------------|
| $000 | SFC | 3-bit | Yes | Yes | Yes | Yes | Yes | Source Function Code |
| $001 | DFC | 3-bit | Yes | Yes | Yes | Yes | Yes | Destination Function Code |
| $002 | CACR | 32-bit | -- | Yes | Yes | Yes | Yes | Cache Control Register |
| $003 | TC (TCR) | 32-bit | -- | -- | Yes* | Yes | Yes | Translation Control Register |
| $004 | ITT0 | 32-bit | -- | -- | TT0* | Yes | Yes | Instruction Transparent Translation 0 |
| $005 | ITT1 | 32-bit | -- | -- | TT1* | Yes | Yes | Instruction Transparent Translation 1 |
| $006 | DTT0 | 32-bit | -- | -- | -- | Yes | Yes | Data Transparent Translation 0 |
| $007 | DTT1 | 32-bit | -- | -- | -- | Yes | Yes | Data Transparent Translation 1 |
| $008 | BUSCR | 32-bit | -- | -- | -- | -- | Yes | Bus Control Register |
| $800 | USP | 32-bit | Yes | Yes | Yes | Yes | Yes | User Stack Pointer |
| $801 | VBR | 32-bit | Yes | Yes | Yes | Yes | Yes | Vector Base Register |
| $802 | CAAR | 32-bit | -- | Yes | Yes | -- | -- | Cache Address Register |
| $803 | MSP | 32-bit | -- | Yes | Yes | Yes | Yes | Master Stack Pointer |
| $804 | ISP | 32-bit | -- | Yes | Yes | Yes | Yes | Interrupt Stack Pointer |
| $805 | MMUSR | 16-bit | -- | -- | --* | Yes | Yes | MMU Status Register |
| $806 | URP | 32-bit | -- | -- | -- | Yes | Yes | User Root Pointer |
| $807 | SRP | 32-bit | -- | -- | -- | Yes | Yes | Supervisor Root Pointer |
| $808 | PCR | 32-bit | -- | -- | -- | -- | Yes | Processor Configuration Register |

**Notes:**
- 68030 TC, TT0, TT1, and MMUSR are accessed via PMOVE, not MOVEC (marked *).
  On the 68040+, they are accessed via MOVEC.
- 68020 has CAAR for cache entry clear. 68030 also has CAAR. 68040+ do not
  (they use CINV/CPUSH instead).
- 68060 PCR provides processor identification and FPU/superscalar enable bits.

### 7.2 CACR Bit Layouts by CPU

**68020 CACR:**
```
Bit 3: C  (Clear Cache)       Bit 2: CE (Clear Entry)
Bit 1: F  (Freeze Cache)      Bit 0: E  (Enable Cache)
```

**68030 CACR:**
```
Bit 13: WA (Write Allocate)    Bit 12: DBE (Data Burst Enable)
Bit 11: CD (Clear D-cache)    Bit 10: CED (Clear D Entry)
Bit 9:  FD (Freeze D-cache)   Bit 8:  ED  (Enable D-cache)
Bit 3:  IBE (I Burst Enable)  Bit 2:  CI  (Clear I-cache)
Bit 1:  CEI (Clear I Entry)   Bit 0:  EI  (Enable I-cache)
```

**68040 CACR:**
```
Bit 31: DE (Enable D-cache)   Bit 15: IE (Enable I-cache)
All other bits reserved.
```

**68060 CACR:**
```
Bit 31: EDC (Enable D-cache)  Bit 30: NAD (No Alloc D)
Bit 29: ESB (Enable Store Buffer)  Bit 28: DPI (Disable CPUSH Invalidation)
Bit 27: FOC (1/2-cache D)     Bit 23: EBC (Enable Branch Cache)
Bit 22: CABC (Clear All BC)   Bit 21: CUBC (Clear User BC)
Bit 15: EIC (Enable I-cache)  Bit 14: NAI (No Alloc I)
Bit 13: FIC (1/2-cache I)
```

---

## 8. Instruction Encoding: Full Extension Words (68020+)

The 68020 introduces the "full extension word" format for addressing modes.
This is distinct from the "brief extension word" used by the 68000/68010.
An emulator must distinguish between the two formats.

### 8.1 Brief Extension Word (68000-compatible)

Used when bit 8 of the extension word is 0.

```
Bit 15:    D/A (0=Dn, 1=An for index register)
Bit 14-12: Register number (index register)
Bit 11:    W/L (0=sign-extended word index, 1=long-word index)
Bit 10-9:  Scale (00=1, 01=2, 10=4, 11=8) -- 68000 always 00
Bit 8:     0 (identifies brief format)
Bit 7-0:   Displacement (sign-extended 8-bit)
```

**68000 compatibility note:** On the 68000/68010, bits 10-9 (scale) are
ignored (treated as 00 = scale 1). The 68020+ honours the scale field.
Bit 8 is always 0 on 68000 instructions, so there is no ambiguity.

### 8.2 Full Extension Word (68020+)

Used when bit 8 of the extension word is 1.

```
Bit 15:    D/A (0=Dn, 1=An for index register)
Bit 14-12: Register number (index register)
Bit 11:    W/L (0=sign-extended word index, 1=long-word index)
Bit 10-9:  Scale (00=1, 01=2, 10=4, 11=8)
Bit 8:     1 (identifies full format)
Bit 7:     BS (Base Register Suppress: 1=suppress An)
Bit 6:     IS (Index Suppress: 1=suppress Xn)
Bit 5-4:   BD Size (00=reserved, 01=null, 10=word, 11=long)
Bit 3:     0 (reserved)
Bit 2-0:   I/IS (Index/Indirect Selection)
```

**I/IS field encoding:**

| I/IS | IS=0 (index not suppressed) | IS=1 (index suppressed) |
|------|---------------------------|-------------------------|
| 000  | No memory indirect | No memory indirect |
| 001  | Indirect preindexed, null od | Indirect, null od |
| 010  | Indirect preindexed, word od | Indirect, word od |
| 011  | Indirect preindexed, long od | Indirect, long od |
| 100  | Reserved | Reserved |
| 101  | Indirect postindexed, null od | Indirect, null od |
| 110  | Indirect postindexed, word od | Indirect, word od |
| 111  | Indirect postindexed, long od | Indirect, long od |

**BD Size determines additional extension words:**
- 01: No base displacement (null)
- 10: 16-bit base displacement (1 extension word)
- 11: 32-bit base displacement (2 extension words)

**od (outer displacement) size is encoded in I/IS:**
- null: No outer displacement
- word: 16-bit outer displacement (1 extension word)
- long: 32-bit outer displacement (2 extension words)

**Memory indirect operation:**
1. Compute intermediate address = Base + Base Displacement (+ Index if preindexed)
2. Read longword from intermediate address (the indirection)
3. Add outer displacement (+ Index if postindexed)
4. Result is the effective address

### 8.3 How to Distinguish Brief from Full

Read bit 8 of the first extension word:
- Bit 8 = 0: Brief format (68000-compatible)
- Bit 8 = 1: Full format (68020+ only)

On a 68000/68010, bit 8 of the extension word is always 0, so there is
never ambiguity when running 68000 code on a 68020+.

---

## 9. Superscalar Considerations (68060)

The 68060's superscalar architecture is the most complex aspect of its
design for emulation purposes. This section covers the dispatch rules that
determine which instruction pairs can execute simultaneously.

### 9.1 Instruction Classification

Every M68000 instruction falls into one of five classes for 68060 dispatch:
`(MC68060UM §10.1.2)`

| Class | Can execute in sOEP? | Description |
|-------|---------------------|-------------|
| `pOEP\|sOEP` | Yes | Standard single-cycle instructions. Can run in either pipe. |
| `pOEP-only` | No | Must execute in pOEP. Multi-cycle or complex instructions. |
| `pOEP-until-last` | On last cycle only | Multi-operation instructions that allow sOEP dispatch during their last cycle. |
| `pOEP-but-allows-sOEP` | sOEP instruction can be dispatched | Must execute in pOEP but allows a `pOEP\|sOEP` instruction in sOEP simultaneously. |

**Dual-issue rule:** Two instructions can dispatch simultaneously only if:
1. The pOEP instruction is `pOEP|sOEP`, `pOEP-until-last` (on last cycle), or `pOEP-but-allows-sOEP`
2. The sOEP instruction is `pOEP|sOEP`
3. All six dispatch tests pass (see below)

### 9.2 Six Dispatch Tests

For a pair of instructions to dual-issue, ALL six tests must pass:
`(MC68060UM §10.1)`

1. **Test 1: Valid opword/extensions.** The sOEP instruction's opword and
   all required extension words must be available in the instruction buffer.

2. **Test 2: Instruction classification.** Both instructions must have
   compatible classes (see table above).

3. **Test 3: Addressing mode.** The sOEP instruction must not use:
   - Address register indirect with index + base displacement `(bd,An,Xi*SF)`
   - Any PC-relative mode `(d16,PC)`, `(d8,PC,Xi*SF)`, `(bd,PC,Xi*SF)`

4. **Test 4: Single memory access.** At most one of the two instructions can
   make a data memory access (the data cache has one port per cycle).

5. **Test 5: No AGU register conflicts.** The sOEP's base or index register
   must not conflict with the pOEP's address or execute result.

6. **Test 6: No IEE register conflicts.** The sOEP's source operand
   registers must not conflict with the pOEP's execute result.
   **Exception:** `MOVE.L <ea>,Rn` followed by an instruction using Rn
   succeeds due to hardware bypass. `(MC68060UM §10.1.6)`

### 9.3 Complete Superscalar Classification

The following tables reproduce the full 68060 instruction classification
from the MC68060UM. This is essential for accurate cycle counting.
`(MC68060UM §10.1.2, Tables 10-2 through 10-4)`

#### 9.3.1 Integer Instructions

| Mnemonic | OEP Class | Notes |
|----------|-----------|-------|
| `ABCD` | pOEP-only | |
| `ADD` | pOEP\|sOEP | All forms |
| `ADDA` | pOEP\|sOEP | |
| `ADDI Dx` | pOEP\|sOEP | Register destination |
| `ADDI -(Ax)+` | pOEP\|sOEP | Predec/postinc destination |
| `ADDI (other)` | pOEP-until-last | Other EA destinations |
| `ADDQ` | pOEP\|sOEP | |
| `ADDX` | pOEP-only | |
| `AND` | pOEP\|sOEP | |
| `ANDI Dx` | pOEP\|sOEP | |
| `ANDI -(Ax)+` | pOEP\|sOEP | |
| `ANDI (other)` | pOEP-until-last | |
| `ANDI to CCR` | pOEP-only | |
| `ASL` | pOEP\|sOEP | |
| `ASR` | pOEP\|sOEP | |
| `Bcc` | pOEP-only | Or pOEP-but-allows-sOEP if not predicted taken |
| `BCHG Dy,` | pOEP-only | |
| `BCHG #imm,` | pOEP-until-last | |
| `BCLR Dy,` | pOEP-only | |
| `BCLR #imm,` | pOEP-until-last | |
| `BFCHG` | pOEP-only | |
| `BFCLR` | pOEP-only | |
| `BFEXTS` | pOEP-only | |
| `BFEXTU` | pOEP-only | |
| `BFFFO` | pOEP-only | |
| `BFINS` | pOEP-only | |
| `BFSET` | pOEP-only | |
| `BFTST` | pOEP-only | |
| `BKPT` | pOEP-only | |
| `BRA` | pOEP-only | Folded by branch cache |
| `BSET Dy,` | pOEP-only | |
| `BSET #imm,` | pOEP-until-last | |
| `BSR` | pOEP-only | |
| `BTST Dy,` | pOEP-only | |
| `BTST #imm,` | pOEP-until-last | |
| `CAS` | pOEP-only | Aligned only; misaligned traps |
| `CHK` | pOEP-only | |
| `CLR` | pOEP\|sOEP | |
| `CMP` | pOEP\|sOEP | |
| `CMPA` | pOEP\|sOEP | |
| `CMPI Dx` | pOEP\|sOEP | |
| `CMPI -(Ax)+` | pOEP\|sOEP | |
| `CMPI (other)` | pOEP-until-last | |
| `CMPM` | pOEP-until-last | |
| `DBcc` | pOEP-only | |
| `DIVS.L` | pOEP-only | 38 cycles |
| `DIVS.W` | pOEP-only | 38 cycles |
| `DIVU.L` | pOEP-only | 38 cycles |
| `DIVU.W` | pOEP-only | 38 cycles |
| `EOR` | pOEP\|sOEP | |
| `EORI Dx` | pOEP\|sOEP | |
| `EORI -(Ax)+` | pOEP\|sOEP | |
| `EORI (other)` | pOEP-until-last | |
| `EORI to CCR` | pOEP-only | |
| `EXG` | pOEP-only | |
| `EXT` | pOEP\|sOEP | |
| `EXTB.L` | pOEP\|sOEP | |
| `ILLEGAL` | pOEP\|sOEP | |
| `JMP` | pOEP-only | Folded by branch cache |
| `JSR` | pOEP-only | |
| `LEA` | pOEP\|sOEP | |
| `LINK` | pOEP-until-last | |
| `LSL` | pOEP\|sOEP | |
| `LSR` | pOEP\|sOEP | |
| `MOVE Rx` | pOEP\|sOEP | Register source or destination |
| `MOVE Ry,` | pOEP\|sOEP | |
| `MOVE <mem>,<mem>` | pOEP-until-last | Memory to memory |
| `MOVE #imm,<mem>` | pOEP-until-last | Immediate to memory |
| `MOVEA` | pOEP\|sOEP | |
| `MOVE from CCR` | pOEP-only | |
| `MOVE to CCR` | pOEP\|sOEP | |
| `MOVE16` | pOEP-only | |
| `MOVEM` | pOEP-only | Serialises pipeline |
| `MOVEQ` | pOEP\|sOEP | |
| `MULS.L` | pOEP-only | 2 cycles |
| `MULS.W` | pOEP-only | 2 cycles |
| `MULU.L` | pOEP-only | 2 cycles |
| `MULU.W` | pOEP-only | 2 cycles |
| `NBCD` | pOEP-only | |
| `NEG` | pOEP\|sOEP | |
| `NEGX` | pOEP-only | |
| `NOP` | pOEP-only | Pipeline sync |
| `NOT` | pOEP\|sOEP | |
| `OR` | pOEP\|sOEP | |
| `ORI Dx` | pOEP\|sOEP | |
| `ORI -(Ax)+` | pOEP\|sOEP | |
| `ORI (other)` | pOEP-until-last | |
| `ORI to CCR` | pOEP-only | |
| `PACK` | pOEP-only | |
| `PEA` | pOEP-only | |
| `ROL` | pOEP\|sOEP | |
| `ROR` | pOEP\|sOEP | |
| `ROXL` | pOEP-only | |
| `ROXR` | pOEP-only | |
| `RTD` | pOEP-only | |
| `RTR` | pOEP-only | |
| `RTS` | pOEP-only | |
| `SBCD` | pOEP-only | |
| `Scc` | pOEP-but-allows-sOEP | |
| `SUB` | pOEP\|sOEP | |
| `SUBA` | pOEP\|sOEP | |
| `SUBI Dx` | pOEP\|sOEP | |
| `SUBI -(Ax)+` | pOEP\|sOEP | |
| `SUBI (other)` | pOEP-until-last | |
| `SUBQ` | pOEP\|sOEP | |
| `SUBX` | pOEP-only | |
| `SWAP` | pOEP-only | |
| `TAS` | pOEP-only | |
| `TRAP` | pOEP\|sOEP | |
| `TRAPF` | pOEP\|sOEP | |
| `TRAPcc (other)` | pOEP-only | |
| `TRAPV` | pOEP-only | |
| `TST` | pOEP\|sOEP | |
| `UNLK` | pOEP-only | |
| `UNPK` | pOEP-only | |

#### 9.3.2 Privileged Instructions

All privileged instructions are **pOEP-only**:

```
ANDI to SR, CINV, CPUSH, EORI to SR, MOVE from SR, MOVE to SR,
MOVE USP, MOVEC, MOVES, ORI to SR, PFLUSH, PLPA, RESET, RTE, STOP
```

#### 9.3.3 Floating-Point Instructions

| Mnemonic | OEP Class | Notes |
|----------|-----------|-------|
| `FABS/FDABS/FSABS` | pOEP-but-allows-sOEP | Except FPn,FPn and #imm,FPn forms = pOEP-only |
| `FADD/FDADD/FSADD` | pOEP-but-allows-sOEP | Same exception |
| `FBcc` | pOEP-only | |
| `FCMP` | pOEP-but-allows-sOEP | Same exception |
| `FDIV/FDDIV/FSDIV/FSGLDIV` | pOEP-but-allows-sOEP | Same exception |
| `FINT/FINTRZ` | pOEP-but-allows-sOEP | Same exception |
| `FMOVE (FP regs)` | pOEP-but-allows-sOEP | Same exception |
| `FMOVE (system CR)` | pOEP-only | |
| `FMOVEM` | pOEP-only | |
| `FMUL/FDMUL/FSMUL/FSGLMUL` | pOEP-but-allows-sOEP | Same exception |
| `FNEG/FDNEG/FSNEG` | pOEP-but-allows-sOEP | Same exception |
| `FNOP` | pOEP-only | |
| `FSQRT` | pOEP-but-allows-sOEP | Same exception |
| `FSUB/FDSUB/FSSUB` | pOEP-but-allows-sOEP | Same exception |
| `FTST` | pOEP-but-allows-sOEP | Same exception |

Multi-cycle FPU instructions classified as pOEP-but-allows-sOEP enable
integer instructions to execute in the sOEP while the FPU computes. This
is the primary mechanism for FPU/integer parallelism on the 68060.
`(MC68060UM §10.1.2)`

### 9.4 Instructions That Serialise

These instructions block the pipeline and prevent dual-issue:

- **MOVEM**: Always pOEP-only. Serialises the pipeline.
- **DIVS.L / DIVU.L**: pOEP-only, multi-cycle.
- **MULS.L / MULU.L**: pOEP-only, 2 cycles.
- **EXG, SWAP**: pOEP-only (modify two registers).
- **All BCD instructions** (ABCD, SBCD, NBCD, PACK, UNPK): pOEP-only.
- **Bit field instructions** (BFCHG, BFCLR, BFEXTS, BFEXTU, BFFFO, BFINS,
  BFSET, BFTST): pOEP-only.
- **DBcc**: pOEP-only.
- **All branch instructions** (BRA, BSR, JMP, JSR, RTS, RTR, RTD): pOEP-only.
- **NOP**: pOEP-only (used for pipeline synchronisation).
- **All privileged instructions**: pOEP-only.

### 9.5 Change/Use Penalties

When a register is modified by one instruction and used as an address
component by the immediately following instruction, the pipeline may stall:
`(MC68060UM §10.2)`

| Use | Penalty |
|-----|---------|
| Modified register used as base (An) | 2 cycles |
| Modified register used as index Xi.l*1 or Xi.l*4 | 2 cycles |
| Modified register used as index Xi.l*2, Xi.l*8, or Xi.w | 3 cycles |

**Zero-penalty instructions:** The following produce their result early
enough to avoid any change/use penalty:
```
LEA, MOVE.L #imm,Rn, MOVEQ, CLR.L Dn, any op (An)+, any op -(An)
```

### 9.6 Performance Comparison: 68060 vs RISC

The 68060 at 50 MHz achieves roughly:
- 100+ MIPS for integer (dual-issue peak)
- Sustained throughput of < 1 cycle per instruction for typical code
- 1.6-1.7x the MC68040 performance at the same clock rate
- 3.2-3.4x the performance of a 25 MHz MC68040

`(MC68060UM §1)`

---

## Appendix A. Exception Stack Frame Catalogue

This appendix collects all stack frame formats across all CPUs in one place.
The format code (4 bits) is in bits 15-12 of the format/vector-offset word
at SP+6.

### A.1 Format $0: Four-Word Normal Frame (All CPUs, 68010+)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0000 | Vector Offset             |
```
Total: 4 words (8 bytes)
Used for: Interrupts, TRAP #n, illegal instruction, A-line, F-line,
privilege violation, coprocessor pre-instruction.

### A.2 Format $1: Throwaway Frame (68020+)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0001 | Vector Offset             |
```
Total: 4 words (8 bytes)
Created on ISP during interrupt exception processing when transitioning
from master to interrupt state.

### A.3 Format $2: Six-Word Frame (68020+)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0010 | Vector Offset             |
SP+$08  |  Instruction Address (high)      |
SP+$0A  |  Instruction Address (low)       |
```
Total: 6 words (12 bytes)
Used for: CHK, CHK2, TRAPcc, TRAPV, trace, zero divide, address error
(68060), coprocessor post-instruction, MMU configuration.

### A.4 Format $3: FPU Post-Instruction Frame (68040+)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0011 | Vector Offset             |
SP+$08  |  Effective Address (high)        |
SP+$0A  |  Effective Address (low)         |
```
Total: 6 words (12 bytes)
Used for: FPU post-instruction exceptions on 68040/68060.

### A.5 Format $4: Eight-Word Frame (68060: Access Error; 68040: FP Unimpl)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 0100 | Vector Offset             |
SP+$08  |  Fault/Effective Address (high)  |
SP+$0A  |  Fault/Effective Address (low)   |
SP+$0C  |  FSLW or Internal Data (high)    |
SP+$0E  |  FSLW or Internal Data (low)     |
```
Total: 8 words (16 bytes)
68060: Access error (bus error / MMU fault)
68040: Floating-point unimplemented instruction, FP disabled (LC040/EC040)

### A.6 Format $7: Access Error Frame (68040 Only)

See Section 4.6 for the full 30-word layout.
Total: 30 words (60 bytes)

### A.7 Format $8: Long Bus/Address Error Frame (68010 Only)

See Section 1.7 for the full 29-word layout.
Total: 29 words (58 bytes)

### A.8 Format $9: Coprocessor Mid-Instruction Frame (68020/68030)
```
SP+$00  |  Status Register                 |
SP+$02  |  Program Counter (high)          |
SP+$04  |  Program Counter (low)           |
SP+$06  | 1001 | Vector Offset             |
SP+$08  |  Instruction Address (high)      |
SP+$0A  |  Instruction Address (low)       |
SP+$0C  |  Internal Registers (4 words)    |
 ...    |  ...                             |
SP+$12  |  (end of 10-word frame)          |
```
Total: 10 words (20 bytes)

### A.9 Format $A: Short Bus Fault Frame (68020/68030)

See Section 2.4 for the full 16-word layout.
Total: 16 words (32 bytes)

### A.10 Format $B: Long Bus Fault Frame (68020/68030)

See Section 2.4 for the full 46-word layout.
Total: 46 words (92 bytes)

### A.11 Stack Frame Format Summary by CPU

| Format | 68010 | 68020 | 68030 | 68040 | 68060 |
|--------|-------|-------|-------|-------|-------|
| $0 (4-word normal) | Yes | Yes | Yes | Yes | Yes |
| $1 (throwaway) | -- | Yes | Yes | Yes | -- |
| $2 (6-word) | -- | Yes | Yes | Yes | Yes |
| $3 (FP post-instruction) | -- | -- | -- | Yes | Yes |
| $4 (8-word) | -- | -- | -- | Yes* | Yes |
| $7 (access error, 30-word) | -- | -- | Yes** | Yes | -- |
| $8 (bus error, 29-word) | Yes | -- | -- | -- | -- |
| $9 (cp mid-instruction) | -- | Yes | Yes | -- | -- |
| $A (short bus fault, 16-word) | -- | Yes | Yes | -- | -- |
| $B (long bus fault, 46-word) | -- | Yes | Yes | -- | -- |

\* 68040 format $4 is for FP unimplemented/disabled only.
\*\* 68030 format $7 is for MMU faults.

---

## Appendix B. Instruction Set Delta Tables

This appendix lists every instruction that was added, removed, or changed
between CPU generations.

### B.1 Instructions Added per CPU

| CPU | Instructions Added |
|-----|-------------------|
| **68010** | `MOVEC`, `MOVES`, `MOVE from CCR` |
| **68020** | `BFCHG`, `BFCLR`, `BFEXTS`, `BFEXTU`, `BFFFO`, `BFINS`, `BFSET`, `BFTST`, `PACK`, `UNPK`, `CAS`, `CAS2`, `CALLM`, `RTM`, `DIVS.L`, `DIVU.L`, `MULS.L`, `MULU.L`, `TRAPcc`, `EXTB.L`, `CHK2`, `CMP2`, `cpBcc`, `cpDBcc`, `cpGEN`, `cpScc`, `cpTRAPcc`, `cpSAVE`, `cpRESTORE`, `RTD` |
| **68030** | `PFLUSH`, `PLOAD`, `PMOVE`, `PTEST` (MMU instructions) |
| **68040** | `CINV`, `CPUSH`, `MOVE16` |
| **68060** | `PLPA` |

### B.2 Instructions Removed per CPU

| CPU | Instructions Removed | Disposition |
|-----|---------------------|-------------|
| **68030** | `CALLM`, `RTM` | Unimplemented instruction exception |
| **68040** | `CALLM`, `RTM` | Same |
| **68060** | `MOVEP` | Unimplemented integer instruction exception (vector 61) |
| **68060** | `CAS2` | Unimplemented integer instruction exception (vector 61) |
| **68060** | `CAS` (misaligned) | Access error exception (must emulate) |

### B.3 Privilege Level Changes

| Instruction | 68000 | 68010+ |
|-------------|-------|--------|
| `MOVE SR,<ea>` | User | **Supervisor** (privilege violation in user mode) |

### B.4 FPU Instruction Coverage

| Instruction Group | 68881/82 | 68040 HW | 68060 HW |
|-------------------|----------|----------|----------|
| Basic arithmetic (FADD, FSUB, FMUL, FDIV) | Yes | Yes | Yes |
| FSQRT | Yes | Yes | Yes |
| FMOVE, FMOVEM, FCMP, FTST | Yes | Yes | Yes |
| FINT, FINTRZ | Yes | Yes | Yes |
| Branches (FBcc, FDBcc, FScc, FTRAPcc) | Yes | Yes | Yes |
| FSAVE, FRESTORE | Yes | Yes | Yes |
| Trigonometric (FSIN, FCOS, FSINCOS, FTAN) | Yes | **SW** | **SW** |
| Inverse trig (FASIN, FACOS, FATAN, FATANH) | Yes | **SW** | **SW** |
| Hyperbolic (FSINH, FCOSH, FTANH) | Yes | **SW** | **SW** |
| Exponential (FETOX, FETOXM1, FTWOTOX, FTENTOX) | Yes | **SW** | **SW** |
| Logarithmic (FLOGN, FLOGNP1, FLOG10, FLOG2) | Yes | **SW** | **SW** |
| FMOD, FREM | Yes | **SW** | **SW** |
| FSGLDIV, FSGLMUL | Yes | **SW** | **SW** |
| FSCALE, FGETEXP, FGETMAN | Yes | Yes | **SW** |
| FMOVECR (ROM constants) | Yes | **SW** | **SW** |
| Single/double precision (FSADD, FDADD, etc.) | -- | Yes | Yes |

**SW** = Software emulated via FPSP/ISP (F-line exception trap handler).

---

## Appendix C. Cache Control Quick Reference

### C.1 Cache Architecture Summary

| Property | 68020 | 68030 | 68040 | 68060 |
|----------|-------|-------|-------|-------|
| I-Cache Size | 256 B | 256 B | 4 KB | 8 KB |
| D-Cache Size | -- | 256 B | 4 KB | 8 KB |
| I-Cache Type | Direct-mapped | Direct-mapped | 4-way SA | 4-way SA |
| D-Cache Type | -- | Direct-mapped | 4-way SA | 4-way SA |
| Line Size | 4 bytes | 4 bytes | 16 bytes | 16 bytes |
| D-Cache Write | -- | Write-through | WT or copyback | WT or copyback |
| Burst Fill | No | Optional | Yes | Yes |
| Snoop Support | No | No | Yes | Yes |
| Branch Cache | No | No | No | 256-entry BTB |
| Store Buffer | No | No | No | 4-entry |

### C.2 AmigaOS Cache Management Functions

| Function | Effect |
|----------|--------|
| `CacheClearU()` | Flush all caches (push dirty lines, invalidate) |
| `CacheClearE(addr, len, caches)` | Flush specific address range |
| `CacheControl(bits_set, bits_clear)` | Set/clear CACR bits |
| `CachePreDMA(addr, len, flags)` | Prepare for DMA (push dirty lines) |
| `CachePostDMA(addr, len, flags)` | Clean up after DMA (invalidate) |

### C.3 Self-Modifying Code and Cache Flushes

On 68020+ systems, self-modifying code (including library function patching,
Copper list generation by CPU, and JIT compilation) must flush the instruction
cache after modifying code in memory. Without a flush, the CPU may execute
stale instructions from the I-cache.

**Why this matters for Amiga emulation:**
- `SetFunction()` patches library function vectors -- must flush I-cache
- Programs that generate code at runtime (JIT, self-modifying demos)
- Copper lists are not affected (the Copper reads from chip RAM, not CPU cache)
  but CPU code that writes Copper lists and then reads them back IS affected

AmigaOS calls `CacheClearU()` in `SetFunction()` and related functions.
Well-behaved programs call `CacheClearU()` after writing code to memory.
Badly-behaved programs may work on 68000/68010 (no cache) but fail on 68020+.

---

## Appendix D. Gaps and Source Map

### D.1 Known Gaps

| Topic | Gap | Impact |
|-------|-----|--------|
| 68030 bus interface timing | Part 2 of MC68030UM not available | Bus timing specifications not covered |
| 68010 instruction timing | MC68010 datasheet has AC specs but no instruction timing table | Must rely on M68000 Family Ref for instruction times |
| 68020 detailed instruction timing | Timing is context-dependent due to overlap | Examples provided but no comprehensive table |
| 68040 FPU timing | Not covered here | See companion FPU reference |
| 68060 FPU timing | Partially covered (section 10.15 of MC68060UM) | See companion FPU reference |
| 68020/68030 addressing mode cycle costs | Not tabulated per mode | Manual discusses them qualitatively |
| 68040/68060 TLB miss cost | Page table walk timing varies | Depends on table depth and memory speed |
| 68060 precise vs imprecise exception mode | Mentioned but not fully detailed | See MC68060UM §11.1.3 |

### D.2 Source Map

| Section | Primary Source | Pages/Lines |
|---------|---------------|-------------|
| 1 (68010) | MC68010 Technical Data | Lines 1-2073 |
| 2 (68020) | MC68020UM | Lines 1-17380 |
| 3 (68030) | MC68030UM-P1 | Lines 1-14098 |
| 4 (68040) | MC68040UM | Lines 1-25718 |
| 5 (68060) | MC68060UM | Lines 1-22576 |
| 6 (Amiga quirks) | amiga-68000-timing.md, community knowledge | -- |
| 7 (MOVEC) | All manuals, cross-referenced | -- |
| 8 (Encoding) | MC68020UM §1.3, MC68030UM §2.4 | -- |
| 9 (Superscalar) | MC68060UM §10 | Lines 16200-16700 |
| App A (Frames) | All manuals' exception processing sections | -- |
| App B (Deltas) | All manuals' instruction set summaries | -- |
| App C (Caches) | All manuals' cache sections | -- |

### D.3 Cross-References

| Related Doc | What It Covers | Overlap Policy |
|-------------|---------------|----------------|
| `amiga-68000-timing.md` | 68000 bus cycles, prefetch, exception handling, instruction timing | Baseline -- not duplicated here |
| `amiga-fpu-68881-reference.md` | FPU instruction set, data formats, timing, exception model | FPU details not duplicated; only 68040/68060 FPU differences covered here |

---

## Appendix E0. Implementation Checklist for Emulator Authors

### E0.1 Per-CPU Implementation Priority

When implementing 68010+ CPU emulation, implement features in this order:

**68010 (minimal effort, rarely needed):**
- [ ] VBR register (MOVEC $801)
- [ ] MOVE from SR is privileged (privilege violation in user mode)
- [ ] MOVE from CCR instruction (new)
- [ ] MOVEC instruction (SFC, DFC, USP, VBR)
- [ ] MOVES instruction
- [ ] Bus error long frame (format $8) if virtual memory support needed
- [ ] Loop mode (optional -- timing only)

**68020 (required for A1200 emulation):**
- [ ] 32-bit address space (EC020 wraps at 24 bits externally)
- [ ] 18 addressing modes (full extension word decoding)
- [ ] All new instructions (bit fields, CAS/CAS2, PACK/UNPK, TRAPcc, etc.)
- [ ] Extended multiply/divide (32x32->64, 64/32->32:32)
- [ ] Instruction cache (256 bytes, direct-mapped)
- [ ] CACR and CAAR registers
- [ ] Master/Interrupt stack pointer (MSP/ISP, M bit in SR)
- [ ] T0 trace mode (trace on change of flow)
- [ ] Exception stack frames: $0, $1, $2, $9, $A, $B
- [ ] Coprocessor interface (F-line decoding for FPU)
- [ ] Dynamic bus sizing (model cycle counts for 16-bit chip RAM)

**68030 (required for A3000 emulation):**
- [ ] Everything from 68020
- [ ] Data cache (256 bytes, write-through)
- [ ] On-chip MMU (ATC, table walk, PFLUSH/PLOAD/PMOVE/PTEST)
- [ ] Transparent translation registers (TT0, TT1)
- [ ] Remove CALLM/RTM (generate exception)
- [ ] Burst mode support (affects cache fill timing)

**68040 (required for A4000/040 emulation):**
- [ ] 4 KB I-cache + 4 KB D-cache (4-way set-associative)
- [ ] Copyback data cache with dirty line tracking
- [ ] CINV/CPUSH instructions
- [ ] MOVE16 instruction
- [ ] Dual MMU (I-ATC + D-ATC, 64 entries each)
- [ ] Fixed page sizes (4 KB / 8 KB)
- [ ] MOVEC for MMU registers (ITT0/ITT1/DTT0/DTT1/URP/SRP/TC/MMUSR)
- [ ] Integrated FPU (subset -- generate exceptions for missing instructions)
- [ ] Access error stack frame (format $7) with write-back fields
- [ ] Simplified CACR (just enable bits)
- [ ] Synchronous bus model

**68060 (required for accelerator card emulation):**
- [ ] Everything from 68040
- [ ] 8 KB I-cache + 8 KB D-cache
- [ ] Superscalar dispatch (dual-issue)
- [ ] Branch cache (256-entry BTB, branch prediction)
- [ ] Store buffer (4-entry)
- [ ] Access error stack frame (format $4) with FSLW
- [ ] Remove MOVEP (vector 61 exception)
- [ ] Remove CAS2 (vector 61 exception)
- [ ] Unimplemented effective address exception (vector 60)
- [ ] PCR register
- [ ] BUSCR register
- [ ] Extended CACR (branch cache enable, store buffer enable, etc.)
- [ ] Additional FPU instructions missing (FSCALE, FGETEXP, FGETMAN)

### E0.2 Common Implementation Mistakes

1. **Forgetting MOVE from SR privilege change.** The most common compatibility
   issue. Kickstart 1.x code uses `MOVE SR,Dn` in user mode; on 68010+ this
   traps. Without a handler, the system crashes.

2. **Not implementing the format word in stack frames.** Every exception on
   68010+ pushes a format/vector-offset word. RTE reads this word to
   determine frame size. If you push a 68000-style frame (no format word),
   RTE will unstack the wrong amount and corrupt the stack.

3. **Ignoring cache coherency.** Self-modifying code works on 68000 (no cache)
   but fails silently on 68020+ if the I-cache is not flushed. The emulator
   must either model the cache or automatically detect code modifications.

4. **Not handling the 68040 write-back mechanism.** When the 68040 takes an
   access error, up to three pending writes are saved in the stack frame.
   The exception handler must complete these writes. If your emulator does
   not model this, virtual memory (Enforcer, MungWall) will malfunction.

5. **Assuming aligned access.** The 68020+ handle misaligned access
   transparently (extra cycles, no exception). The 68060 generates an
   exception for misaligned CAS. Address error exceptions only occur for
   instruction fetches from odd addresses (all CPUs).

6. **Ignoring the 68060 branch cache.** The branch cache causes zero-cycle
   branches when predicted correctly and 8-10 cycle penalties on
   misprediction. If you aim for cycle-accurate 68060 emulation, you must
   model branch prediction.

---

## Appendix E. Detailed Instruction Timing Tables

### E.1 MC68020/68030 Instruction Timing (Selected Instructions)

All times in clock cycles. Assumes 32-bit memory, no wait states, cache
enabled and hitting. "Overlap" means the instruction can partially execute
concurrently with the previous instruction's bus activity.
`(MC68020UM §8.2)`

#### E.1.1 MOVE Instructions

| Instruction | Head | Tail | I-Cache Miss Penalty |
|-------------|------|------|---------------------|
| `MOVE.L Dn,Dm` | 2 | 0 | +2 |
| `MOVE.L Dn,(An)` | 2 | 1 | +2 |
| `MOVE.L Dn,(d16,An)` | 2 | 1 | +2 |
| `MOVE.L (An),Dm` | 2 | 1 | +2 |
| `MOVE.L (An),(An)` | 2 | 2 | +2 |
| `MOVE.L (An)+,Dm` | 2 | 1 | +2 |
| `MOVE.L Dm,-(An)` | 2 | 1 | +2 |
| `MOVE.L #imm,Dn` | 2 | 0 | +2 |
| `MOVEM.L regs,(An)` | 4+2n | 0 | +2 per miss |
| `MOVEM.L (An),regs` | 4+2n | 0 | +2 per miss |

n = number of registers in the register list.

#### E.1.2 Arithmetic Instructions

| Instruction | Head | Tail | Notes |
|-------------|------|------|-------|
| `ADD.L Dn,Dm` | 2 | 0 | Can overlap completely |
| `ADD.L (An),Dn` | 2 | 1 | 1 read cycle |
| `ADD.L Dn,(An)` | 2 | 2 | 1 read + 1 write |
| `ADDQ.L #imm,Dn` | 2 | 0 | |
| `ADDQ.L #imm,(An)` | 2 | 2 | Read-modify-write |
| `SUB.L Dn,Dm` | 2 | 0 | |
| `MULU.W Dn,Dm` | 28 | 0 | Worst case; average ~20 |
| `MULS.W Dn,Dm` | 28 | 0 | Worst case; average ~20 |
| `MULU.L Dn,Dm` | 44 | 0 | 32x32->32 |
| `MULU.L Dn,Dh:Dl` | 44 | 0 | 32x32->64 |
| `DIVU.W Dn,Dm` | 44 | 0 | |
| `DIVS.W Dn,Dm` | 56 | 0 | |
| `DIVU.L Dn,Dm` | 44 | 0 | 32/32->32:32 |
| `DIVS.L Dn,Dm` | 56 | 0 | 32/32->32:32 |

#### E.1.3 Branch Instructions

| Instruction | Taken | Not Taken |
|-------------|-------|-----------|
| `Bcc.B (byte disp)` | 6 | 4 |
| `Bcc.W (word disp)` | 6 | 6 |
| `BRA.B` | 6 | -- |
| `BRA.W` | 6 | -- |
| `BSR.B` | 6 | -- |
| `BSR.W` | 6 | -- |
| `JMP (An)` | 4 | -- |
| `JSR (An)` | 4 | -- |
| `RTS` | 10 | -- |
| `DBcc` (not expired) | 6 | 8 |
| `DBcc` (expired) | 10 | -- |

### E.2 MC68040 Instruction Timing (Selected Instructions)

All times in clock cycles, best case (operand cache hits, aligned).
`(MC68040UM §10)`

| Instruction | Cycles | Reads | Writes | Notes |
|-------------|--------|-------|--------|-------|
| `MOVE.L Dn,Dm` | 1 | 0 | 0 | |
| `MOVE.L (An),Dn` | 1 | 1 | 0 | D-cache hit |
| `MOVE.L Dn,(An)` | 1 | 0 | 1 | |
| `MOVE.L (An),(An)` | 3 | 1 | 1 | |
| `MOVEQ #imm,Dn` | 1 | 0 | 0 | |
| `ADD.L Dn,Dm` | 1 | 0 | 0 | |
| `ADD.L (An),Dn` | 1 | 1 | 0 | |
| `ADD.L #imm,Dn` | 1 | 0 | 0 | |
| `ADDQ.L #imm,Dn` | 1 | 0 | 0 | |
| `MULU.W Dn,Dm` | 2 | 0 | 0 | Hardware multiplier |
| `MULU.L Dn,Dm` | 2 | 0 | 0 | |
| `MULU.L Dn,Dh:Dl` | 3 | 0 | 0 | 64-bit result |
| `MULS.W Dn,Dm` | 2 | 0 | 0 | |
| `MULS.L Dn,Dm` | 2 | 0 | 0 | |
| `DIVU.W Dn,Dm` | 38 | 0 | 0 | Still multi-cycle |
| `DIVU.L Dn,Dm` | 38 | 0 | 0 | |
| `DIVS.W Dn,Dm` | 44 | 0 | 0 | |
| `DIVS.L Dn,Dm` | 44 | 0 | 0 | |
| `LEA (An),Am` | 1 | 0 | 0 | |
| `LEA (d16,An),Am` | 1 | 0 | 0 | |
| `LEA (d8,An,Xi),Am` | 1 | 0 | 0 | |
| `PEA (An)` | 2 | 0 | 1 | |
| `CLR.L Dn` | 1 | 0 | 0 | |
| `CLR.L (An)` | 1 | 0 | 1 | |
| `TST.L Dn` | 1 | 0 | 0 | |
| `CMP.L Dn,Dm` | 1 | 0 | 0 | |
| `Bcc (taken)` | 2 | 0 | 0 | Both paths prefetched |
| `Bcc (not taken)` | 1 | 0 | 0 | |
| `BRA` | 2 | 0 | 0 | |
| `BSR` | 3 | 0 | 1 | Push return address |
| `JMP (An)` | 2 | 0 | 0 | |
| `JSR (An)` | 3 | 0 | 1 | |
| `RTS` | 3 | 1 | 0 | |
| `NOP` | 1 | 0 | 0 | |
| `MOVEM.L regs,(An)` | 3+n | 0 | n | n = register count |
| `MOVEM.L (An),regs` | 2+n | n | 0 | |
| `EXG Dn,Dm` | 2 | 0 | 0 | |
| `SWAP Dn` | 1 | 0 | 0 | |
| `EXT.W Dn` | 1 | 0 | 0 | |
| `EXT.L Dn` | 1 | 0 | 0 | |
| `EXTB.L Dn` | 1 | 0 | 0 | |
| `TAS (An)` | 3 | 1 | 1 | Locked R-M-W |
| `CAS.L Dc,Du,(An)` | 3-4 | 1 | 0-1 | Locked R-M-W |
| `CINVA IC` | 16 | 0 | 0 | |
| `CINVA DC` | 16 | 0 | 0 | Plus bus cycles for dirty |
| `CPUSHA DC` | 16 | 0 | varies | Dirty lines pushed |
| `MOVE16 (Ax)+,xxx.L` | 3 | 1 (line) | 1 (line) | 16-byte transfer |

### E.3 MC68060 Instruction Timing (Selected Instructions)

All times in clock cycles, single-dispatch (pOEP only). When dual-issued,
the effective throughput for the instruction pair equals the pOEP instruction
time. `(MC68060UM §10.5-10.15)`

#### E.3.1 Move Instructions

| Instruction | Cycles (r/w) | OEP Class |
|-------------|--------------|-----------|
| `MOVE.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `MOVE.L Dn,(An)` | 1(0/1) | pOEP\|sOEP |
| `MOVE.L (An),Dn` | 1(1/0) | pOEP\|sOEP |
| `MOVE.L (An)+,Dn` | 1(1/0) | pOEP\|sOEP |
| `MOVE.L Dn,-(An)` | 1(0/1) | pOEP\|sOEP |
| `MOVE.L (An),(Am)` | 2(1/1) | pOEP-until-last |
| `MOVE.L #imm,(An)` | 2(0/1) | pOEP-until-last |
| `MOVEA.L (An),Am` | 1(1/0) | pOEP\|sOEP |
| `MOVEQ #imm,Dn` | 1(0/0) | pOEP\|sOEP |

#### E.3.2 Arithmetic Instructions

| Instruction | Cycles (r/w) | OEP Class |
|-------------|--------------|-----------|
| `ADD.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `ADD.L (An),Dn` | 1(1/0) | pOEP\|sOEP |
| `ADD.L Dn,(An)` | 3(1/1) | pOEP-until-last |
| `ADDQ.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ADDI.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `SUB.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `CMP.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `CMPI.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `MULU.W Dn,Dm` | 2(0/0) | pOEP-only |
| `MULU.L Dn,Dm` | 2(0/0) | pOEP-only |
| `MULU.L Dn,Dh:Dl` | 3(0/0) | pOEP-only |
| `MULS.W Dn,Dm` | 2(0/0) | pOEP-only |
| `MULS.L Dn,Dm` | 2(0/0) | pOEP-only |
| `DIVU.W Dn,Dm` | 38(0/0) | pOEP-only |
| `DIVU.L Dn,Dm` | 38(0/0) | pOEP-only |
| `DIVS.W Dn,Dm` | 38(0/0) | pOEP-only |
| `DIVS.L Dn,Dm` | 38(0/0) | pOEP-only |

#### E.3.3 Logical and Shift Instructions

| Instruction | Cycles (r/w) | OEP Class |
|-------------|--------------|-----------|
| `AND.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `OR.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `EOR.L Dn,Dm` | 1(0/0) | pOEP\|sOEP |
| `NOT.L Dn` | 1(0/0) | pOEP\|sOEP |
| `NEG.L Dn` | 1(0/0) | pOEP\|sOEP |
| `LSL.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `LSR.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ASL.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ASR.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ROL.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ROR.L #imm,Dn` | 1(0/0) | pOEP\|sOEP |
| `ROXL.L #imm,Dn` | 1(0/0) | pOEP-only |
| `ROXR.L #imm,Dn` | 1(0/0) | pOEP-only |

#### E.3.4 Branch Instructions

| Instruction | Cycles | OEP Class | Notes |
|-------------|--------|-----------|-------|
| `Bcc (predicted taken)` | 0 | special | Branch folded |
| `Bcc (predicted not-taken, correct)` | 0 | pOEP-but-allows-sOEP | |
| `Bcc (mispredicted)` | 8-10 | pOEP-only | Pipeline flush |
| `Bcc (not in branch cache, forward)` | 1 | pOEP-but-allows-sOEP | Predicted not-taken |
| `Bcc (not in branch cache, backward)` | 0 | pOEP-only | Predicted taken |
| `BRA` | 0 | pOEP-only | Folded |
| `BSR` | 1(0/1) | pOEP-only | |
| `JMP (An)` | 0 | pOEP-only | Folded |
| `JSR (An)` | 1(0/1) | pOEP-only | |
| `RTS` | 1(1/0) | pOEP-only | |
| `DBcc (not expired)` | 1(0/0) | pOEP-only | |

#### E.3.5 Miscellaneous Instructions

| Instruction | Cycles (r/w) | OEP Class |
|-------------|--------------|-----------|
| `LEA (d16,An),Am` | 1(0/0) | pOEP\|sOEP |
| `LEA (d8,An,Xi),Am` | 1(0/0) | pOEP\|sOEP |
| `PEA (An)` | 2(0/1) | pOEP-only |
| `LINK An,#d16` | 2(0/1) | pOEP-until-last |
| `UNLK An` | 2(1/0) | pOEP-only |
| `NOP` | 1(0/0) | pOEP-only |
| `SWAP Dn` | 1(0/0) | pOEP-only |
| `EXG Dn,Dm` | 1(0/0) | pOEP-only |
| `EXT.W Dn` | 1(0/0) | pOEP\|sOEP |
| `EXT.L Dn` | 1(0/0) | pOEP\|sOEP |
| `EXTB.L Dn` | 1(0/0) | pOEP\|sOEP |
| `CLR.L Dn` | 1(0/0) | pOEP\|sOEP |
| `CLR.L (An)` | 1(0/1) | pOEP\|sOEP |
| `TST.L Dn` | 1(0/0) | pOEP\|sOEP |
| `TAS (An)` | 4(1/1) | pOEP-only |
| `CAS.L Dc,Du,(An)` | 4(1/1) | pOEP-only |
| `MOVEM.L regs,(An)` | 3+n(0/n) | pOEP-only |
| `MOVEM.L (An),regs` | 2+n(n/0) | pOEP-only |

#### E.3.6 Cache/MMU/Exception Timing

| Operation | Cycles | Notes |
|-----------|--------|-------|
| Instruction ATC miss | +9-20 | Depends on table depth |
| Data ATC miss | +9-20 | Depends on table depth |
| Instruction cache miss | +3-5 | Line fill from bus |
| Data cache miss (read) | +3-5 | Line fill from bus |
| Data cache miss (write, copyback) | +3-5 | Line fill + push if dirty |
| Interrupt exception | ~18 | Including vector fetch |
| Trap exception | ~12 | |
| Branch prediction miss | 8-10 | Pipeline refill |

### E.4 Effective Address Calculation Time Penalties (68060)

The 68060 charges additional cycles for complex addressing modes.
`(MC68060UM §10.4)`

| Addressing Mode | Additional Cycles | sOEP Allowed? |
|-----------------|-------------------|---------------|
| Dn | 0 | Yes |
| An | 0 | Yes |
| (An) | 0 | Yes |
| (An)+ | 0 | Yes |
| -(An) | 0 | Yes |
| (d16,An) | 0 | Yes |
| (d8,An,Xi*SF) | 0 | Yes (Xi.l*1 or Xi.l*4) |
| (d8,An,Xi*SF) | 0 | **No** (Xi.l*2, Xi.l*8, Xi.w) |
| (bd,An,Xi*SF) | 0 | **No** |
| ([bd,An],Xi,od) | +2 per indirection | **No** |
| ([bd,An,Xi],od) | +2 per indirection | **No** |
| (d16,PC) | 0 | **No** |
| (d8,PC,Xi*SF) | 0 | **No** |
| (bd,PC,Xi*SF) | 0 | **No** |
| (xxx).W | 0 | Yes |
| (xxx).L | 0 | Yes |
| #imm | 0 | Yes |

---

## Appendix F. Detailed Register Layouts

### F.1 68040 Special Status Word (SSW) -- Format $7 Stack Frame

The SSW in the 68040 access error stack frame (format $7) describes what
the CPU was attempting when the fault occurred. `(MC68040UM §8.4.6)`

```
Bit 15-12: CP  -- Completion status of pending writes
Bit 11-10: CU  -- Undefined
Bit 9:     CT  -- Undefined
Bit 8:     CM  -- Undefined
Bit 7:     MA  -- Misaligned access flag
Bit 6:     ATC -- ATC fault (vs physical bus error)
Bit 5:     LK  -- Locked transfer
Bit 4:     RW  -- Read/Write (1=read, 0=write)
Bit 3-2:   SIZ -- Transfer size (00=byte, 01=word, 10=long, 11=line)
Bit 1-0:   TT  -- Transfer type
```

The write-back status words (WB3S, WB2S, WB1S) at offsets $0E, $10, $12
describe up to three pending writes that must be completed by the exception
handler before returning.

### F.2 68060 Fault Status Long Word (FSLW) -- Format $4 Stack Frame

The FSLW provides detailed fault information for the 68060 access error
handler. `(MC68060UM §8.4.4)`

```
Bit 31-28: Reserved
Bit 27:    MA  -- Misaligned access
Bit 26-24: Reserved
Bit 23:    LK  -- Locked bus transfer
Bit 22-21: RW  -- Read/Write field (00=write, 01=read, 10=RMW read, 11=RMW write)
Bit 20-18: SIZE -- Transfer size (000=byte, 001=word, 010=long, 011=line, 100-111=reserved)
Bit 17-16: TT  -- Transfer type
Bit 15-12: TM  -- Transfer modifier
Bit 11:    IO  -- Instruction or operand (1=instruction, 0=operand)
Bit 10:    PBE -- Push buffer bus error
Bit 9:     SBE -- Store buffer bus error
Bit 8:     PTA -- Physical address valid (for PTEST)
Bit 7:     PTAE -- Physical translated address error
Bit 6:     Reserved
Bit 5:     SP  -- Supervisor protect fault
Bit 4:     WP  -- Write protect fault
Bit 3:     TWE -- Table walk error (bus error during table search)
Bit 2:     RE  -- Resident fault (page not resident)
Bit 1:     WE  -- Reserved
Bit 0:     BPE -- Branch prediction error
```

**Key fields for virtual memory handlers:**
- **RE (bit 2):** Page not resident -- demand paging trigger
- **WP (bit 4):** Write protection fault -- copy-on-write trigger
- **SP (bit 5):** Supervisor-only page accessed from user mode
- **TWE (bit 3):** Bus error during table walk -- system error
- **PBE (bit 10) / SBE (bit 9):** Write buffer bus errors (non-recoverable)
- **BPE (bit 0):** Branch prediction error -- clear branch cache and retry

### F.3 68030 Translation Control Register (TC)

The TC register controls the MMU operation on the 68030. Accessed via PMOVE.
`(MC68030UM §9)`

```
Bit 31:    E   -- Enable MMU (1=enabled, 0=disabled)
Bit 30:    SRE -- Supervisor Root Pointer Enable
Bit 29:    FCL -- Function Code Lookup
Bit 28-24: PS  -- Page Size (encoding: 00100=256B, 01000=1KB, ... 01111=32KB)
Bit 23-20: IS  -- Initial Shift
Bit 19-16: TIA -- Table Index A bits
Bit 15-12: TIB -- Table Index B bits
Bit 11-8:  TIC -- Table Index C bits
Bit 7-4:   TID -- Table Index D bits
Bit 3-0:   Reserved
```

### F.4 68040/68060 Translation Control Register (TC/TCR)

Accessed via MOVEC (Rc=$003).

**68040 TC:**
```
Bit 15:    E   -- Enable MMU
Bit 14:    P   -- Page size (0=4KB, 1=8KB)
Bit 13-0:  Reserved
```

**68060 TCR:**
```
Bit 15:    E   -- Enable MMU
Bit 14:    P   -- Page size (0=4KB, 1=8KB)
Bit 13:    NAD -- Default No Allocate Data
Bit 12:    NAI -- Default No Allocate Instruction
Bit 11-10: DCM -- Default Cache Mode (00=WT, 01=CB, 10=CI precise, 11=CI imprecise)
Bit 9:     DWP -- Default Write Protect
Bit 8:     DUP -- Default User Page
Bit 7-0:   Reserved
```

The 68060 adds default translation bits that apply when no transparent
translation register matches and MMU is disabled or the EC variant is used.

### F.5 68040/68060 Transparent Translation Registers (ITT0/ITT1/DTT0/DTT1)

Accessed via MOVEC. Format is the same for all four registers.

```
Bit 31-24: LA  -- Logical Address Base
Bit 23-16: LAM -- Logical Address Mask (1=ignore this bit in comparison)
Bit 15:    E   -- Enable
Bit 14-13: S   -- Supervisor mode (00=match both, 01=match user only, 10=match supervisor only)
Bit 12-10: Reserved
Bit 9-8:   CM  -- Cache Mode (00=WT, 01=CB, 10=CI precise, 11=CI imprecise)
Bit 7:     Reserved
Bit 6:     W   -- Write protect (1=write protected)
Bit 5-0:   Reserved
```

The transparent translation registers match when:
```
(Logical Address[31:24] AND NOT LAM) == (LA AND NOT LAM) AND mode matches S field
```

If a match occurs, the logical address is used directly as the physical
address (transparent/identity mapping), and the CM and W fields control
caching and write protection.

### F.6 68060 Processor Configuration Register (PCR)

Accessed via MOVEC (Rc=$808).

```
Bit 31-16: Revision and ID (read-only)
  Bit 31-18: Revision level
  Bit 17-16: CPU ID (00=68060)
Bit 15-6:  Reserved
Bit 5:     Reserved
Bit 4:     Reserved
Bit 3-2:   Reserved
Bit 1:     EDFS -- Enable Dual-issue FPU (0=disable FPU dispatch, 1=enable)
Bit 0:     ESS  -- Enable Superscalar dispatch (0=single issue, 1=dual issue)
```

Setting ESS=0 forces single-issue mode (useful for debugging or avoiding
superscalar-related issues). Setting EDFS=0 prevents FPU instructions from
being dispatched alongside integer instructions.

---

## Appendix G. Exception Vector Table (68010-68060)

This table shows vector assignments that differ from or are not present on
the 68000. For the full 68000 vector table, see the companion
`amiga-68000-timing.md`.

| Vector | Offset | Assignment | CPUs | Notes |
|--------|--------|------------|------|-------|
| 0 | $000 | Reset Initial SSP | All | |
| 1 | $004 | Reset Initial PC | All | |
| 2 | $008 | Access Fault / Bus Error | All | Frame format varies by CPU |
| 3 | $00C | Address Error | All | |
| 4 | $010 | Illegal Instruction | All | |
| 5 | $014 | Integer Divide by Zero | All | |
| 6 | $018 | CHK, CHK2 | 68020+ | CHK2 is new |
| 7 | $01C | TRAPcc, TRAPV | 68020+ | TRAPcc is new |
| 8 | $020 | Privilege Violation | All | |
| 9 | $024 | Trace | All | |
| 10 | $028 | Line 1010 Emulator | All | Unimplemented A-line |
| 11 | $02C | Line 1111 Emulator | All | Unimplemented F-line / FPU unimpl / FP disabled |
| 12 | $030 | Emulator Interrupt | 68020+ | |
| 13 | $034 | Coprocessor Protocol Violation | 68020/030 | Not used by 68040/060 |
| 14 | $038 | Format Error | All (68010+) | Invalid frame format on RTE |
| 15 | $03C | Uninitialised Interrupt | All | |
| 24 | $060 | Spurious Interrupt | All | |
| 25-31 | $064-$07C | Level 1-7 Autovectors | All | |
| 32-47 | $080-$0BC | TRAP #0-15 | All | |
| 48 | $0C0 | FP Branch/Set on Unordered Condition | 68040/060 | Via FPSP |
| 49 | $0C4 | FP Inexact Result | 68040/060 | Via FPSP |
| 50 | $0C8 | FP Divide by Zero | 68040/060 | |
| 51 | $0CC | FP Underflow | 68040/060 | Via FPSP |
| 52 | $0D0 | FP Operand Error | 68040/060 | Via FPSP |
| 53 | $0D4 | FP Overflow | 68040/060 | Via FPSP |
| 54 | $0D8 | FP Signaling NAN | 68040/060 | Via FPSP |
| 55 | $0DC | FP Unimplemented Data Type | 68040/060 | FPSP handler required |
| 56 | $0E0 | MMU Configuration Error | 68030/68851 | Not used by 68040/060 |
| 57 | $0E4 | Reserved (68851) | 68851 | |
| 58 | $0E8 | Reserved (68851) | 68851 | |
| 60 | $0F0 | Unimplemented Effective Address | **68060 only** | Complex EA not in HW |
| 61 | $0F4 | Unimplemented Integer Instruction | **68060 only** | MOVEP, CAS2 |
| 64-255 | $100-$3FC | User-Defined Interrupt Vectors | All | 192 vectors |

---

## Appendix H. Addressing Mode Comparison

This table shows which addressing modes are available on each CPU.

| Mode | Syntax | 68000/010 | 68020/030 | 68040 | 68060 |
|------|--------|-----------|-----------|-------|-------|
| Data Register Direct | `Dn` | Yes | Yes | Yes | Yes |
| Address Register Direct | `An` | Yes | Yes | Yes | Yes |
| Address Register Indirect | `(An)` | Yes | Yes | Yes | Yes |
| Address Register Indirect with Postincrement | `(An)+` | Yes | Yes | Yes | Yes |
| Address Register Indirect with Predecrement | `-(An)` | Yes | Yes | Yes | Yes |
| Address Register Indirect with Displacement | `(d16,An)` | Yes | Yes | Yes | Yes |
| Address Register Indirect with Index (Brief) | `(d8,An,Xn)` | Yes* | Yes | Yes | Yes |
| Address Register Indirect with Index (Full) | `(bd,An,Xn)` | -- | Yes | Yes | Yes** |
| Memory Indirect Postindexed | `([bd,An],Xn,od)` | -- | Yes | Yes | Yes** |
| Memory Indirect Preindexed | `([bd,An,Xn],od)` | -- | Yes | Yes | Yes** |
| PC Indirect with Displacement | `(d16,PC)` | Yes | Yes | Yes | Yes |
| PC Indirect with Index (Brief) | `(d8,PC,Xn)` | Yes* | Yes | Yes | Yes |
| PC Indirect with Index (Full) | `(bd,PC,Xn)` | -- | Yes | Yes | Yes** |
| PC Memory Indirect Postindexed | `([bd,PC],Xn,od)` | -- | Yes | Yes | Yes** |
| PC Memory Indirect Preindexed | `([bd,PC,Xn],od)` | -- | Yes | Yes | Yes** |
| Absolute Short | `(xxx).W` | Yes | Yes | Yes | Yes |
| Absolute Long | `(xxx).L` | Yes | Yes | Yes | Yes |
| Immediate | `#imm` | Yes | Yes | Yes | Yes |

\* On 68000/010, scale factor is always 1 (bits 10-9 of extension word ignored).
\*\* On 68060, full extension word and memory indirect modes work but some
generate an "unimplemented effective address" exception (vector 60) for
certain instruction combinations. The 68060 ISP (Instruction Support
Package) emulates these in software. They also cannot execute in the sOEP.

---

## Appendix I. Bus Signal Comparison

| Signal | 68000/010 | 68020 | 68030 | 68040 | 68060 |
|--------|-----------|-------|-------|-------|-------|
| Data Bus Width | D0-D15 (16) | D0-D31 (32) | D0-D31 (32) | D0-D31 (32) | D0-D31 (32) |
| Address Bus | A1-A23 (23) | A0-A31 (32) | A0-A31 (32) | A0-A31 (32) | A0-A31 (32) |
| Bus Protocol | Async (DTACK) | Async (DSACK) | Async (DSACK) + Burst | **Sync** (TA/TEA) | **Sync** (TA/TEA) |
| Bus Sizing | 16-bit fixed | Dynamic (8/16/32) | Dynamic (8/16/32) | 32-bit only | 32-bit only |
| Function Codes | FC0-FC2 | FC0-FC2 | FC0-FC2 | TT0-TT1, TM0-TM2 | TT0-TT1, TM0-TM2 |
| Address Strobe | AS | AS | AS | TS (Transfer Start) | TS |
| Data Strobe | UDS, LDS | DS | DS | -- | -- |
| Read/Write | R/W | R/W | R/W | R/W | R/W |
| Bus Error | BERR | BERR | BERR | TEA (Transfer Error Ack) | TEA |
| Bus Grant | BG | BG | BG | BG | BG |
| Bus Request | BR | BR | BR | BR | BR |
| Bus Acknowledge | BGACK | BGACK | BGACK | BB (Bus Busy) | BB |
| Halt | HALT | HALT | HALT | -- | -- |
| Cache Disable | -- | CDIS | CDIS | CDIS | CDIS |
| MMU Disable | -- | -- | -- | MDIS | MDIS |
| Interrupt | IPL0-IPL2 | IPL0-IPL2 | IPL0-IPL2 | IPL0-IPL2 | IPL0-IPL2 |
| Clock Input | CLK | CLK | CLK | BCLK + PCLK | CLK |
| Snoop Control | -- | -- | -- | SC0-SC1 | SNOOP |
| Transfer Type | -- | -- | -- | TT0-TT1 | TT0-TT1 |
| Transfer Modifier | -- | -- | -- | TM0-TM2 | TM0-TM2 |
| Transfer Size | -- | SIZ0-SIZ1 | SIZ0-SIZ1 | SIZ0-SIZ1 | SIZ0-SIZ1 |
| Transfer In Progress | -- | -- | -- | TIP | TIP |
| Burst Inhibit | -- | -- | CBREQ/CBACK | TBI | TBI |
| Memory Inhibit | -- | -- | -- | MI | -- |
| Reset Output | RESET (bidir) | RESET (bidir) | RESET (bidir) | RSTO (output) | RSTO (output) |

---

## Appendix J. Amiga System CPU Clock Configurations

| System | CPU | CPU Clock | Chip Clock | Ratio | Fast RAM Bus | Chip RAM Bus |
|--------|-----|-----------|------------|-------|-------------|-------------|
| A1000 | 68000 | 7.09/7.16 MHz | Same | 1:1 | N/A | 16-bit sync |
| A500 | 68000 | 7.09/7.16 MHz | Same | 1:1 | N/A | 16-bit sync |
| A2000 | 68000 | 7.09/7.16 MHz | Same | 1:1 | N/A | 16-bit sync |
| A1200 | 68EC020 | 14.18/14.32 MHz | 7.09/7.16 | 2:1 | 32-bit | 16-bit async |
| A3000 | 68030 | 25 MHz | 7.09/7.16 | ~3.5:1 | 32-bit burst | 16-bit async |
| A4000/030 | 68030 | 25 MHz | 7.09/7.16 | ~3.5:1 | 32-bit | 16-bit via Buster |
| A4000/040 | 68040 | 25 MHz | 7.09/7.16 | ~3.5:1 | 32-bit sync | 16-bit via Buster |
| Accelerators | 68060 | 50-75 MHz | 7.09/7.16 | ~7-10:1 | 32-bit sync | 16-bit async |

**Buster/Ramsey:** The A4000 uses the Buster chip to bridge between the
68040's synchronous 32-bit bus and the chipset's 16-bit bus. Buster handles
dynamic bus sizing that the 68040 cannot do itself. The Ramsey chip provides
the DRAM controller for fast RAM.

**A1200 note:** The A1200 uses a 68EC020 at 2x the chip clock. Since the
EC020 supports dynamic bus sizing natively (via DSACK), it can talk to
the 16-bit chip bus directly without a bridge chip. However, chip RAM
accesses still take multiple CPU cycles due to the 16-bit bus width.

---

## Appendix K. CPU Feature Detection

Software can detect which CPU is present by testing for features that
differ between models. AmigaOS stores the CPU type in `SysBase->AttnFlags`.

### K.1 AttnFlags Bit Assignments (exec/execbase.h)

| Bit | Mask | Flag | Meaning |
|-----|------|------|---------|
| 0 | $0001 | AFF_68010 | 68010 or better |
| 1 | $0002 | AFF_68020 | 68020 or better |
| 2 | $0004 | AFF_68030 | 68030 or better |
| 3 | $0008 | AFF_68040 | 68040 or better |
| 4 | $0010 | AFF_68881 | 68881 FPU present |
| 5 | $0020 | AFF_68882 | 68882 FPU present |
| 6 | $0040 | AFF_FPU40 | 68040 on-chip FPU present |
| 7 | $0080 | AFF_68060 | 68060 or better |
| 8 | $0100 | AFF_PRIVATE | (private, do not use) |

### K.2 Detection Methods

**68010 detection:** Execute `MOVEC VBR,D0` in supervisor mode. If no
illegal instruction exception, it is a 68010+.

**68020 detection:** Attempt to use a full extension word. If the CPU
interprets it as a brief extension word, it is a 68000/68010.

**68030 detection:** Attempt `PMOVE TC,-(SP)`. If no exception, the on-chip
MMU is present (68030). If an exception occurs, it is a 68020.

**68040 detection:** Check for CINV instruction. Or check CACR -- writing
bit 15 (IE) and reading back will return the written value on 68040+
but not on 68020/68030.

**68060 detection:** Check for PCR register via `MOVEC PCR,D0`. Only the
68060 has PCR.

### K.3 Feature Matrix for Quick Reference

| Feature | 68000 | 68010 | 68020 | 68030 | 68040 | 68060 |
|---------|-------|-------|-------|-------|-------|-------|
| VBR | -- | Yes | Yes | Yes | Yes | Yes |
| MOVEC | -- | Yes | Yes | Yes | Yes | Yes |
| MOVES | -- | Yes | Yes | Yes | Yes | Yes |
| I-Cache | -- | -- | 256B | 256B | 4KB | 8KB |
| D-Cache | -- | -- | -- | 256B | 4KB | 8KB |
| MMU | -- | -- | Ext | On-chip | On-chip | On-chip |
| FPU | -- | -- | Ext | Ext | On-chip | On-chip |
| Copyback | -- | -- | -- | -- | Yes | Yes |
| Burst | -- | -- | -- | Yes | Yes | Yes |
| Superscalar | -- | -- | -- | -- | -- | Yes |
| Branch Pred | -- | -- | -- | -- | -- | Yes |
| Store Buffer | -- | -- | -- | -- | -- | Yes |
| Dynamic Bus | 16b | 16b | 8/16/32 | 8/16/32 | 32 only | 32 only |
| Bus Protocol | Async | Async | Async | Async | Sync | Sync |
| Addr Bus | 24 | 24 | 32 | 32 | 32 | 32 |
| Data Bus | 16 | 16 | 32 | 32 | 32 | 32 |

---

## Appendix L. Performance Scaling Across the Family

Approximate relative integer performance at stock Amiga clock rates.
Normalised to 68000 at 7.14 MHz = 1.0.

| CPU | Clock (MHz) | MIPS (approx) | Relative to 68000 | Notes |
|-----|-------------|---------------|-------------------|-------|
| 68000 | 7.14 | 0.7 | 1.0x | Baseline |
| 68010 | 7.14 | 0.8 | 1.1x | Loop mode helps |
| 68EC020 | 14.28 | 2.5 | 3.5x | A1200 stock |
| 68020 | 14.28 | 2.5 | 3.5x | Same as EC020 for integer |
| 68030 | 25 | 5.0 | 7x | A3000 stock |
| 68040 | 25 | 18 | 25x | Pipeline + fast multiply |
| 68060 | 50 | 100 | 140x | Superscalar + branch pred |

These are rough estimates for typical Amiga workloads (mix of memory access,
arithmetic, and branches). Actual performance varies dramatically based on
cache hit rates, memory speed, and instruction mix.

**Key performance cliffs:**
- 68020->68030: Modest gain (data cache + burst fills, similar core)
- 68030->68040: Huge gain (deep pipeline, hardware multiply, large caches)
- 68040->68060: Another large gain (superscalar, branch prediction, 2x clock)

---

*End of document.*

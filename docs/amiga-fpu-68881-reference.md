# Amiga FPU Reference: MC68881/MC68882

**Purpose:** Implementation reference for Amiga emulator authors adding
floating-point coprocessor support. Covers the MC68881/MC68882 instruction
set, register model, data formats, timing, exception handling, and
Amiga-specific integration.

**Audience:** Emulator authors who need correct FPU behaviour, not just
functional approximations. Supplements the companion
[Amiga 68000 Timing Reference](amiga-68000-timing.md), which covers the
CPU side.

**Sources:**
- Motorola MC68881/MC68882 Floating-Point Coprocessor User's Manual,
  Second Edition (Freescale/Prentice Hall) -- cited as `(MC68881UM)`
- Amiga NDK 3.9 Autodocs (mathieee*.library) -- cited as `(NDK 3.9)`
- Amiga Kickstart ROM internals -- cited as `(Kickstart Internals)`
- Community knowledge (WinUAE, UAE source)

---

## Table of Contents

1.  [Which Amigas Have FPUs](#1-which-amigas-have-fpus)
2.  [Coprocessor Interface](#2-coprocessor-interface)
3.  [FPU Register Set](#3-fpu-register-set)
4.  [FPCR -- Floating-Point Control Register](#4-fpcr----floating-point-control-register)
5.  [FPSR -- Floating-Point Status Register](#5-fpsr----floating-point-status-register)
6.  [Data Formats](#6-data-formats)
7.  [Instruction Set](#7-instruction-set)
8.  [ROM Constant Table (FMOVECR)](#8-rom-constant-table-fmovecr)
9.  [Exception Model](#9-exception-model)
10. [68040 and 68060 FPU Differences](#10-68040-and-68060-fpu-differences)
11. [Instruction Timing](#11-instruction-timing)
12. [Amiga Integration](#12-amiga-integration)

**Appendices:**
- [A. Instruction Timing Tables](#appendix-a-instruction-timing-tables)
- [B. ROM Constant Table (Complete)](#appendix-b-rom-constant-table-complete)
- [C. Conditional Predicate Encoding](#appendix-c-conditional-predicate-encoding)
- [D. Instruction Encoding Summary](#appendix-d-instruction-encoding-summary)
- [E. Gaps and Source Map](#appendix-e-gaps-and-source-map)

---

## 1. Which Amigas Have FPUs

### 1.1 FPU Presence by Model

| Amiga Model   | CPU        | FPU                     | FPU Clock   | Notes                                      |
|---------------|------------|-------------------------|-------------|---------------------------------------------|
| A1000         | 68000      | None                    | --          | No coprocessor interface on 68000           |
| A500/A500+    | 68000      | None                    | --          |                                             |
| A2000         | 68000      | Optional 68881/68882    | Varies      | Via accelerator board (A2620, GVP, etc.)    |
| A2500         | 68020      | Optional 68881          | 14 MHz      | A2620 accelerator                           |
| A3000         | 68030      | 68882 onboard           | 25 MHz      | Soldered to motherboard                     |
| A4000/030     | 68030      | Optional 68882          | 25 MHz      | Socket on CPU card                          |
| A4000/040     | 68040      | Built-in (subset)       | 25 MHz      | Integrated FPU; missing transcendentals     |
| A4000/060     | 68060      | Built-in (smaller subset)| 50 MHz     | Even fewer hardware FPU instructions        |
| A1200         | 68EC020    | None (stock)            | --          | 68881/68882 via accelerator (Blizzard, etc.)|
| CDTV          | 68000      | None                    | --          |                                             |
| CD32          | 68EC020    | None (stock)            | --          | Accelerator boards possible                 |

### 1.2 68881 vs 68882

The MC68881 and MC68882 are software-compatible: same instruction set,
same register model, same encoding. The MC68882 is faster because it
adds:

- **Concurrent execution of multiple FP instructions.** The MC68882 has a
  separate conversion unit (CU) that can prepare the next instruction while
  the arithmetic processing unit (APU) is busy. The MC68881 can only
  overlap FP execution with integer instructions on the CPU.
  `(MC68881UM SS1, SS5.1.1)`

- **Hardware-accelerated format conversion.** The MC68882 converts
  single/double memory operands to extended precision in its CU without
  involving the APU. `(MC68881UM SS1)`

- **Reduced coprocessor interface overhead.** Fewer bus handshake cycles
  for common operations. `(MC68881UM SS1)`

For emulation purposes, the only difference that matters is timing. The
instruction set, register model, exception behaviour, and data formats
are identical.

### 1.3 68040/68060 FPU

The 68040 has an integrated FPU that supports a subset of the
MC68881/68882 instruction set. All basic arithmetic (FADD, FSUB, FMUL,
FDIV, FSQRT, FABS, FNEG, FMOVE, FMOVEM, FCMP, FTST, FBcc, FScc,
FDBcc, FTRAPcc, FINT, FINTRZ, FSCALE, FGETEXP, FGETMAN, FSAVE,
FRESTORE) is in hardware. The transcendental and logarithmic
instructions must be software-emulated:

**Instructions missing from 68040 FPU hardware:**

FSIN, FCOS, FSINCOS, FTAN, FASIN, FACOS, FATAN, FATANH,
FSINH, FCOSH, FTANH, FETOX, FETOXM1, FTWOTOX, FTENTOX,
FLOGN, FLOGNP1, FLOG10, FLOG2, FMOD, FREM, FSGLDIV, FSGLMUL

When the 68040 encounters one of these instructions, it takes an F-line
exception (vector 11). The operating system's trap handler (installed by
the 68040.library on the Amiga) provides the software implementation.

The **68060** removes even more instructions from hardware -- it also
drops FSQRT (among others), requiring the 68060.library to handle those
traps. See [Section 10](#10-68040-and-68060-fpu-differences).

---

## 2. Coprocessor Interface

### 2.1 How the FPU Connects

The MC68881/MC68882 connects to the MC68020 or MC68030 via the M68000
coprocessor interface. This is not an internal subsystem -- it is a
separate chip that communicates through bus cycles in CPU address space.
`(MC68881UM SS1.1, SS7)`

Key points for emulation:

- **F-line instructions.** All FPU instructions start with the bit
  pattern `1111` in bits [15:12] of the operation word. The CPU
  recognises this as a coprocessor instruction and initiates a bus
  dialog with the addressed coprocessor. `(MC68881UM SS7.4)`

- **Coprocessor ID (cpID).** Bits [11:9] of the operation word select
  which coprocessor. The standard ID for the FPU is `001`.
  `(MC68881UM SS4.8.1)`

- **The FPU is not "inside" the CPU.** On a real A3000 with a 68030+68882,
  the CPU writes instruction data to coprocessor interface registers (CIRs)
  mapped into CPU space, then reads response primitives to find out what
  the FPU needs. This handshake involves real bus cycles.
  `(MC68881UM SS7.2)`

### 2.2 Coprocessor Interface Registers (CIRs)

The FPU occupies a block of CPU-space addresses decoded from function
codes FC2-FC0 = `111` (CPU space), address bits A19-A16 = `0010`
(coprocessor space type), and A15-A13 = cpID.
`(MC68881UM SS7.1, SS7.2)`

| Register              | Offset | Width | Type  | Purpose                                    |
|-----------------------|--------|-------|-------|--------------------------------------------|
| Response              | `$00`  | 16    | Read  | FPU returns service requests/status         |
| Control               | `$02`  | 16    | Write | CPU sends abort/exception acknowledge       |
| Save                  | `$04`  | 16    | Read  | Initiate FSAVE; returns format word         |
| Restore               | `$06`  | 16    | R/W   | Write format word for FRESTORE              |
| Operation Word        | `$08`  | 16    | R/W   | Not used by MC68881/MC68882                 |
| Command               | `$0A`  | 16    | Write | CPU writes command word to start instruction|
| Condition             | `$0E`  | 16    | Write | CPU writes condition predicate              |
| Operand               | `$10`  | 32    | R/W   | Data transfer between CPU and FPU           |
| Register Select       | `$14`  | 16    | Read  | Register mask for FMOVEM                    |
| Instruction Address   | `$18`  | 32    | Write | CPU passes PC for FPIAR                     |
| Operand Address       | `$1C`  | 32    | R/W   | Not used by MC68881/MC68882                 |

`(MC68881UM SS7.2, Table 7-2)`

### 2.3 Bus Protocol Summary

The basic instruction flow:

1. CPU encounters F-line word, decodes coprocessor type (general,
   conditional, save, restore).
2. CPU writes command word to Command CIR (or condition to Condition CIR).
3. CPU reads Response CIR. The response primitive tells the CPU what to do:
   - **Null (CA=1, IA=1):** FPU is busy; CPU may check for interrupts,
     then read Response CIR again.
   - **Null (CA=0):** Instruction complete; CPU proceeds to next
     instruction.
   - **Evaluate EA and transfer data:** CPU calculates effective address,
     transfers operand through Operand CIR.
   - **Transfer single MPU register:** CPU passes a data register.
   - **Transfer multiple coprocessor registers:** CPU reads register mask
     from Register Select CIR and transfers FP registers through
     Operand CIR.
   - **Take exception:** FPU requests an exception trap.
4. Steps 2-3 repeat as needed (come-again bit CA=1 means "read response
   again after performing the requested service").

`(MC68881UM SS7.4.1, SS7.4.2)`

### 2.4 Response Primitive Format

All FPU responses are 16-bit words read from the Response CIR:

```
Bit:  15  14  13  12  11  10   9   8   7   6   5   4   3   2   1   0
      +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
      |CA | PC| DR|          FUNCTION       |       PARAMETER          |
      +---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+---+
```

- **CA (bit 15):** Come Again. If set, the CPU must perform the requested
  service and then read the Response CIR again. If clear, the instruction
  dialog is complete.
- **PC (bit 14):** Pass Program Counter. If set, the CPU must write the
  current PC to the Instruction Address CIR ($18) before performing any
  other service. This updates the FPIAR for exception handling.
- **DR (bit 13):** Direction. 0 = CPU writes to FPU, 1 = FPU writes to CPU.

The six primitives used by the MC68881/MC68882:

1. **Null (CA=1, IA=1):** FPU is busy. CPU checks for pending interrupts
   (reducing interrupt latency) and reads Response CIR again. The IA bit
   is bit 8 in the response word.

2. **Null (CA=0):** Instruction complete. CPU proceeds to next instruction.
   The PF bit (bit 0) indicates whether the FPU has finished processing:
   PF=1 means fully complete (used in trace mode synchronisation).

3. **Evaluate EA and Transfer Data:** CPU calculates the effective
   address, transfers operand to/from the FPU through the Operand CIR.
   Contains the operand length, allowed addressing modes, and direction.

4. **Transfer Single MPU Register:** CPU transfers one data register
   value (used for passing the k-factor in packed decimal FMOVE).

5. **Transfer Multiple Coprocessor Registers:** CPU reads the register
   mask from the Register Select CIR and transfers FP data registers
   through the Operand CIR (used by FMOVEM).

6. **Take Exception:** FPU requests the CPU to initiate exception
   processing. Contains the vector number. Two forms:
   - Take Pre-Instruction Exception: stacked before the instruction
   - Take Mid-Instruction Exception: includes the effective address
     of the destination operand in the stack frame

`(MC68881UM SS7.4.2)`

### 2.5 Instruction Dialog Examples

**Register-to-register FMUL.X FP0,FP1:**
1. CPU writes command word to Command CIR ($0A)
2. CPU reads Response CIR: Null (CA=1, PC=1 if exceptions enabled)
   - CPU writes PC to Instruction Address CIR if PC bit set
3. CPU reads Response CIR: Null (CA=0)
   - Instruction complete; FPU calculates result concurrently

**Memory-to-register FADD.D (A0),FP2:**
1. CPU writes command word to Command CIR
2. CPU reads Response CIR: Evaluate EA and Transfer Data (CA=1, PC=x)
   - CPU writes PC if requested
   - CPU evaluates (A0), reads 8 bytes from memory
   - CPU writes data to Operand CIR (two long-word writes)
3. CPU reads Response CIR: Null (CA=0)
   - Instruction complete

**FSAVE -(SP):**
1. CPU evaluates effective address -(SP) from the F-line word
2. CPU reads Save CIR ($04): receives format word
   - If format word indicates "not ready," CPU re-reads Save CIR
3. CPU writes format word to memory at destination
4. If idle or busy frame: CPU reads state data from Operand CIR ($10)
   in long-word chunks and writes to memory
5. Instruction complete (no Response CIR read needed)

`(MC68881UM SS7.5)`

### 2.6 What This Means for Emulation

Most emulators do not need to emulate the bus handshake. Instead:

- Decode F-line words directly.
- Execute the FPU instruction semantically (read source, compute, write
  destination).
- Handle FPU exceptions synchronously.
- Implement FSAVE/FRESTORE state frames for context switching.

The bus protocol matters only for cycle-exact emulation or when emulating
the FPU as a truly separate device (relevant for accurate interrupt
latency during long FPU operations).

For cycle-exact emulation, the key timing consideration is that the FPU
releases the CPU (CA=0) after the operand transfer phase. During the
calculation phase, the CPU is free to execute integer instructions
concurrently. This overlap is significant: the CPU can execute 50-600+
clocks worth of integer instructions while the FPU computes a
transcendental function. The next FPU instruction will block (Null CA=1)
until the FPU finishes.

---

## 3. FPU Register Set

### 3.1 Floating-Point Data Registers (FP0-FP7)

Eight 80-bit registers, each holding one extended-precision value.
Analogous to the integer data registers D0-D7. All eight are
general-purpose -- any instruction can use any register.

Internal layout:

```
Bit:  79                           64 63                                0
      +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
      |S|    15-bit Exponent          |     64-bit Mantissa              |
      +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

- **S:** Sign bit (bit 79). 0 = positive, 1 = negative.
- **Exponent:** Bits [78:64]. 15-bit unsigned biased exponent,
  bias = 16383 ($3FFF).
- **Mantissa:** Bits [63:0]. 64-bit significand with explicit integer
  bit (bit 63). For normalised numbers, bit 63 = 1.

All internal operations use this format. External operands in other
formats are converted to extended precision before computation.
`(MC68881UM SS2.1, SS3.4)`

### 3.2 Control Registers

| Register | Width | Description                                    |
|----------|-------|------------------------------------------------|
| FPCR     | 32    | Floating-Point Control Register                |
| FPSR     | 32    | Floating-Point Status Register                 |
| FPIAR    | 32    | Floating-Point Instruction Address Register    |

These are described in detail in Sections 4 and 5.

`(MC68881UM SS2.2, SS2.3, SS2.4)`

### 3.3 FPIAR

The 32-bit Floating-Point Instruction Address Register holds the logical
address of the last FPU instruction that could generate an exception. It
is loaded before each arithmetic instruction executes (when exceptions
are enabled). FMOVE to/from control registers and FMOVEM do not modify
the FPIAR because they cannot generate FP exceptions.

The FPIAR is essential for exception handlers to locate the offending
instruction, because the CPU program counter may have advanced past it
due to concurrent execution. `(MC68881UM SS2.4)`

---

## 4. FPCR -- Floating-Point Control Register

The FPCR is a 32-bit register. Bits [31:16] are reserved (read as zero,
write as zero). The active fields are in the low 16 bits.

### 4.1 Bit Layout

```
Bit:  31                         16 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
      +---------------------------+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
      |        Reserved           |            ENABLE         |    PREC  |    RND   |
      |       (all zeros)         |BS|SN|OP|OV|UN|DZ|X2|X1|  |         |          |
      +---------------------------+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
```

`(MC68881UM SS2.2, Figures 2-2 and 2-3)`

### 4.2 Exception Enable Byte (Bits [15:8])

Each bit enables the trap for one class of FP exception. When a bit in
the FPSR exception status byte is set by the FPU *and* the corresponding
bit here is also set, the FPU signals an exception to the CPU.

| Bit | Name  | Exception Class                      | Priority |
|-----|-------|--------------------------------------|----------|
| 15  | BSUN  | Branch/Set on Unordered              | Highest  |
| 14  | SNAN  | Signalling Not-a-Number              |          |
| 13  | OPERR | Operand Error                        |          |
| 12  | OVFL  | Overflow                             |          |
| 11  | UNFL  | Underflow                            |          |
| 10  | DZ    | Divide by Zero                       |          |
|  9  | INEX2 | Inexact Operation                    |          |
|  8  | INEX1 | Inexact Decimal Input                | Lowest   |

When multiple exceptions occur with traps enabled for more than one
class, only the highest-priority exception is reported. The handler must
check for additional exceptions. `(MC68881UM SS2.2.1)`

Possible multiple exception combinations:
- SNAN and INEX1
- OPERR and INEX2
- OPERR and INEX1
- OVFL and INEX2 and/or INEX1
- UNFL and INEX2 and/or INEX1
- INEX2 and INEX1

### 4.3 Mode Control Byte (Bits [7:0])

| Bits  | Field | Values                                            |
|-------|-------|---------------------------------------------------|
| [7:6] | PREC  | `00` = Extended, `01` = Single, `10` = Double, `11` = Reserved |
| [5:4] | RND   | `00` = Round to Nearest (RN), `01` = Round to Zero (RZ), `10` = Round to Minus Infinity (RM), `11` = Round to Plus Infinity (RP) |
| [3:0] |       | Reserved (write as zero)                          |

`(MC68881UM SS2.2.2, Figure 2-3)`

**Rounding precision** selects where rounding of the mantissa occurs:
- Extended: round to 64-bit boundary
- Single: round to 24-bit boundary
- Double: round to 53-bit boundary

When single or double precision is selected, the result mantissa is
rounded to the selected precision and the exponent range is checked
against that format's limits, but the value is still stored in the
80-bit extended format in the FP register.

**Important:** Single and double precision modes significantly degrade
execution speed -- they exist for IEEE compliance emulation, not for
general use. `(MC68881UM SS2.2.2)`

**Reset default:** FPCR = $00000000 (extended precision, round to
nearest, all exceptions disabled). This matches the IEEE standard
defaults.

---

## 5. FPSR -- Floating-Point Status Register

The FPSR is a 32-bit register containing four fields.

### 5.1 Bit Layout

```
Bit:  31 30 29 28 27 26 25 24 23 22 21 20 19 18 17 16 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
      +--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
      | FPCC       |  QUOT  |  Quotient Bits     |     | EXC          |            |  AEXC     |       |
      | N  Z  I NAN| S      |  Q6-Q0             | Rsv |BS SN OP OV UN DZ X2 X1   | IO OV UN DZ IX    |
      +--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
```

`(MC68881UM SS2.3, Figures 2-4 through 2-7)`

### 5.2 Condition Code Byte (Bits [31:24])

| Bit | Name | Meaning                              |
|-----|------|--------------------------------------|
| 31  | N    | Negative (sign of result)            |
| 30  | Z    | Zero                                 |
| 29  | I    | Infinity                             |
| 28  | NAN  | Not-a-Number (or Unordered)          |

These are set at the end of all arithmetic instructions and FMOVE/FTST
to a single FP data register. They are *not* set by FMOVE FPm,<ea>,
FMOVEM, or FMOVE FPcr instructions.

Valid condition code combinations and their meanings:

| N | Z | I | NAN | Meaning                            |
|---|---|---|-----|------------------------------------|
| 0 | 0 | 0 |  0  | Positive normalised/denormalised   |
| 1 | 0 | 0 |  0  | Negative normalised/denormalised   |
| 0 | 1 | 0 |  0  | +0                                 |
| 1 | 1 | 0 |  0  | -0                                 |
| 0 | 0 | 1 |  0  | +Infinity                          |
| 1 | 0 | 1 |  0  | -Infinity                          |
| 0 | 0 | 0 |  1  | +NAN                               |
| 1 | 0 | 0 |  1  | -NAN                               |

Only these 8 combinations are generated by the FPU. Loading other
combinations via FMOVE to FPSR and executing a conditional instruction
produces undefined behaviour. `(MC68881UM SS2.3.1, Table 2-1)`

The IEEE conditions are derived as:
- EQ = Z
- GT = NOT(N) AND NOT(NAN) AND NOT(Z)
- LT = N AND NOT(NAN) AND NOT(Z)
- UN = NAN

`(MC68881UM SS2.3.1)`

### 5.3 Quotient Byte (Bits [23:16])

| Bit   | Name | Meaning                             |
|-------|------|-------------------------------------|
| 23    | S    | Sign of quotient                    |
| [22:16]| Q6-Q0| Seven least-significant bits of quotient (unsigned) |

Set by FMOD and FREM instructions only. Useful for argument reduction
in transcendental functions (seven bits are enough to determine the
quadrant). Remains set until explicitly cleared or until another
FMOD/FREM executes. `(MC68881UM SS2.3.2)`

### 5.4 Exception Status Byte (EXC, Bits [15:8])

Same bit positions as the FPCR enable byte:

| Bit | Name  | Exception Class              |
|-----|-------|------------------------------|
| 15  | BSUN  | Branch/Set on Unordered      |
| 14  | SNAN  | Signalling NAN               |
| 13  | OPERR | Operand Error                |
| 12  | OVFL  | Overflow                     |
| 11  | UNFL  | Underflow                    |
| 10  | DZ    | Divide by Zero               |
|  9  | INEX2 | Inexact Operation            |
|  8  | INEX1 | Inexact Decimal Input        |

Cleared at the start of most operations, then set if the corresponding
exception occurs during that operation. Operations that cannot generate
exceptions (FMOVEM, FMOVE FPcr) do not clear this byte.
`(MC68881UM SS2.3.3)`

### 5.5 Accrued Exception Byte (AEXC, Bits [7:0])

| Bit | Name | Maps from EXC bits                  |
|-----|------|-------------------------------------|
| 7-5 |      | Reserved                            |
|  4  | IOP  | Invalid Operation (BSUN or SNAN or OPERR) |
|  3  | OVFL | Overflow                            |
|  2  | UNFL | Underflow AND INEX2                 |
|  1  | DZ   | Divide by Zero                      |
|  0  | INEX | Inexact (INEX1 or INEX2 or OVFL)   |

These are "sticky" bits: ORed with the derived values after each
operation. The AEXC byte is never cleared by the FPU during normal
operations -- only by user writes to FPSR, hardware reset, or null state
restore. This allows polling for exceptions at the end of a computation
rather than after each instruction. `(MC68881UM SS2.3.4)`

The derivation equations:
```
AEXC(IOP)  = AEXC(IOP)  OR EXC(BSUN OR SNAN OR OPERR)
AEXC(OVFL) = AEXC(OVFL) OR EXC(OVFL)
AEXC(UNFL) = AEXC(UNFL) OR (EXC(UNFL) AND EXC(INEX2))
AEXC(DZ)   = AEXC(DZ)   OR EXC(DZ)
AEXC(INEX) = AEXC(INEX) OR EXC(INEX1 OR INEX2 OR OVFL)
```

`(MC68881UM SS2.3.4)`

---

## 6. Data Formats

The FPU supports seven external data formats. All are converted to
80-bit extended precision internally before any computation.
`(MC68881UM SS3)`

### 6.1 Format Summary

| Format             | Size    | Suffix | Exponent | Mantissa | Bias    |
|--------------------|---------|--------|----------|----------|---------|
| Byte Integer       | 8 bits  | .B     | --       | 8-bit signed 2's complement | --  |
| Word Integer       | 16 bits | .W     | --       | 16-bit signed 2's complement | -- |
| Long Integer       | 32 bits | .L     | --       | 32-bit signed 2's complement | -- |
| Single Precision   | 32 bits | .S     | 8 bits   | 23-bit fraction (1-bit implied) | 127 |
| Double Precision   | 64 bits | .D     | 11 bits  | 52-bit fraction (1-bit implied) | 1023 |
| Extended Precision | 96 bits | .X     | 15 bits  | 64-bit mantissa (explicit integer bit) | 16383 |
| Packed Decimal     | 96 bits | .P     | 3-digit BCD exponent | 17-digit BCD mantissa | -- |

`(MC68881UM SS3, SS3.1, SS3.2, SS3.3)`

### 6.2 IEEE 754 Binary Formats

**Single Precision (32-bit):**
```
Bit:  31  30       23  22                    0
      +---+---------+------------------------+
      | S | 8-bit   |   23-bit Fraction      |
      |   | Exponent|                        |
      +---+---------+------------------------+
```
Mantissa = 1.fraction (implicit leading 1 for normalised numbers).
Value = (-1)^S x 2^(e-127) x 1.fraction
`(MC68881UM SS3.2, Table 3-1)`

**Double Precision (64-bit):**
```
Bit:  63  62       52  51                                   0
      +---+---------+---------------------------------------+
      | S | 11-bit  |     52-bit Fraction                   |
      |   | Exponent|                                       |
      +---+---------+---------------------------------------+
```
Value = (-1)^S x 2^(e-1023) x 1.fraction
`(MC68881UM SS3.2, Table 3-2)`

**Extended Precision (96-bit in memory, 80 bits used):**
```
Bit:  95  94       80  79  78       64  63  62                    0
      +---+---------+---+-----------+---+--------------------------+
      | S | 15-bit  | 0 | (unused)  | J |   63-bit Fraction       |
      |   | Exponent|   | (16 bits) |   |                         |
      +---+---------+---+-----------+---+--------------------------+
```
J = explicit integer bit (bit 63 of the mantissa). For normalised
numbers, J = 1. The 16 unused bits between exponent and mantissa are
zero on output and don't-care on input.
Value = (-1)^S x 2^(e-16383) x J.fraction
`(MC68881UM SS3.2, Table 3-3)`

### 6.3 Special Values

For each binary format:

| Data Type     | Exponent        | Mantissa                          |
|---------------|-----------------|-----------------------------------|
| Normalised    | 0 < e < MAX     | Any bit pattern (J=1 for ext.)    |
| Denormalised  | e = 0           | Nonzero (J=0 for ext.)            |
| Zero          | e = 0           | All zeros                         |
| Infinity      | e = MAX         | All zeros (J = don't care for ext.)|
| NAN           | e = MAX         | Nonzero                           |

Two types of NAN:
- **Nonsignaling (quiet) NAN:** Most significant fraction bit = 1
- **Signaling NAN (SNAN):** Most significant fraction bit = 0

The FPU never creates an SNAN as a result. When an SNAN is used as input
with the SNAN trap disabled, the SNAN bit is set (converting it to a
quiet NAN) and the result is returned. `(MC68881UM SS3.2.5)`

FPU-created NANs always have all mantissa bits set to 1.

### 6.4 Packed Decimal Format

96 bits (12 bytes) in memory:

```
Bit:  95  94  93 92  91        80  79        64  63                    0
      +---+---+--+--+-----------+-----------+---+-----------------------+
      |SM |SE | YY | 3-digit    | 1-digit   |   16-digit Fraction       |
      |   |   |    | Exponent   | Integer   |                           |
      +---+---+--+--+-----------+-----------+---+-----------------------+
```

- SM = sign of mantissa (0 = positive, 1 = negative)
- SE = sign of exponent
- YY = 2 bits used only for infinity/NAN encoding (zero otherwise)
- Exponent: 3 BCD digits (EXP2, EXP1, EXP0), range 000-999
- Integer: 1 BCD digit (the integer part of the mantissa)
- Fraction: 16 BCD digits

For in-range numbers: value = (-1)^SM x integer.fraction x 10^((-1)^SE x exponent)

Special encodings:
- Infinity: SE=1, YY=11, exponent=$FFF, fraction=0
- NAN: SE=1, YY=11, exponent=$FFF, fraction=nonzero
- Zero: SM=0 or 1, exponent=000-999, integer=0, fraction=0

A fourth exponent digit (EXP3) is generated during move-out if the
binary-to-decimal conversion produces an exponent exceeding 999.
`(MC68881UM SS3.3, SS3.6, Figure 3-9, Table 3-4)`

### 6.5 Internal Format and Conversions

Internally, the FPU uses a 67-bit mantissa and a 17-bit two's-complement
exponent for intermediate results:

```
Intermediate Result Format:
  17-bit exponent | 67-bit mantissa
                  |
                  +-- bit 66: overflow bit
                  +-- bit 65: integer bit (J)
                  +-- bits 64-3: fraction
                  +-- bit 2: guard bit (g)
                  +-- bit 1: round bit (r)
                  +-- bit 0: sticky bit (s)
```

The three extra mantissa bits (guard, round, sticky) allow the FPU to
compute as if to infinite precision and then round correctly to the
destination format. The 17-bit exponent prevents intermediate overflow or
underflow during multiply/divide operations, simplifying detection of the
final result's range.

At the end of every operation, the intermediate result is:
1. Checked for underflow
2. Rounded to the selected precision (extended/single/double per FPCR)
3. Checked for overflow
4. Stored in the destination

All external operands -- regardless of format -- are converted to
extended precision before the specified operation begins. Single and
double precision inputs with denormalised values are normalised during
conversion. Extended precision unnormalised inputs (integer bit = 0 with
nonzero exponent) are normalised before use. `(MC68881UM SS3.4, SS3.5,
Figure 3-10)`

### 6.6 Format Ranges

| Format   | Max Positive Normalised | Min Positive Normalised | Min Positive Denormalised |
|----------|------------------------|------------------------|--------------------------|
| Single   | ~3.4 x 10^38           | ~1.2 x 10^-38          | ~1.4 x 10^-45            |
| Double   | ~1.8 x 10^308          | ~2.2 x 10^-308         | ~4.9 x 10^-324           |
| Extended | ~6 x 10^4931           | ~8 x 10^-4933          | ~9 x 10^-4952            |

`(MC68881UM SS3.6, Tables 3-1 through 3-3)`

---

## 7. Instruction Set

All FPU instructions are F-line instructions (bits [15:12] = `1111`).
They are grouped below by function. `(MC68881UM SS4)`

### 7.1 Data Movement

| Mnemonic | Syntax                     | Formats          | Operation                    |
|----------|----------------------------|------------------|------------------------------|
| FMOVE    | `<ea>,FPn`                 | B,W,L,S,D,X,P   | Source -> FPn                |
| FMOVE    | `FPm,FPn`                  | X                | FPm -> FPn                   |
| FMOVE    | `FPm,<ea>`                 | B,W,L,S,D,X     | FPm -> destination           |
| FMOVE    | `FPm,<ea>{#k}`             | P                | FPm -> packed decimal dest.  |
| FMOVE    | `FPm,<ea>{Dn}`             | P                | FPm -> packed decimal (dynamic k)|
| FMOVE    | `<ea>,FPcr`                | L                | Source -> FPCR/FPSR/FPIAR    |
| FMOVE    | `FPcr,<ea>`                | L                | FPCR/FPSR/FPIAR -> dest.     |
| FMOVECR  | `#ccc,FPn`                 | X                | ROM constant -> FPn          |
| FMOVEM   | `<ea>,<list>` / `<list>,<ea>` | L,X           | Multiple register transfer   |
| FMOVEM   | `<ea>,Dn` / `Dn,<ea>`     | X                | Dynamic register list        |

- FMOVE to FPn sets condition codes. FMOVE from FPn to memory does *not*
  set condition codes.
- FMOVE FPcr and FMOVEM do not affect condition codes or exception
  status bits and do not modify the FPIAR. They cannot generate FP
  exceptions.
- FMOVEM transfers extended-precision registers in FP7-FP0 order (or
  FP0-FP7 for control modes/postincrement). Each register is 12 bytes
  in memory (96-bit extended format).

`(MC68881UM SS4.2.1, Table 4-1)`

### 7.2 Dyadic Arithmetic (Two operands)

| Mnemonic | Operation                         |
|----------|-----------------------------------|
| FADD     | FPn + source -> FPn               |
| FSUB     | FPn - source -> FPn               |
| FMUL     | FPn x source -> FPn               |
| FDIV     | FPn / source -> FPn               |
| FMOD     | FPn MOD source -> FPn             |
| FREM     | FPn IEEE remainder source -> FPn  |
| FSCALE   | FPn x 2^source -> FPn             |
| FSGLDIV  | FPn / source -> FPn (single precision mantissa, extended exponent) |
| FSGLMUL  | FPn x source -> FPn (single precision mantissa, extended exponent) |
| FCMP     | FPn - source (set condition codes, no result stored) |

Syntax: `F<op>.<fmt> <ea>,FPn` or `F<op>.X FPm,FPn`

All formats (B,W,L,S,D,X,P) are supported for the source `<ea>` operand.
The result always goes to FPn. For FMOD and FREM, the quotient byte in
the FPSR is also set.

FSGLDIV and FSGLMUL round the mantissa to single precision but keep the
extended exponent range -- they cannot generate overflow/underflow within
the extended range. `(MC68881UM SS4.2.2, Tables 4-2, 4-3)`

### 7.3 Monadic Arithmetic (One operand)

| Mnemonic  | Operation                              | Category       |
|-----------|----------------------------------------|----------------|
| FABS      | |source| -> FPn                        | Arithmetic     |
| FNEG      | -source -> FPn                         | Arithmetic     |
| FSQRT     | sqrt(source) -> FPn                    | Arithmetic     |
| FINT      | Integer part of source -> FPn          | Arithmetic     |
| FINTRZ    | Integer part (round-to-zero) -> FPn    | Arithmetic     |
| FGETEXP   | Extract biased exponent -> FPn         | Arithmetic     |
| FGETMAN   | Extract mantissa -> FPn                | Arithmetic     |
| FSIN      | sin(source) -> FPn                     | Transcendental |
| FCOS      | cos(source) -> FPn                     | Transcendental |
| FTAN      | tan(source) -> FPn                     | Transcendental |
| FASIN     | arcsin(source) -> FPn                  | Transcendental |
| FACOS     | arccos(source) -> FPn                  | Transcendental |
| FATAN     | arctan(source) -> FPn                  | Transcendental |
| FATANH    | arctanh(source) -> FPn                 | Transcendental |
| FSINH     | sinh(source) -> FPn                    | Transcendental |
| FCOSH     | cosh(source) -> FPn                    | Transcendental |
| FTANH     | tanh(source) -> FPn                    | Transcendental |
| FETOX     | e^source -> FPn                        | Exponential    |
| FETOXM1   | e^source - 1 -> FPn                    | Exponential    |
| FTWOTOX   | 2^source -> FPn                        | Exponential    |
| FTENTOX   | 10^source -> FPn                       | Exponential    |
| FLOGN     | ln(source) -> FPn                      | Logarithmic    |
| FLOGNP1   | ln(source + 1) -> FPn                  | Logarithmic    |
| FLOG10    | log10(source) -> FPn                   | Logarithmic    |
| FLOG2     | log2(source) -> FPn                    | Logarithmic    |

Syntax: `F<op>.<fmt> <ea>,FPn` or `F<op>.X FPm,FPn` or `F<op>.X FPn`
(last form: source and destination are the same register).

**FSINCOS** is a special dual monadic instruction:
- Syntax: `FSINCOS.<fmt> <ea>,FPc:FPs` or `FSINCOS.X FPm,FPc:FPs`
- Computes sin(source) -> FPs and cos(source) -> FPc simultaneously.
- If FPc and FPs are the same register, cosine is stored (sine is lost).

`(MC68881UM SS4.2.3, Tables 4-4 through 4-6)`

### 7.4 Individual Instruction Details

The following provides per-instruction notes relevant to emulation.
For each instruction, the operation table defines what result data type
is produced for each combination of input data types. If a NAN is input
to any arithmetic instruction, the NAN propagation rules apply (see
Section 7.4.1). `(MC68881UM SS4.5, SS4.6)`

**FADD** -- Floating-Point Add
- Operation: FPn + source -> FPn
- (+inf) + (-inf) = OPERR (NAN result)
- (+0) + (-0) = +0 in RN/RZ/RP, -0 in RM
- Sets all condition codes. IEEE-compliant (1/2 ULP accuracy in RN).

**FSUB** -- Floating-Point Subtract
- Operation: FPn - source -> FPn
- Same-sign infinities -> OPERR (NAN result)
- (+0) - (+0) = +0 in RN/RZ/RP, -0 in RM

**FMUL** -- Floating-Point Multiply
- Operation: FPn x source -> FPn
- 0 x infinity = OPERR (NAN result)
- Sign of result: XOR of operand signs (even for special values).

**FDIV** -- Floating-Point Divide
- Operation: FPn / source -> FPn
- 0/0 or inf/inf = OPERR (NAN result)
- nonzero/0 = DZ (correctly-signed infinity result)
- 0/nonzero = correctly-signed zero

**FMOD** -- Modulo Remainder
- Operation: FPn - (N x source) -> FPn, where N is the integer nearest
  to FPn/source (rounded to zero).
- Sets quotient byte in FPSR (sign + 7 LSBs of |quotient|).
- OPERR if FPn is infinity or source is zero.
- Useful for argument reduction (the quotient tells which "period"
  the original value was in).

**FREM** -- IEEE Remainder
- Operation: FPn - (N x source) -> FPn, where N is the integer nearest
  to FPn/source (rounded to nearest, ties to even).
- Otherwise identical to FMOD in exception behaviour.
- The IEEE remainder can produce results with magnitude up to
  |source|/2.

**FSCALE** -- Scale Exponent
- Operation: FPn x 2^INT(source) -> FPn
- The source is truncated to an integer before use.
- OPERR if source is infinity.
- Efficient way to multiply/divide by powers of 2.

**FSQRT** -- Square Root
- OPERR if source < 0 (NAN result).
- sqrt(-0) = -0, sqrt(+0) = +0, sqrt(+inf) = +inf.
- IEEE-compliant (1/2 ULP in RN).

**FABS / FNEG** -- Absolute Value / Negate
- FABS: |source| -> FPn. Clears the sign bit.
- FNEG: -source -> FPn. Inverts the sign bit.
- Both are very fast (4 cycles internal calculation on MC68881).

**FINT** -- Extract Integer Part
- Rounds source to an integer using the current FPCR rounding mode.
- Result is still in floating-point format (not an integer type).
- FINT of 2.7 in RN mode = 3.0; in RZ mode = 2.0.

**FINTRZ** -- Extract Integer Part, Round-to-Zero
- Always uses round-to-zero regardless of the FPCR rounding mode.
- Equivalent to mathematical truncation.

**FGETEXP** -- Get Exponent
- Extracts the unbiased exponent of the source as an extended-precision
  floating-point integer.
- FGETEXP of 3.14159 = 1.0 (since 3.14159 = 1.5708 x 2^1).
- OPERR if source is infinity.

**FGETMAN** -- Get Mantissa
- Extracts the mantissa (significand) of the source, with the exponent
  set to produce a value in the range [1.0, 2.0).
- OPERR if source is infinity.

**FCMP** -- Floating-Point Compare
- Operation: FPn - source (set condition codes, discard result).
- No result is stored. FPn is not modified.
- Sets N, Z, I, NAN condition codes based on the difference.

**FTST** -- Test Floating-Point Operand
- Sets condition codes based on the source operand value.
- No computation; no result stored.

**FSIN / FCOS / FTAN** -- Trigonometric Functions
- Input: angle in radians.
- OPERR if source is infinity.
- For source values outside [-2pi, +2pi], internal argument reduction
  is performed using FMOD/FREM, which adds to execution time.
- Accuracy: worst case 1 ULP in double precision (4096 ULP extended).
  Typical: ~64 ULP in extended.

**FSINCOS** -- Simultaneous Sine and Cosine
- Computes both sin and cos in one instruction, roughly 20% faster than
  separate FSIN + FCOS.
- If FPc = FPs (same destination register), cosine overwrites sine.

**FASIN / FACOS / FATAN** -- Inverse Trigonometric
- FASIN/FACOS: OPERR if |source| > 1 or source is infinity.
- FATAN: defined for all finite inputs; FATAN(+inf) = +pi/2,
  FATAN(-inf) = -pi/2.

**FATANH** -- Inverse Hyperbolic Tangent
- OPERR if |source| > 1 or source is infinity.
- DZ if source = +1 or -1 (result is infinity).

**FSINH / FCOSH / FTANH** -- Hyperbolic Functions
- FSINH: defined for all finite inputs; sinh(+inf) = +inf.
- FCOSH: defined for all finite inputs; cosh(+inf) = +inf.
- FTANH: defined for all finite inputs; approaches +/-1 for large inputs.

**FETOX / FETOXM1** -- Exponential Functions
- FETOX: e^source -> FPn. FETOX(0) = 1.0, FETOX(+inf) = +inf.
- FETOXM1: e^source - 1 -> FPn. More accurate than FETOX for small
  source values (avoids catastrophic cancellation).

**FTWOTOX / FTENTOX** -- Power-of-2 and Power-of-10
- FTWOTOX: 2^source -> FPn.
- FTENTOX: 10^source -> FPn.
- These do not check for exact integer inputs, so FTENTOX #1 may not
  produce exactly 10.0. INEX2 may be set even for exact results.

**FLOGN / FLOGNP1** -- Natural Logarithm
- FLOGN: ln(source) -> FPn. OPERR if source < 0.
  DZ if source = 0 (result = -inf).
- FLOGNP1: ln(source + 1) -> FPn. More accurate than FLOGN for source
  values near zero. OPERR if source < -1. DZ if source = -1.

**FLOG10 / FLOG2** -- Common and Binary Logarithm
- FLOG10: log10(source) -> FPn. Same exception conditions as FLOGN.
- FLOG2: log2(source) -> FPn. Same exception conditions as FLOGN.

**FSGLDIV / FSGLMUL** -- Single-Precision Divide/Multiply
- Perform the operation with the mantissa rounded to single precision
  (24 bits) but keeping the full extended-precision exponent range.
- Faster than FDIV/FMUL (fewer mantissa bits to compute).
- Cannot overflow/underflow within the extended exponent range.
- The result mantissa is only accurate to single precision even though
  the result is stored in extended format.

#### 7.4.1 NAN Propagation Rules

When NANs are inputs to arithmetic operations:
- If one operand is a nonsignaling NAN and the other is not a NAN:
  the NAN is returned as the result.
- If both operands are nonsignaling NANs: the destination operand NAN
  is returned.
- If either operand is a signaling NAN (SNAN): the SNAN bit in FPSR EXC
  is set. If the SNAN trap is enabled, the trap is taken and the
  destination is not modified. If disabled, the SNAN is converted to a
  nonsignaling NAN (by setting the most significant fraction bit) and
  the above rules apply.

`(MC68881UM SS4.5.4)`

### 7.5 Transcendental Accuracy

Arithmetic instructions (FADD, FSUB, FMUL, FDIV, FSQRT, FREM, FMOD,
FABS, FNEG, FINT, FINTRZ, FGETEXP, FGETMAN, FSCALE, FCMP, FTST, FMOVE)
are accurate to one-half ULP in round-to-nearest mode and one ULP in
other modes, as required by IEEE 754.

Transcendental and exponential/logarithmic functions are *not* covered by
IEEE 754 accuracy requirements. Worst case accuracy is one ULP in double
precision (= 4096 ULP in extended precision). Typical error is about
64 ULP in extended precision (6 bits of the 64-bit mantissa).
`(MC68881UM SS4.3.1, SS4.3.2)`

### 7.6 Program Control

| Mnemonic  | Syntax               | Operation                               |
|-----------|-----------------------|-----------------------------------------|
| FBcc      | `<label>`            | Branch if condition true (16 or 32-bit displacement) |
| FDBcc     | `Dn,<label>`         | Test, decrement and branch              |
| FScc      | `<ea>`               | Set byte to all-1s if true, all-0s if false |
| FTRAPcc   | `#xxx` / none        | Trap if condition true                  |
| FTST      | `<ea>` / `FPn`       | Set FPSR condition codes                |
| FNOP      | (none)               | No operation; forces synchronisation    |

32 conditional tests are available, grouped as:
- **IEEE nonaware (16):** Set BSUN if NAN condition code is set (except
  EQ and NE). Used for porting code from non-IEEE systems.
- **IEEE aware (16):** Never set BSUN. Used for programs that explicitly
  handle unordered conditions.

`(MC68881UM SS4.2.4, SS4.4, Tables 4-7 and 4-8)`

### 7.7 System Control

| Mnemonic  | Syntax         | Operation                          | Privilege |
|-----------|----------------|------------------------------------|-----------|
| FSAVE     | `<ea>`         | Save FPU internal state to memory  | Supervisor|
| FRESTORE  | `<ea>`         | Restore FPU internal state         | Supervisor|

These are privileged instructions. FSAVE suspends any in-progress
operation and writes the internal state frame. FRESTORE loads a
previously saved state. Both are essential for multitasking context
switches. `(MC68881UM SS4.2.5, Table 4-9)`

### 7.8 Instruction Encoding

All general-type FPU instructions use a two-word format:

```
Word 1 (Operation Word):
  15 14 13 12  11 10  9  8  7  6  5  4  3  2  1  0
  1  1  1  1  | cpID  |  TYPE  |  TYPE-DEPENDENT     |

Word 2 (Command Word):
  15 14 13  12 11 10  9  8  7  6  5  4  3  2  1  0
  |OPCLASS|  RX      |  RY      |   EXTENSION        |
```

cpID = `001` for the FPU. TYPE field selects the instruction type:
- `000` = General (arithmetic, moves)
- `001` = FDBcc / FScc / FTRAPcc
- `010` = FBcc.W
- `011` = FBcc.L
- `100` = FSAVE
- `101` = FRESTORE

For general instructions, OPCLASS determines the operation class:
- `000` = Register to register
- `010` = External operand to register (or FMOVECR when RX=111)
- `011` = Register to external destination
- `100` = Move to system control register(s)
- `101` = Move from system control register(s)
- `110` = FMOVEM memory to FP data registers
- `111` = FMOVEM FP data registers to memory

The EXTENSION field specifies the arithmetic operation:

| Code | Instruction | Code | Instruction |
|------|-------------|------|-------------|
| $00  | FMOVE       | $18  | FABS        |
| $01  | FINT        | $19  | FCOSH       |
| $02  | FSINH       | $1A  | FNEG        |
| $03  | FINTRZ      | $1C  | FACOS       |
| $04  | FSQRT       | $1D  | FCOS        |
| $06  | FLOGNP1     | $1E  | FGETEXP     |
| $08  | FETOXM1     | $1F  | FGETMAN     |
| $09  | FTANH       | $20  | FDIV        |
| $0A  | FATAN       | $21  | FMOD        |
| $0C  | FASIN       | $22  | FADD        |
| $0D  | FATANH      | $23  | FMUL        |
| $0E  | FSIN        | $24  | FSGLDIV     |
| $0F  | FTAN        | $25  | FREM        |
| $10  | FETOX       | $26  | FSCALE      |
| $11  | FTWOTOX     | $27  | FSGLMUL     |
| $12  | FTENTOX     | $28  | FSUB        |
| $14  | FLOGN       | $30-$37 | FSINCOS |
| $15  | FLOG10      | $38  | FCMP        |
| $16  | FLOG2       | $3A  | FTST        |

Source format field (for external operand instructions):

| Code | Format           | Size    |
|------|------------------|---------|
| 000  | Long Integer     | 4 bytes |
| 001  | Single Precision | 4 bytes |
| 010  | Extended Precision| 12 bytes|
| 011  | Packed Decimal   | 12 bytes|
| 100  | Word Integer     | 2 bytes |
| 101  | Double Precision | 8 bytes |
| 110  | Byte Integer     | 1 byte  |

`(MC68881UM SS4.7, SS4.8, Tables 4-11 through 4-14)`

---

## 8. ROM Constant Table (FMOVECR)

The FMOVECR instruction loads a constant from the FPU's on-chip ROM
into a floating-point data register, rounded to the precision selected
in the FPCR mode control byte. The constant is selected by a 7-bit
offset in the command word. `(MC68881UM SS4.6, FMOVECR description)`

| Offset | Constant        | Approximate Value                    |
|--------|-----------------|--------------------------------------|
| $00    | Pi              | 3.14159265358979323846...            |
| $0B    | Log10(2)        | 0.30102999566398119521...            |
| $0C    | e               | 2.71828182845904523536...            |
| $0D    | Log2(e)         | 1.44269504088896340736...            |
| $0E    | Log10(e)        | 0.43429448190325182765...            |
| $0F    | 0.0             | 0.0                                  |
| $30    | ln(2)           | 0.69314718055994530941...            |
| $31    | ln(10)          | 2.30258509299404568402...            |
| $32    | 10^0            | 1.0                                  |
| $33    | 10^1            | 10.0                                 |
| $34    | 10^2            | 100.0                                |
| $35    | 10^4            | 10000.0                              |
| $36    | 10^8            | 1.0 x 10^8                           |
| $37    | 10^16           | 1.0 x 10^16                          |
| $38    | 10^32           | 1.0 x 10^32                          |
| $39    | 10^64           | 1.0 x 10^64                          |
| $3A    | 10^128          | 1.0 x 10^128                         |
| $3B    | 10^256          | 1.0 x 10^256                         |
| $3C    | 10^512          | 1.0 x 10^512                         |
| $3D    | 10^1024         | 1.0 x 10^1024                        |
| $3E    | 10^2048         | 1.0 x 10^2048                        |
| $3F    | 10^4096         | 1.0 x 10^4096                        |

The ROM contains additional constants used by internal microcode
routines. Offsets not listed above are reserved; their values may differ
between mask revisions. `(MC68881UM, FMOVECR description)`

**Emulation note:** The 68040 and 68060 do not support FMOVECR in
hardware. It must be software-emulated (typically by the F-line trap
handler). The constant values should be stored exactly as per the
MC68881/MC68882 ROM.

---

## 9. Exception Model

### 9.1 FPU Exception Classes

The FPU detects eight classes of exceptions, which map to dedicated
vectors in the M68000 exception vector table:

| Vector | Offset  | Exception                   |
|--------|---------|-----------------------------|
| 48     | $0C0    | BSUN (Branch/Set on Unordered) |
| 49     | $0C4    | Inexact Result              |
| 50     | $0C8    | Divide by Zero              |
| 51     | $0CC    | Underflow                   |
| 52     | $0D0    | Operand Error               |
| 53     | $0D4    | Overflow                    |
| 54     | $0D8    | Signaling NAN               |

Additional vectors used:
| 7      | $01C    | FTRAPcc instruction         |
| 11     | $02C    | F-line emulator             |
| 13     | $034    | Coprocessor protocol violation |

`(MC68881UM SS6, Table 6-1)`

### 9.2 Exception Reporting

When an exception occurs:
1. The corresponding bit in the FPSR EXC byte is set.
2. If the matching FPCR ENABLE bit is also set, the FPU signals an
   exception to the CPU via a "take exception" response primitive.
3. The CPU stacks an exception frame and vectors to the handler.

If the trap is disabled, the FPU provides a default result:
- **BSUN (disabled):** Condition is evaluated normally.
- **SNAN (disabled):** SNAN converted to quiet NAN; result stored.
- **OPERR (disabled):** NAN stored for FP register destinations;
  largest integer for integer destinations.
- **OVFL (disabled):** Result depends on rounding mode (infinity for RN,
  largest magnitude number for RZ, etc.).
- **UNFL (disabled):** Denormalised number or zero stored.
- **DZ (disabled):** Correctly-signed infinity stored.
- **INEX (disabled):** Rounded result stored.

`(MC68881UM SS6.1)`

### 9.3 Pre-instruction vs Mid-instruction Exceptions

- **Pre-instruction:** Exception is reported when the CPU initiates the
  *next* FPU instruction (reads response CIR and gets "take exception").
  The offending instruction's result may or may not be stored depending on
  the exception type. The MC68881 reports most arithmetic exceptions this
  way.

- **Mid-instruction:** Exception is reported during the current
  instruction's bus dialog. The CPU stacks a mid-instruction exception
  frame (format $3, 10 words on 68020/68030) that includes the effective
  address of the destination operand. FMOVE FPn,<ea> exceptions are always
  mid-instruction.

`(MC68881UM SS6.1, SS6.2)`

### 9.4 Operand Error Conditions

The following operations generate OPERR:

| Instruction | Condition                                        |
|-------------|--------------------------------------------------|
| FADD        | (+inf) + (-inf) or (-inf) + (+inf)               |
| FSUB        | Same-sign infinities                             |
| FMUL        | 0 x infinity                                     |
| FDIV        | 0/0 or inf/inf                                   |
| FSQRT       | Source < 0 or source = -infinity                 |
| FACOS/FASIN | Source > +1, < -1, or +/-infinity                |
| FATAN       | --                                               |
| FATANH      | Source > +1 or < -1                              |
| FCOS/FSIN/FTAN | Source = +/-infinity                          |
| FSINCOS     | Source = +/-infinity                             |
| FLOG10/FLOG2/FLOGN | Source < 0 or source = -infinity           |
| FLOGNP1     | Source < -1 or source = -infinity                |
| FGETEXP/FGETMAN | Source = +/-infinity                         |
| FMOD/FREM   | FPn = +/-infinity or source = 0                  |
| FSCALE      | Source = +/-infinity                             |
| FMOVE to B/W/L | Integer overflow, NAN, or infinity source    |
| FMOVE to P  | Exponent > 999 or k-factor > +17                |

`(MC68881UM SS6.1.3, Table 6-2)`

### 9.5 Overflow Exception Details

Overflow occurs when the intermediate result exponent exceeds the
maximum for the selected rounding precision (or destination format for
memory stores).

**Trap disabled results** depend on the rounding mode:

| Rounding Mode | Positive Overflow Result | Negative Overflow Result |
|---------------|--------------------------|--------------------------|
| RN (Nearest)  | +Infinity                | -Infinity                |
| RZ (Zero)     | Largest positive number  | Largest negative number  |
| RM (Minus Inf)| Largest positive number  | -Infinity                |
| RP (Plus Inf) | +Infinity                | Largest negative number  |

**Trap enabled results:** The OVFL result is stored as above, and a take
exception primitive is returned. For FP register destinations, the
exceptional operand in the FSAVE state frame has its exponent biased by
an additional -$6000 to "wrap" the 17-bit intermediate exponent into
15 bits. To recover the true exponent: sign-extend the 15-bit value to
at least 17 bits, then subtract the bias ($3FFF - $6000).

Exponential instructions (FETOX, FTENTOX, FTWOTOX, FSINH, FCOSH,
FSCALE) can generate "catastrophic overflow" where even the 17-bit
intermediate exponent overflows. In this case, the exceptional operand
exponent is set to $0000. `(MC68881UM SS6.1.4)`

### 9.6 Underflow Exception Details

Underflow occurs when the intermediate result exponent is less than or
equal to the minimum for the selected rounding precision.

**Trap disabled results:** The FPU denormalises the intermediate result
by shifting the mantissa right while incrementing the exponent until it
reaches the minimum value. If all significant bits are shifted out, the
rounding mode determines whether the result is +0, -0, or the smallest
denormalised number. The INEX2 bit is always set along with UNFL when
underflow occurs.

Note: the AEXC(UNFL) bit is set only when both UNFL and INEX2 are set
in the EXC byte. This prevents "exact" underflows (which produce exact
denormalised results) from being flagged in the accrued byte.
`(MC68881UM SS6.1.5)`

### 9.7 Divide by Zero Details

DZ occurs when a nonzero finite number is divided by zero (FDIV), or
when the logarithm of zero is taken (FLOGN, FLOG10, FLOG2, FLOGNP1
with source = -1).

**Trap disabled result:** Correctly-signed infinity.

Note: 0/0 is an OPERR, not a DZ. `(MC68881UM SS6.1.6)`

### 9.8 Inexact Result Details

Two inexact exception classes exist:

- **INEX2 (Inexact Operation):** The result of an arithmetic operation
  or output conversion was rounded because it could not be exactly
  represented in the destination format. This is by far the most common
  exception; most transcendental operations set it.

- **INEX1 (Inexact Decimal Input):** A packed decimal input operand
  could not be exactly converted to extended precision binary format.

These are the lowest-priority exceptions. For many applications they
are left disabled and the AEXC(INEX) sticky bit is checked once at the
end of a computation. `(MC68881UM SS6.1.7, SS6.1.8)`

### 9.9 FSAVE/FRESTORE and State Frames

The FSAVE instruction saves the FPU's non-user-visible internal state to
memory. Three frame formats exist:

**Null State Frame (4 bytes):**
```
+---+---+---+---+
| $00 | (undef) | (reserved) |
+---+---+---+---+
```
Generated when no FPU instruction has executed since the last reset or
null-state restore. Version number = 0 (wild card). Restoring a null
frame performs a reset: all FP registers loaded with quiet NANs, FPCR
and FPSR cleared to zero.

**Idle State Frame:**
- MC68881: 28 bytes total (4 format + 24 internal state)
- MC68882: 60 bytes total (4 format + 56 internal state)

Contains: command/condition register image, exceptional operand (12
bytes, extended precision), operand register, BIU flags. The MC68882
adds 32 bytes of CU internal state.

**Busy State Frame:**
- MC68881: 184 bytes total (4 format + 180 internal state)
- MC68882: 216 bytes total (4 format + 212 internal state)

Generated when FSAVE interrupts a computation in progress. Contains the
full internal state needed to resume the operation after FRESTORE.

The format word (first word of any frame) contains:
- Byte 0: Version number (identifies the microcode revision)
- Byte 1: State size in bytes (of internal data, excluding format word)

Version $00 in the format word is a "wild card" accepted by any revision.

`(MC68881UM SS6.4, SS6.4.2, Figures 6-4 and 6-5)`

### 9.10 Context Switching Sequence

To save an FPU context (e.g., on a task switch):
1. `FSAVE -(SP)` -- saves internal state, enters idle mode, clears
   pending exceptions
2. `FMOVEM FP0-FP7,-(SP)` -- saves all data registers
3. `FMOVEM FPCR/FPSR/FPIAR,-(SP)` -- saves control registers

To restore:
1. `FMOVEM (SP)+,FPCR/FPSR/FPIAR`
2. `FMOVEM (SP)+,FP0-FP7`
3. `FRESTORE (SP)+` -- restores internal state, resumes any interrupted
   operation

`(MC68881UM SS6.4, SS6.4.5)`

---

## 10. 68040 and 68060 FPU Differences

### 10.1 68040 FPU

The 68040 integrates an FPU that handles the IEEE-required subset of
instructions in hardware. The following instructions are *not*
implemented in 68040 FPU hardware and generate F-line exceptions
(vector 11) when executed:

**Transcendental/logarithmic/exponential:**
FSIN, FCOS, FSINCOS, FTAN, FASIN, FACOS, FATAN, FATANH,
FSINH, FCOSH, FTANH, FETOX, FETOXM1, FTWOTOX, FTENTOX,
FLOGN, FLOGNP1, FLOG10, FLOG2

**Other:**
FMOD, FREM, FSGLDIV, FSGLMUL, FMOVECR

On the Amiga, the **68040.library** (loaded during boot) installs
F-line exception handlers that provide software implementations of
these instructions. Alternatively, the **fpsp.resource** (Floating
Point Support Package) can be linked in for faster emulation that
avoids full exception processing overhead. `(NDK 3.9, mathieeesingbas.library --Background--)`

### 10.2 68060 FPU

The 68060 removes even more instructions from hardware. In addition to
everything the 68040 is missing, the 68060 also requires software
emulation for FSQRT and other operations. The **68060.library** handles
this.

### 10.3 Complete List of 68040 Hardware vs Software Instructions

For clarity, here is the full breakdown of 68040 FPU support:

**Hardware-implemented (execute natively):**
FABS, FADD, FBcc, FCMP, FDBcc, FDIV, FINT, FINTRZ, FMOVE,
FMOVEM, FMUL, FNEG, FNOP, FRESTORE, FSAVE, FSCALE, FScc,
FSQRT, FSUB, FTRAPcc, FTST, FGETEXP, FGETMAN

**Software-emulated (F-line trap on 68040):**
FACOS, FASIN, FATAN, FATANH, FCOS, FCOSH, FETOX, FETOXM1,
FLOG10, FLOG2, FLOGN, FLOGNP1, FMOD, FMOVECR, FREM,
FSGLDIV, FSGLMUL, FSIN, FSINCOS, FSINH, FTAN, FTANH,
FTENTOX, FTWOTOX

The 68040 also has some differences in FMOVE behaviour: FMOVE of
denormalised numbers and FMOVE with packed decimal format may take
the unimplemented instruction exception on the 68040.

### 10.4 68060 Additional Restrictions

The 68060 removes even more from hardware. In addition to everything
the 68040 software-emulates, the 68060 also requires software for:
- FSQRT (hardware on 68040, software on 68060)
- Various edge cases of FDIV

The 68060.library provides all necessary trap handlers. The practical
effect: software compiled for the 68881/68882 instruction set runs on
all Amiga CPUs, but with different performance characteristics. On the
68060, transcendentals and square root are slower (software emulation)
but basic arithmetic is much faster due to the 68060's superscalar
pipeline.

### 10.5 State Frame Differences

The 68040 and 68060 have different FSAVE/FRESTORE state frame formats
from the 68881/68882:
- 68040: Null frame (4 bytes), Idle frame (4 bytes), Unimplemented frame,
  Busy frame (varies)
- 68060: Similar but with its own version numbers and sizes

Emulators must handle the correct state frame format for the CPU being
emulated. The format word's version byte identifies the originating
device.

### 10.6 AttnFlags Detection

On the Amiga, `AttnFlags` in ExecBase ($128) indicates what FPU is
present. From `execbase.h` `(Kickstart Internals SS4.5)`:

```c
#define AFB_68881   4   /* also set for 68882 */
#define AFB_68882   5
#define AFB_FPU40   6   /* Set if 68040 FPU */

#define AFF_68881   (1L<<4)   /* $10 */
#define AFF_68882   (1L<<5)   /* $20 */
#define AFF_FPU40   (1L<<6)   /* $40 */
```

The convention is "inclusive" for the 68881 flag: if a 68882 is present,
*both* `AFB_68881` and `AFB_68882` are set. On a bare 68040:
- `AFB_FPU40` is set
- `AFB_68881` and `AFB_68882` are clear (because the 68040 FPU is not
  fully 68881-compatible)
- After the 68040.library loads and installs software emulation,
  `AFB_68881` and `AFB_68882` may be set retroactively

`(Kickstart Internals SS4.5)`

---

## 11. Instruction Timing

### 11.1 General Timing Structure

FPU instruction execution has several phases:
1. **Instruction start-up:** CPU decodes F-line word, writes command/
   condition CIR, reads first response primitive. (CPU-dependent)
2. **Operand transfer:** If the source is in memory, the CPU evaluates
   the effective address and transfers the operand. (CPU-dependent)
3. **Input conversion:** FPU converts the operand to extended precision.
4. **Calculation:** FPU performs the arithmetic operation.
5. **Round/store result:** FPU rounds the result and stores it.
6. **Instruction termination:** CPU processes the final null primitive.

Phases 1-2 and 6 depend on the CPU; phases 3-5 depend on the FPU.
Concurrent execution allows phases to overlap.
`(MC68881UM SS8, SS8.1)`

### 11.2 Timing Assumptions

The timing tables assume:
- MC68020 as the main processor, same clock as the FPU
- 32-bit memory interface, zero wait states
- No instruction cache hits (worst case for prefetch)
- CIR accesses: 3 clocks (5 for Response and Save CIR reads)
- Default rounding mode (round-to-nearest, extended precision)
- No exceptions enabled, no exceptions occurring
- Typical normalised input operands

All times in FPU clock cycles. `(MC68881UM SS8.5.1)`

### 11.3 Key Timing Observations

**Fast instructions (MC68881, FPn to FPm):**
- FMOVECR: 29 cycles
- FMOVE: 33 cycles
- FCMP: 33 cycles
- FTST: 33 cycles
- FABS, FNEG: 35 cycles
- FSCALE: 41 cycles
- FADD, FSUB: 51 cycles

**Medium instructions (MC68881, FPn to FPm):**
- FSGLMUL: 59 cycles
- FSGLDIV: 69 cycles
- FMOD: 70 cycles
- FMUL: 71 cycles
- FREM: 100 cycles
- FDIV: 103 cycles
- FSQRT: 107 cycles

**Slow instructions -- transcendentals (MC68881, FPn to FPm):**
- FSIN, FCOS: 391 cycles
- FATAN: 403 cycles
- FSINCOS: 461 cycles
- FTAN: 473 cycles
- FETOX: 497 cycles
- FTENTOX, FTWOTOX: 567 cycles
- FASIN: 581 cycles
- FLOG10, FLOG2: 581 cycles
- FACOS: 625 cycles
- FLOGN: 625 cycles (sic -- also listed as 525 in some entries)
- FETOXM1: 645 cycles
- FTANH: 661 cycles
- FSINH: 667 cycles
- FATANH: 693 cycles
- FCOSH: 607 cycles

Memory source operands add roughly 27-30 cycles for integer/single/double
formats and ~800 cycles for packed decimal.

**MC68882 speedup:** The MC68882 is substantially faster due to
concurrent conversion and execution. Typical overall execution times
are 30-50% of the MC68881 times for sequences of instructions.
`(MC68881UM SS8.5.1, Tables 8-2 and 8-3)`

### 11.4 MC68882 Concurrency Model

The MC68882 provides head (H) and tail (T) values for each instruction:
- **H (Head):** The number of cycles before the MC68882 can begin
  accepting a new instruction. Add effective address time to get the
  true head.
- **T (Tail):** The number of cycles during which the MC68882 can begin
  another instruction concurrently.

Overlap = min(T of current instruction, effective H of next instruction).
Total time for a sequence = sum of individual times - total overlap.

**Worked example** from the manual `(MC68881UM SS8.5.1.3, Table 8-5)`:

```
Instruction         <ea>  MC68881  MC68882  H    T    Adjusted H  Overlap
FMUL.D <ea>,FP1       6     98      95     36   58    36+6=42       --
FMOVE.D FP2,<ea>      6     86      44     44    *    44+6=50       58
FADD.D <ea>,FP1       6     78      75     36   38    36+6=42       38
FMOVE.X FP0,FP2       0     33      21     21    *         21       --
FMUL.D <ea>,FP2       6     98      95     36   58    36+6=42       58
FMOVE.D FP1,<ea>      6     86      44     44    *    44+6=50       --
FADD.D <ea>,FP2       6     78      75     36   38    36+6=42       38
FMOVE.X FP0,FP1       0     33      21     21    *         21       21
                     ---   ----    ----                            ----
Totals:               36    590     470                             175

MC68881 total: 590 + 36 = 626 clocks (sic: manual says 593)
MC68882 total: 470 + 36 - 175 = 331 clocks
Speedup ratio: 1.80x
```

*T=* means the FMOVE instruction is "fully concurrent" -- its tail time
is not fixed but depends on the next instruction's head time.

The concurrency model has practical implications:
- Interleaving FP operations with FMOVE instructions (which can execute
  in the CU while the APU processes arithmetic) maximises throughput.
- Back-to-back transcendentals gain no concurrency on the MC68881 and
  minimal concurrency on the MC68882 (the second instruction stalls
  until the APU finishes the first).
- Register conflicts (reading a register being written by a concurrent
  instruction) force serialisation.

`(MC68881UM SS8.5.1.3, SS5.1.2)`

### 11.5 Interrupt Latency During FPU Operations

When the FPU is executing a long instruction (e.g., FSIN at 391 cycles),
the CPU may be blocked polling the Response CIR with Null (CA=1, IA=1)
primitives. The IA (Interrupts Allowed) bit permits the CPU to check for
and service pending interrupts between polls.

Most FPU instructions provide very short worst-case interrupt latency
(a few microseconds) even during long operations. The exception is
FRESTORE with a busy state frame, which has the longest single
non-interruptible period.

For emulation: if cycle-exact interrupt timing is needed, the emulator
must model the FPU as a separate device that occupies N cycles and
allows interrupt checks at defined points during execution.
`(MC68881UM SS8.3)`

### 11.6 Coprocessor Interface Overhead

The overhead for the CPU-to-FPU handshake is typically 11 clocks
(unoptimised) but can be reduced to 2 clocks with optimised code
sequences. This overhead is already included in the overall execution
times in the timing tables.

CIR access times:
- Write to CIR: 3 clock cycles
- Read from CIR (general): 3 clock cycles
- Read from Response or Save CIR: 5 clock cycles (the FPU needs extra
  time to prepare the response)

`(MC68881UM SS8.4, SS8.5.1)`

### 11.7 FMOVEM and Context Switch Timing

FMOVEM timing scales linearly with the number of registers transferred.
For n floating-point data registers:

| Operation              | Formula (best case)     | 8 registers (worst case) |
|------------------------|-------------------------|--------------------------|
| FMOVEM FPdr to memory  | 35 + 25n clocks         | ~235 clocks              |
| FMOVEM memory to FPdr  | 33 + 31n clocks         | ~281 clocks              |

State frame transfer (FSAVE/FRESTORE):

| Operation       | Frame Type | Transfer Time |
|-----------------|------------|---------------|
| FSAVE           | Idle       | 36 clocks     |
| FSAVE           | Busy       | 270 clocks    |
| FRESTORE        | Idle       | 35 clocks     |
| FRESTORE        | Busy       | 270 clocks    |

A full context switch (FSAVE + FMOVEM FP0-FP7 + FMOVEM FPCR/FPSR/FPIAR
+ restore sequence) takes roughly 600-1200 clocks depending on whether
the FPU was idle or busy. `(MC68881UM SS8.5.1.4, SS8.5.2.8, SS8.5.2.9,
Tables 8-6, 8-21, 8-22)`

---

## 12. Amiga Integration

### 12.1 Math IEEE Libraries

The Amiga provides four math libraries that abstract FPU operations:

| Library                       | Functions                           |
|-------------------------------|-------------------------------------|
| `mathieeesingbas.library`     | Single-precision basic: Add, Sub, Mul, Div, Cmp, Abs, Neg, Tst, Fix, Flt, Floor, Ceil |
| `mathieeedoubbas.library`     | Double-precision basic: same functions |
| `mathieeesingtrans.library`   | Single-precision transcendental: Sin, Cos, Tan, Asin, Acos, Atan, Sinh, Cosh, Tanh, Exp, Log, Log10, Pow, Sqrt, Fieee, Tieee |
| `mathieeedoubtrans.library`   | Double-precision transcendental: same functions |

These libraries detect the presence of an FPU at open time. If an FPU is
present, the library patches its jump table to use FPU instructions
directly. If no FPU is present, software emulation routines are used.

**Critical restriction (V45+):** The library base must not be shared
between tasks. Each task must open the library independently because the
library open vector initialises the FPU context for the calling task.
Opening in one task and using from another will not initialise the FPU
properly for the second task. `(NDK 3.9, mathieeesingbas.library)`

### 12.2 68040/68060 Software Emulation

The 68040 and 68060 built-in FPUs do not implement all MC68881/MC68882
instructions. Unimplemented instructions generate F-line exceptions.
The Amiga handles this through two mechanisms:

1. **68040.library / 68060.library:** Installed during boot, these
   libraries set up exception handlers for the F-line vector. When an
   unimplemented FPU instruction is encountered, the handler decodes the
   instruction and provides a software implementation.

2. **fpsp.resource (Floating Point Support Package):** An optional
   resource that provides faster emulation by intercepting unimplemented
   instructions without full exception frame overhead. When available,
   the math IEEE libraries use it instead of the exception-based
   approach.

From the NDK documentation: "All this -- complete CPU usage, FPU usage
plus optional fpsp.resource support -- is completely transparent to the
user of this library." `(NDK 3.9, mathieeedoubtrans.library)`

### 12.3 FPU Detection at Boot

During Kickstart boot:
1. The CPU detection routine (step 19 in the Exec startup list) probes
   for FPU presence by attempting an FPU instruction inside a bus error
   handler.
2. If the instruction succeeds (no bus error), the FPU type is determined
   and the appropriate `AttnFlags` bits are set in ExecBase.
3. The math IEEE libraries check `AttnFlags` at open time to select
   between FPU and software code paths.

For emulator authors: set the correct `AttnFlags` bits based on the
emulated FPU type. If you emulate a 68882, set both `AFF_68881` and
`AFF_68882`. If you emulate a 68040 with full FPU emulation, set
`AFF_68881`, `AFF_68882`, and `AFF_FPU40`.
`(Kickstart Internals SS4.5)`

### 12.4 SetPatch and FPU

On Amigas with 68040/68060 processors, SetPatch (run from the startup
sequence) loads the appropriate CPU library (68040.library or
68060.library) which installs the FPU trap handlers. Without this step,
any program using unimplemented FPU instructions will crash with an
unhandled F-line exception.

Emulators that provide full 68881/68882 instruction set emulation
(including all transcendentals) in the emulated FPU hardware do not need
to worry about this -- the trap handlers will never be invoked because
no instructions generate F-line exceptions.

### 12.5 Practical FPU Usage on the Amiga

Most Amiga software does not use FPU instructions directly. Instead,
programs use the math IEEE libraries or are compiled with compiler flags
that emit FPU instructions (e.g., SAS/C's `MATH=IEEE` or GCC's `-m68881`).

Software that uses the FPU directly (inline assembly or compiler-
generated FPU code) typically:
1. Checks `AttnFlags` for FPU presence before using FPU instructions
2. Opens the appropriate math library anyway (for OS compatibility)
3. Uses FPU instructions for inner loops and hot code paths

Games and demos that use the FPU (mostly on A3000/A4000) often use it
for 3D transformations, texture mapping, and audio DSP. The A1200 demo
scene relied on accelerator boards (Blizzard 1230/1260) that included
FPUs.

### 12.6 Emulation Implementation Notes

For emulators implementing FPU support:

1. **Use the host platform's 80-bit extended precision** if available
   (x87 on x86, or `long double` on some platforms). If not available,
   use software extended precision or 64-bit double with careful
   handling of the extended exponent range.

2. **FPCR rounding mode must be enforced.** The FPU's rounding mode
   affects every arithmetic result. On x86 hosts, the x87 FPU control
   word can be set to match. For softfloat implementations, pass the
   rounding mode to each operation.

3. **FPCR precision mode matters.** When single or double precision is
   selected, results must be rounded to that precision's mantissa and
   exponent range, even though they are stored in 80-bit format. This
   is not the same as performing the operation in single/double -- the
   computation uses full extended precision internally, then the result
   is rounded.

4. **Condition codes are set consistently.** After every arithmetic
   operation, the four condition code bits (N, Z, I, NAN) must be set
   based on the result value, not the operation type.

5. **FSAVE/FRESTORE state frames** must match the frame format of the
   emulated FPU (MC68881 or MC68882). The version number in the format
   word identifies the chip. AmigaOS checks the version number during
   FRESTORE and will take a format error exception if it does not match
   the current FPU.

6. **Exception handling can be simplified** for most use cases. Amiga
   software rarely enables FPU exception traps. An emulator that does
   not implement exception traps (always uses the trap-disabled default
   results) will run the vast majority of software correctly.

7. **FMOVECR** must return the correct ROM constants. The 68040/68060
   do not implement FMOVECR in hardware, so if emulating those CPUs,
   the F-line handler must provide the constants.

8. **Transcendental accuracy** does not need to match the MC68881 bit
   for bit. The IEEE standard does not specify transcendental accuracy,
   and different FPU mask revisions may produce slightly different
   results. Use a good math library (e.g., MPFR) and ensure results
   are within ~1 ULP of double precision.

### 12.7 Cross-Reference to CPU Timing Reference

The companion [Amiga 68000 Timing Reference](amiga-68000-timing.md),
located at the same path, covers:
- CPU variants and clock rates (Section 1) -- the FPU clock on the A3000
  matches the 68030 at 25 MHz
- Exception vector table (Appendix A) -- includes FPU exception vectors
  48-54
- Bus cycle timing -- relevant for understanding CIR access overhead
- Cache behaviour (Section 14) -- instruction cache hits reduce FPU
  instruction start-up time

---

## Appendix A. Instruction Timing Tables

### A.1 MC68881 Overall Execution Times (Clock Cycles)

Source: `(MC68881UM SS8.5.1.2, Table 8-2)`

| Instruction | FPn-FPm | Integer | Single | Double | Extended | Packed |
|-------------|---------|---------|--------|--------|----------|--------|
| FABS        | 35      | 62      | 54     | 60     | 58       | 872    |
| FACOS       | 625     | 652     | 644    | 650    | 646      | 1462   |
| FADD        | 51      | 80      | 72     | 78     | 76       | 888    |
| FASIN       | 581     | 608     | 600    | 606    | 604      | 1418   |
| FATAN       | 403     | 430     | 422    | 428    | 426      | 1240   |
| FATANH      | 693     | 720     | 712    | 718    | 716      | 1530   |
| FCMP        | 33      | 62      | 54     | 60     | 58       | 870    |
| FCOS        | 391     | 418     | 410    | 416    | 414      | 1228   |
| FCOSH       | 607     | 634     | 626    | 632    | 630      | 1444   |
| FDIV        | 103     | 132     | 124    | 130    | 128      | 940    |
| FETOX       | 497     | 524     | 516    | 522    | 520      | 1334   |
| FETOXM1     | 645     | 572     | 564    | 570    | 568      | 1382   |
| FGETEXP     | 45      | 72      | 64     | 70     | 68       | 882    |
| FGETMAN     | 31      | 58      | 50     | 56     | 54       | --     |
| FINT        | 55      | 82      | 74     | 80     | 78       | 892    |
| FINTRZ      | 55      | 82      | 74     | 80     | 78       | 892    |
| FLOGN       | 625     | 552     | 544    | 550    | 548      | 1362   |
| FLOGNP1     | 571     | 598     | 590    | 596    | 594      | 1408   |
| FLOG10      | 581     | 608     | 600    | 606    | 604      | 1418   |
| FLOG2       | 581     | 608     | 600    | 606    | 604      | 1418   |
| FMOD        | 70      | 99      | 91     | 97     | 95       | 907    |
| FMOVE->FPn  | 33      | 60      | 52     | 58     | 56       | 870    |
| FMOVE->mem  | --      | 100     | 80     | 86     | 72       | 2002   |
| FMOVECR     | 29      | --      | --     | --     | --       | --     |
| FMUL        | 71      | 100     | 92     | 98     | 96       | 908    |
| FNEG        | 35      | 62      | 54     | 60     | 58       | 872    |
| FREM        | 100     | 129     | 121    | 127    | 125      | 937    |
| FSCALE      | 41      | 70      | 62     | 68     | 66       | 878    |
| FSGLDIV     | 69      | 98      | 90     | 96     | 94       | 906    |
| FSGLMUL     | 59      | 88      | 80     | 86     | 84       | 896    |
| FSIN        | 391     | 418     | 410    | 416    | 414      | 1228   |
| FSINCOS     | 461     | 478     | 470    | 476    | 474      | 1288   |
| FSINH       | 667     | 714     | 706    | 712    | 710      | 1524   |
| FSQRT       | 107     | 134     | 126    | 132    | 130      | 944    |
| FSUB        | 51      | 80      | 72     | 78     | 76       | 888    |
| FTAN        | 473     | 500     | 492    | 498    | 496      | 1310   |
| FTANH       | 661     | 688     | 680    | 686    | 684      | 1498   |
| FTENTOX     | 567     | 594     | 586    | 592    | 590      | 1404   |
| FTST        | 33      | 60      | 52     | 58     | 56       | 870    |
| FTWOTOX     | 567     | 594     | 586    | 592    | 590      | 1404   |

Notes:
- Add effective address calculation time for memory operands.
- Subtract 5 clocks if source is an MPU data register.
- Subtract 2 clocks if destination is an MPU data register.
- Add 14 clocks for dynamic k-factor with packed decimal.

### A.2 MC68882 Overall Execution Times (Clock Cycles)

Source: `(MC68881UM SS8.5.1.2, Table 8-3)`

Selected entries (Total column, FPn-FPm / Single source):

| Instruction | FPn-FPm (Total) | Single (Total) | H (FPn-FPm) | T (FPn-FPm) |
|-------------|-----------------|----------------|--------------|--------------|
| FABS        | 38              | 51             | 17           | 17           |
| FADD        | 56              | 69             | 17           | 35           |
| FCMP        | 38              | 51             | 17           | 17           |
| FCOS        | 394             | 407            | 17           | 373          |
| FDIV        | 108             | 121            | 17           | 87           |
| FMOVE->FPn  | 21              | 34             | 21/10*       | --           |
| FMOVECR     | 32              | --             | 10           | 0            |
| FMUL        | 76              | 89             | 17           | 55           |
| FSIN        | 394             | 407            | 17           | 373          |
| FSQRT       | 110             | 123            | 17           | 89           |
| FSUB        | 56              | 69             | 17           | 35           |
| FTST        | 36              | 49             | 17           | 15           |

*FMOVE has different H values depending on register conflicts.

### A.3 Effective Address Calculation Times

Source: `(MC68881UM SS8.5.1.1, Table 8-1)`

| Addressing Mode        | Best | Cache | Worst |
|------------------------|------|-------|-------|
| Dn or An               | 0    | 0     | 0     |
| (An)                   | 0    | 2     | 2     |
| (An)+                  | 3    | 6     | 5     |
| -(An)                  | 3    | 6     | 6     |
| (d16,An) or (d16,PC)  | 0    | 2     | 3     |
| (xxx).W                | 0    | 2     | 3     |
| (xxx).L                | 1    | 4     | 5     |
| #<data>                | 0    | 0     | 0     |
| (d8,An,Xn)             | 1    | 4     | 5     |

### A.4 Conditional Instruction Times

Source: `(MC68881UM SS8.5.1.5, Table 8-7)`

| Operation      | Condition       | Best | Cache | Worst |
|----------------|-----------------|------|-------|-------|
| FBcc.W         | Branch taken    | 18   | 20    | 23    |
| FBcc.W         | Not taken       | 16   | 18    | 19    |
| FBcc.L         | Branch taken    | 18   | 20    | 23    |
| FBcc.L         | Not taken       | 16   | 18    | 21    |
| FDBcc          | True, not taken | 18   | 20    | 24    |
| FDBcc          | False, not taken| 22   | 24    | 32    |
| FDBcc          | False, taken    | 18   | 20    | 26    |
| FNOP           | --              | 16   | 18    | 19    |
| FScc           | Dn              | 16   | 18    | 21    |
| FTRAPcc        | Trap taken      | 36   | 39    | 47    |
| FTRAPcc        | Not taken       | 16   | 18    | 22    |

---

### A.5 MC68882 Overall Execution Times -- Full Table (Clock Cycles)

Source: `(MC68881UM SS8.5.1.2, Table 8-3)`

Total execution time column for each source format. H/T values shown for
FPn-to-FPm only (add EA time to H for memory operands).

| Instruction | FPn-FPm | H  | T   | Integer | Single | Double | Extended | Packed |
|-------------|---------|----|----|---------|--------|--------|----------|--------|
| FABS        | 38      | 17 | 17  | 68      | 51     | 57     | 63       | 893    |
| FACOS       | 628     | 17 | 607 | 658     | 641    | 647    | 653      | 1483   |
| FADD        | 56      | 17 | 35  | 94      | 69     | 75     | 81       | 909    |
| FASIN       | 584     | 17 | 563 | 614     | 597    | 603    | 609      | 1439   |
| FATAN       | 406     | 17 | 385 | 436     | 419    | 425    | 431      | 1261   |
| FATANH      | 696     | 17 | 675 | 725     | 709    | 715    | 721      | 1551   |
| FCMP        | 38      | 17 | 17  | 76      | 51     | 57     | 63       | 891    |
| FCOS        | 394     | 17 | 373 | 424     | 407    | 413    | 419      | 1249   |
| FCOSH       | 610     | 17 | 589 | 640     | 623    | 629    | 635      | 1465   |
| FDIV        | 108     | 17 | 87  | 146     | 121    | 127    | 133      | 961    |
| FETOX       | 500     | 17 | 479 | 530     | 513    | 519    | 525      | 1355   |
| FETOXM1     | 548     | 17 | 527 | 578     | 561    | 567    | 573      | 1403   |
| FGETEXP     | 48      | 17 | 27  | 78      | 61     | 67     | 73       | 903    |
| FGETMAN     | 34      | 17 | 13  | 64      | 47     | 53     | 59       | 889    |
| FINT        | 58      | 17 | 37  | 88      | 71     | 77     | 83       | 913    |
| FINTRZ      | 58      | 17 | 37  | 88      | 71     | 77     | 83       | 913    |
| FLOGN       | 528     | 17 | 507 | 558     | 541    | 547    | 553      | 1383   |
| FLOGNP1     | 574     | 17 | 553 | 604     | 587    | 593    | 599      | 1429   |
| FLOG10      | 584     | 17 | 563 | 614     | 597    | 603    | 609      | 1439   |
| FLOG2       | 584     | 17 | 563 | 614     | 597    | 603    | 609      | 1439   |
| FMOD        | 75      | 17 | 54  | 113     | 88     | 94     | 100      | 928    |
| FMOVE->FPn  | 21      | 21 | --  | 48      | 34     | 40     | 46       | 891    |
| FMOVE->mem  | --      | -- | --  | 110     | 38     | 44     | 50       | 2006   |
| FMOVECR     | 32      | 10 | 0   | --      | --     | --     | --       | --     |
| FMUL        | 76      | 17 | 55  | 114     | 89     | 95     | 101      | 929    |
| FNEG        | 38      | 17 | 17  | 68      | 51     | 57     | 63       | 893    |
| FREM        | 105     | 17 | 84  | 143     | 118    | 124    | 130      | 958    |
| FSCALE      | 46      | 17 | 25  | 84      | 59     | 65     | 71       | 899    |
| FSGLDIV     | 74      | 17 | 53  | 112     | 87     | 93     | 99       | 927    |
| FSGLMUL     | 64      | 17 | 43  | 102     | 77     | 83     | 89       | 917    |
| FSIN        | 394     | 17 | 373 | 424     | 407    | 413    | 419      | 1249   |
| FSINCOS     | 454     | 17 | 433 | 484     | 467    | 473    | 479      | 1309   |
| FSINH       | 690     | 17 | 669 | 720     | 703    | 709    | 715      | 1545   |
| FSQRT       | 110     | 17 | 89  | 140     | 123    | 129    | 135      | 965    |
| FSUB        | 56      | 17 | 35  | 94      | 69     | 75     | 81       | 909    |
| FTAN        | 476     | 17 | 455 | 506     | 489    | 495    | 501      | 1331   |
| FTANH       | 664     | 17 | 643 | 694     | 677    | 683    | 689      | 1519   |
| FTENTOX     | 570     | 17 | 549 | 600     | 583    | 589    | 595      | 1425   |
| FTST        | 36      | 17 | 15  | 66      | 49     | 55     | 61       | 891    |
| FTWOTOX     | 570     | 17 | 549 | 600     | 583    | 589    | 595      | 1425   |

Notes:
- Add effective address calculation time for memory operands.
- Add EA time to H to get the effective head time (not for FMOVE to memory).
- FMOVE->FPn and FMOVE->mem have special concurrency: they do not have a
  fixed tail time. The effective head of the next instruction determines
  the overlap.
- Subtract 5 clocks if source is MPU data register; subtract 2 if
  destination is MPU data register.
- Add 14 clocks for dynamic k-factor with packed decimal.
- Add 2 clocks for all MC68882 entries if using FMOVEM (vs MC68881).

### A.6 FSAVE/FRESTORE Timing

| Operation | Frame Type | Best Case | Notes              |
|-----------|------------|-----------|---------------------|
| FSAVE     | Null       | ~15       | Only format word    |
| FSAVE     | Idle       | ~51       | 24/56 bytes of data |
| FSAVE     | Busy       | ~285      | 180/212 bytes       |
| FRESTORE  | Null       | ~19       | Reset to null state |
| FRESTORE  | Idle       | ~54       |                     |
| FRESTORE  | Busy       | ~289      |                     |

Add effective address calculation time. MC68882 idle/busy frames are
32 bytes larger than MC68881 frames, adding ~25 clocks to the transfer.
`(MC68881UM SS8.5.1.6, SS8.5.2.9, Tables 8-22, 8-23)`

---

## Appendix B. ROM Constant Table (Complete)

Extended-precision hex values for the FMOVECR constants. These are the
exact values stored in the MC68881/MC68882 on-chip ROM.

| Offset | Constant   | Hex (sign:exp:mantissa)                    |
|--------|------------|--------------------------------------------|
| $00    | Pi         | 0:4000:C90FDAA22168C235                    |
| $0B    | Log10(2)   | 0:3FFD:9A209A84FBCFF799                    |
| $0C    | e          | 0:4000:ADF85458A2BB4A9B (approx)           |
| $0D    | Log2(e)    | 0:3FFF:B8AA3B295C17F0BC (approx)           |
| $0E    | Log10(e)   | 0:3FFD:DE5BD8A937287195 (approx)           |
| $0F    | 0.0        | 0:0000:0000000000000000                    |
| $30    | ln(2)      | 0:3FFE:B17217F7D1CF79AC (approx)           |
| $31    | ln(10)     | 0:4000:935D8DDDAAA8AC17 (approx)           |
| $32    | 10^0 = 1   | 0:3FFF:8000000000000000                    |
| $33    | 10^1       | 0:4002:A000000000000000                    |
| $34    | 10^2       | 0:4005:C800000000000000                    |
| $35    | 10^4       | 0:400C:9C40000000000000                    |
| $36    | 10^8       | 0:4019:BEBC200000000000                    |
| $37    | 10^16      | 0:4034:8E1BC9BF04000000                    |
| $38    | 10^32      | 0:4069:9DC5ADA82B70B59E                    |
| $39    | 10^64      | 0:40D3:C2781F49FFCFA6D5                    |
| $3A    | 10^128     | 0:41A8:93BA47C980E98CE0                    |
| $3B    | 10^256     | 0:4351:AA7EEBFB9DF9DE8E                    |
| $3C    | 10^512     | 0:46A3:E319A0AEA60E91C7                    |
| $3D    | 10^1024    | 0:4D48:C976758681750C17                    |
| $3E    | 10^2048    | 0:5A92:9E8B3B5DC53D5DE5                    |
| $3F    | 10^4096    | 0:7525:C46052028A20979B                    |

**Note:** Exact hex values for the mathematical constants (Pi, e, logs)
may vary slightly between references; the values above are approximate
representations. Emulator authors should use the full 80-bit extended
precision values from a known-good source (e.g., WinUAE's FPU
implementation).

---

## Appendix C. Conditional Predicate Encoding

### C.1 IEEE Nonaware Tests (Set BSUN if NAN CC bit is set)

| Mnemonic | Definition                     | Equation              | Predicate |
|----------|--------------------------------|-----------------------|-----------|
| EQ       | Equal                          | Z                     | 000001    |
| NE       | Not Equal                      | NOT(Z)                | 001110    |
| GT       | Greater Than                   | NOT(NAN OR Z OR N)    | 010010    |
| NGT      | Not Greater Than               | NAN OR Z OR N         | 011101    |
| GE       | Greater Than or Equal          | Z OR NOT(NAN OR N)    | 010011    |
| NGE      | Not (Greater or Equal)         | NAN OR (N AND NOT(Z)) | 011100    |
| LT       | Less Than                      | N AND NOT(NAN OR Z)   | 010100    |
| NLT      | Not Less Than                  | NAN OR Z OR NOT(N)    | 011011    |
| LE       | Less Than or Equal             | Z OR (N AND NOT(NAN)) | 010101    |
| NLE      | Not (Less or Equal)            | NAN OR (NOT(N) AND NOT(Z))| 011010|
| GL       | Greater or Less Than           | NOT(NAN OR Z)         | 010110    |
| NGL      | Not (Greater or Less)          | NAN OR Z              | 011001    |
| GLE      | Greater, Less, or Equal        | NOT(NAN)              | 010111    |
| NGLE     | Not (Greater, Less, or Equal)  | NAN                   | 011000    |

### C.2 IEEE Aware Tests (Never set BSUN)

| Mnemonic | Definition                     | Equation              | Predicate |
|----------|--------------------------------|-----------------------|-----------|
| EQ       | Equal                          | Z                     | 000001    |
| NE       | Not Equal                      | NOT(Z)                | 001110    |
| OGT      | Ordered Greater Than           | NOT(NAN OR Z OR N)    | 000010    |
| ULE      | Unordered or Less or Equal     | NAN OR Z OR N         | 001101    |
| OGE      | Ordered Greater or Equal       | Z OR NOT(NAN OR N)    | 000011    |
| ULT      | Unordered or Less Than         | NAN OR (N AND NOT(Z)) | 001100    |
| OLT      | Ordered Less Than              | N AND NOT(NAN OR Z)   | 000100    |
| UGE      | Unordered or Greater or Equal  | NAN OR Z OR NOT(N)    | 001011    |
| OLE      | Ordered Less or Equal          | Z OR (N AND NOT(NAN)) | 000101    |
| UGT      | Unordered or Greater Than      | NAN OR (NOT(N) AND NOT(Z))| 001010|
| OGL      | Ordered Greater or Less Than   | NOT(NAN OR Z)         | 000110    |
| UEQ      | Unordered or Equal             | NAN OR Z              | 001001    |
| OR       | Ordered                        | NOT(NAN)              | 000111    |
| UN       | Unordered                      | NAN                   | 001000    |

### C.3 Miscellaneous Tests

| Mnemonic | Definition          | Equation | Predicate |
|----------|---------------------|----------|-----------|
| F        | False               | False    | 000000    |
| T        | True                | True     | 001111    |
| SF       | Signaling False     | False    | 010000    |
| ST       | Signaling True      | True     | 011111    |
| SEQ      | Signaling Equal     | Z        | 010001    |
| SNE      | Signaling Not Equal | NOT(Z)   | 011110    |

`(MC68881UM SS4.4, SS4.4.1, SS4.4.2, SS4.4.3)`

---

## Appendix D. Instruction Encoding Summary

### D.1 Operation Word (First word of all FPU instructions)

```
15  14  13  12  11  10   9   8   7   6   5   4   3   2   1   0
 1   1   1   1  |  cpID  |  TYPE  |     TYPE-DEPENDENT        |
```

TYPE encodings:
- `000` = General (arithmetic/move/FMOVEM)
- `001` = FDBcc / FScc / FTRAPcc
- `010` = FBcc.W (16-bit displacement follows)
- `011` = FBcc.L (32-bit displacement follows)
- `100` = FSAVE
- `101` = FRESTORE

### D.2 Command Word (Second word for general instructions)

```
15  14  13  12  11  10   9   8   7   6   5   4   3   2   1   0
 OPCLASS  |     RX      |     RY      |      EXTENSION        |
```

OPCLASS:
- `000` = FPm -> FPn (register-to-register)
- `010` = <ea> -> FPn (external to register) / FMOVECR (when RX=111)
- `011` = FPm -> <ea> (register to external)
- `100` = Move to system control register
- `101` = Move from system control register
- `110` = FMOVEM memory to FP registers
- `111` = FMOVEM FP registers to memory

### D.3 FPn Register Encoding

| Code | Register |
|------|----------|
| 000  | FP0      |
| 001  | FP1      |
| 010  | FP2      |
| 011  | FP3      |
| 100  | FP4      |
| 101  | FP5      |
| 110  | FP6      |
| 111  | FP7      |

### D.4 System Control Register Select

| Code | Register(s)         |
|------|---------------------|
| 001  | FPIAR               |
| 010  | FPSR                |
| 011  | FPSR, then FPIAR    |
| 100  | FPCR                |
| 101  | FPCR, then FPIAR    |
| 110  | FPCR, then FPSR     |
| 111  | FPCR, FPSR, FPIAR   |

### D.5 FMOVEM Register List Encoding

For static register list in predecrement mode -(An):
```
Bit 7=FP7, Bit 6=FP6, ..., Bit 0=FP0
```

For static register list in postincrement or control modes:
```
Bit 7=FP0, Bit 6=FP1, ..., Bit 0=FP7
```

For dynamic list: Dn contains the mask in the least-significant 8 bits.

`(MC68881UM SS4.7, SS4.8, SS4.9)`

---

## Appendix E. Gaps and Source Map

### E.1 Known Gaps

| Topic                                  | Status                          |
|----------------------------------------|---------------------------------|
| Exact FMOVECR hex constants            | Approximate values given; consult WinUAE source for verified bit-exact values |
| 68040 FSAVE state frame format         | Not in MC68881UM; requires MC68040 User's Manual |
| 68060 FSAVE state frame format         | Not in MC68881UM; requires MC68060 User's Manual |
| 68060 missing instruction list         | Partial; requires MC68060 User's Manual for complete list |
| Bus cycle timing diagrams              | Described textually; original diagrams are graphical and could not be reproduced from OCR text |
| MC68881 detailed timing tables (8-9 through 8-24) | Arithmetic calculation subtables partially reproduced; full detail requires original manual |
| mathieee*.library internal implementation | NDK autodocs describe the API; internal FPU dispatching is not documented |
| fpsp.resource implementation details   | Mentioned in NDK; no public documentation of internals |

### E.2 Source Map

| Section | Primary Source                           | Manual Section(s)    |
|---------|------------------------------------------|----------------------|
| 1       | Community knowledge, MC68881UM S1        | SS1                  |
| 2       | MC68881UM                                | SS7.1-7.5            |
| 3       | MC68881UM                                | SS2.1, SS2.4         |
| 4       | MC68881UM                                | SS2.2                |
| 5       | MC68881UM                                | SS2.3                |
| 6       | MC68881UM                                | SS3                  |
| 7       | MC68881UM                                | SS4                  |
| 8       | MC68881UM                                | SS4.6 (FMOVECR)      |
| 9       | MC68881UM                                | SS6                  |
| 10      | Community knowledge, NDK 3.9, Kickstart  | --                   |
| 11      | MC68881UM                                | SS8                  |
| 12      | NDK 3.9, Kickstart Internals            | --                   |
| App A   | MC68881UM                                | SS8.5 (Tables 8-1 to 8-7) |
| App B   | MC68881UM, community knowledge           | FMOVECR description  |
| App C   | MC68881UM                                | SS4.4                |
| App D   | MC68881UM                                | SS4.7-4.9            |

---

## Appendix F. FPU Emulation Implementation Checklist

A progressive checklist for implementing FPU support in an Amiga
emulator, ordered from most to least critical.

### F.1 Minimum Viable FPU (Level 1)

These items are required for basic software compatibility:

- [ ] Decode F-line operation words (bits [15:12] = $F, cpID = 001)
- [ ] Implement 8 x 80-bit FP data registers (FP0-FP7)
- [ ] Implement FPCR, FPSR, FPIAR (32-bit each)
- [ ] FMOVE: all 7 source formats to FPn (with conversion to extended)
- [ ] FMOVE: FPn to all 7 destination formats (with conversion from extended)
- [ ] FMOVE: FPm to FPn (register to register)
- [ ] FMOVE: to/from FPCR, FPSR, FPIAR
- [ ] FMOVEM: register list to/from memory (control and data registers)
- [ ] FMOVECR: all 22 ROM constants
- [ ] Basic arithmetic: FADD, FSUB, FMUL, FDIV
- [ ] FCMP, FTST (condition code setting)
- [ ] FABS, FNEG
- [ ] FSQRT
- [ ] FINT, FINTRZ
- [ ] FBcc with all 32 condition predicates
- [ ] Condition code setting (N, Z, I, NAN) after every arithmetic op
- [ ] Rounding mode support (RN, RZ, RM, RP in FPCR)
- [ ] Set AttnFlags in ExecBase correctly

### F.2 Full Instruction Set (Level 2)

Required for programs that use transcendentals directly:

- [ ] FSIN, FCOS, FTAN, FSINCOS
- [ ] FASIN, FACOS, FATAN
- [ ] FSINH, FCOSH, FTANH, FATANH
- [ ] FETOX, FETOXM1, FTWOTOX, FTENTOX
- [ ] FLOGN, FLOGNP1, FLOG10, FLOG2
- [ ] FMOD, FREM (including quotient byte in FPSR)
- [ ] FSCALE, FGETEXP, FGETMAN
- [ ] FSGLDIV, FSGLMUL
- [ ] FScc, FDBcc, FTRAPcc
- [ ] FNOP
- [ ] Rounding precision support (extended/single/double in FPCR)
- [ ] FPSR accrued exception byte (AEXC) -- sticky OR logic
- [ ] Packed decimal format support (FMOVE to/from packed)

### F.3 Exception Handling (Level 3)

Required for programs that enable FPU traps and for OS compatibility:

- [ ] Exception enable/disable via FPCR ENABLE byte
- [ ] BSUN exception on IEEE nonaware conditional tests with NAN set
- [ ] SNAN detection and trap/conversion
- [ ] OPERR detection and default results
- [ ] OVFL detection and rounding-mode-dependent results
- [ ] UNFL detection and gradual underflow (denormalisation)
- [ ] DZ detection
- [ ] INEX2 / INEX1 detection
- [ ] Multiple exception priority handling
- [ ] Pre-instruction vs mid-instruction exception reporting
- [ ] Correct exception vector numbers (48-54, plus 7, 11, 13)
- [ ] FPIAR updated before each arithmetic instruction (when exceptions enabled)

### F.4 Context Switching (Level 4)

Required for multitasking support (AmigaOS needs this):

- [ ] FSAVE: generate correct state frame (null/idle/busy)
- [ ] FRESTORE: accept and restore state frames
- [ ] Format word version number matching
- [ ] Null state restore (reset to NANs, clear FPCR/FPSR)
- [ ] Idle state frame: save/restore exceptional operand, BIU flags
- [ ] Busy state frame: save/restore full internal state
- [ ] Privileged instruction check for FSAVE/FRESTORE

### F.5 Timing Accuracy (Level 5)

Required only for cycle-exact emulation:

- [ ] Model FPU as a separate device with independent cycle counter
- [ ] Implement concurrent execution (CPU proceeds while FPU calculates)
- [ ] Track instruction start-up overhead (11 clocks typical)
- [ ] CIR access timing (3/5 clocks per access)
- [ ] Per-instruction execution times from timing tables
- [ ] MC68882 head/tail concurrency model (if emulating 68882)
- [ ] Interrupt latency during FPU operations (IA bit)
- [ ] FMOVEM transfer timing (proportional to register count)

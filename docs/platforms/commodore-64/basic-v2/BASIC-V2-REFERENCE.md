# BASIC V2 Language Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 1

---

## Overview

Commodore 64 BASIC V2 is a dialect of Microsoft BASIC with specific limitations and behaviors. Understanding these rules is critical for writing correct lesson code and avoiding common pitfalls.

**Key characteristics:**
- **65 keywords** with special meanings
- **Two data types:** Numeric (integer/floating-point) and String
- **Line-based execution:** 80-character logical line limit
- **Variable name significance:** Only first two characters matter
- **Automatic type conversion:** Between integers and floating-point

---

## Operating System Components

The C64 Operating System consists of three interrelated ROM modules:

### 1. BASIC Interpreter
- Analyzes BASIC statement syntax
- Performs calculations and data manipulation
- Contains 65 reserved keywords
- Handles variable storage and memory management

### 2. KERNAL
- Interrupt-level processing
- Actual input/output operations
- Device communication (keyboard, disk, serial)

### 3. Screen Editor
- Video screen output control
- BASIC program text editing
- Keyboard input interception
- Character set management

---

## Operating Modes

### DIRECT Mode
- Statements have **no line numbers**
- Executed immediately when RETURN is pressed
- Used for testing and immediate calculations

**Example:**
```basic
PRINT 2+2
```

### PROGRAM Mode
- All statements **must have line numbers**
- Multiple statements per line separated by colons (`:`)
- 80-character logical line limit (including line number)
- Statements exceeding limit must continue on new numbered line

**Example:**
```basic
10 PRINT "HELLO"
20 FOR I=1 TO 10:PRINT I:NEXT I
```

---

## Character Set

C64 provides **two complete character sets** switchable with CBM+SHIFT:

### SET 1 (Power-on Default)
- Upper case alphabet + numbers 0-9 (no SHIFT)
- Graphics characters (SHIFT or CBM key)

### SET 2 (CBM+SHIFT to toggle)
- Lower case alphabet + numbers 0-9 (no SHIFT)
- Upper case alphabet (SHIFT)
- Graphics characters (CBM key)

---

## Special Characters in BASIC

| Character | Name | Use |
|-----------|------|-----|
| (space) | BLANK | Separates keywords and variable names |
| ; | SEMI-COLON | Formats output, suppresses newline |
| = | EQUAL SIGN | Assignment and relationship testing |
| + | PLUS SIGN | Addition or string concatenation |
| - | MINUS SIGN | Subtraction, unary minus |
| * | ASTERISK | Multiplication |
| / | SLASH | Division |
| ↑ | UP ARROW | Exponentiation |
| ( ) | PARENTHESES | Expression evaluation, functions |
| % | PERCENT | Declares integer variable |
| # | NUMBER SIGN | Logical file number prefix |
| $ | DOLLAR SIGN | Declares string variable |
| , | COMMA | Output formatting, parameter separator |
| . | PERIOD | Decimal point |
| " | QUOTATION MARK | Encloses string constants |
| : | COLON | Separates multiple statements |
| ? | QUESTION MARK | Abbreviation for PRINT |
| < > | COMPARISON | Relationship tests |
| π | PI | Constant 3.141592654 |

---

## Data Types

### Integer Constants

**Range:** -32768 to +32767
**Storage:** 2 bytes (16-bit signed)
**Format:** Whole numbers, no decimal point

**Rules:**
- No decimal points allowed
- No commas between digits
- Leading zeros ignored (waste memory)
- Plus sign (+) optional for positive numbers
- Must be within -32768 to +32767 range

**Examples:**
```
-12
8765
-32768
+44
0
```

**Invalid:**
```
32,768    (comma not allowed)
32768.0   (decimal point makes it floating-point)
-32769    (out of range)
```

### Floating-Point Constants

**Range:** ±1.70141183E+38 (maximum) to ±2.93873588E-39 (minimum)
**Storage:** 5 bytes (40-bit)
**Precision:** 9 digits displayed, 10 digits internal accuracy
**Format:** Simple numbers or scientific notation

**Simple Format:**
```
1.23
.998877
+3.1459
-333.
.01
```

**Scientific Notation:**
- Format: **mantissa** E **exponent**
- Mantissa: Floating-point number
- E: Means "times 10 to the power of"
- Exponent: -39 to +38

**Examples:**
```
235.988E-3     (.235988)
2359E6         (2359000000)
-7.09E-12      (-.00000000000709)
-3.14159E+5    (-314159)
```

**Automatic Scientific Notation:**
- Numbers < .01 displayed in scientific notation
- Numbers > 999999999 displayed in scientific notation

**Rounding:**
- Numbers entered with >9 digits are rounded
- 10th digit ≥5 rounds up
- 10th digit <5 rounds down

**Errors:**
- Result > 1.70141183E+38 → ?OVERFLOW ERROR
- Result < 2.93873588E-39 → 0 (no error)

### String Constants

**Length:** Up to 80 characters (minus line number/statement overhead)
**Storage:** 3 bytes + 1 byte per character
**Format:** Enclosed in double quotes (" ")

**Rules:**
- Can contain letters, numbers, punctuation, spaces
- Can contain color/cursor control codes
- **Cannot contain** double quotes (")
- Use `CHR$(34)` to include quotes in strings
- Ending quote can be omitted if last item on line or before colon

**Examples:**
```basic
""                     (null string)
"HELLO"
"$25,000.00"          (commas allowed in strings)
"NUMBER OF EMPLOYEES"
```

**Special case - Embedded quotes:**
```basic
A$ = "HE SAID " + CHR$(34) + "HELLO" + CHR$(34)
PRINT A$
REM Displays: HE SAID "HELLO"
```

---

## Variables

Variables are names representing data values. If referenced before assignment, automatically initialized to:
- **Numeric:** 0
- **String:** null value ("")

### Variable Naming Rules

**CRITICAL:** Only the **first two characters** are significant in CBM BASIC.

**Valid characters:**
- Letters A-Z (first character must be letter)
- Digits 0-9 (second character onward)
- Type declarations % or $ (last character)

**Examples of identical variables:**
```basic
SCORE
SCREEN
SCROLL
SC9999
```
All represent the **same variable** (only `SC` matters).

**Valid names:**
```
A      (floating-point)
AB     (floating-point)
A1     (floating-point)
AB1    (floating-point)
A%     (integer)
AB%    (integer)
A$     (string)
AB$    (string)
```

**Forbidden:**
- Must NOT match BASIC keywords (FOR, IF, PRINT, etc.)
- Must NOT contain keywords within name
- Keywords include: commands, statements, functions, operators

**Error if keyword embedded:**
```basic
FORD% = 5    (contains FOR - ?SYNTAX ERROR)
```

### Type Declaration Characters

| Character | Type | Example |
|-----------|------|---------|
| % | Integer | `COUNT%` |
| $ | String | `NAME$` |
| (none) | Floating-point | `PRICE` |

### Integer Variables

**Storage:** 2 bytes
**Range:** -32768 to +32767
**Speed:** Faster than floating-point in arithmetic
**Use:** Counters, array subscripts, memory addresses

**Examples:**
```basic
K% = 5
CNT% = CNT% + 1
ADDR% = 1024
```

**Overflow:** Assigning value outside range → ?ILLEGAL QUANTITY

### Floating-Point Variables

**Storage:** 5 bytes (default type)
**Range:** Same as floating-point constants
**Precision:** 9 digits displayed, 10 internal

**Examples:**
```basic
FP = 12.5
PRICE = 99.99
SUM = FP * CNT%    (result is floating-point)
```

### String Variables

**Storage:** 3 bytes + 1 per character
**Maximum length:** Limited by available memory and 80-character line limit

**Examples:**
```basic
A$ = "GROSS SALES"
MTH$ = "JAN" + A$
NAME$ = ""           (null string)
```

---

## Arrays

Arrays are tables/lists of related data items with a single name.

### Array Specifications

**Maximum dimensions:** 255 (theoretical)
**Maximum elements per dimension:** 32767 (theoretical)
**Practical limit:** Available memory

**Subscript range:** 0 to number of elements (inclusive)

### Automatic Array Creation

Arrays with:
- **One dimension**
- **Maximum subscript ≤ 10** (11 elements: 0-10)

Are automatically created and initialized when first referenced.

**Example:**
```basic
A(5) = 10     (automatically creates A(0) to A(10))
```

### DIM Statement Required

For arrays that:
- Have subscripts > 10
- Have multiple dimensions
- Need explicit size declaration

**Syntax:**
```basic
DIM arrayname(size1, size2, ...)
```

**Examples:**
```basic
DIM A(100)              (1D: 0-100, 101 elements)
DIM B(10,20)           (2D: 11×21 = 231 elements)
DIM C$(50)             (string array, 51 elements)
DIM D%(5,5,5)          (3D integer array)
```

### Array Memory Calculation

```
Memory = 5 (array name)
       + 2 × (number of dimensions)
       + 2 × (elements) for integers
    OR + 5 × (elements) for floating-point
    OR + 3 × (elements) for strings
       + 1 × (each character in string elements)
```

**Example:**
```basic
DIM A%(100)
Memory = 5 + (2×1) + (2×101) = 209 bytes
```

### Array Subscripts

Can be:
- Integer constants: `A(5)`
- Variables: `A(I%)`
- Expressions: `A(X*2+1)`

**Error conditions:**
- Subscript < 0 → ?BAD SUBSCRIPT
- Subscript > dimension size → ?BAD SUBSCRIPT
- Wrong number of subscripts → ?BAD SUBSCRIPT

### Multi-Dimensional Arrays

**Examples:**
```basic
DIM A(10)           (1D: list)
DIM B(10,10)        (2D: grid/table)
DIM C(5,5,5)        (3D: cube)

A(5) = 0            (5th element)
B(3,4) = 10         (row 3, column 4)
C(1,2,3) = 99       (position 1,2,3 in 3D space)
```

---

## Operators

### Arithmetic Operators

| Operator | Operation | Example | Result |
|----------|-----------|---------|--------|
| + | Addition | `5 + 3` | 8 |
| - | Subtraction | `5 - 3` | 2 |
| - | Unary minus | `-5` | -5 |
| * | Multiplication | `5 * 3` | 15 |
| / | Division | `10 / 2` | 5 |
| ↑ | Exponentiation | `2 ↑ 3` | 8 |

**Notes:**
- All arithmetic performed in floating-point
- Integers converted to floating-point before operation
- Result converted back to integer if assigned to integer variable

### Relational Operators

| Operator | Meaning | Example | Result |
|----------|---------|---------|--------|
| = | Equal to | `5 = 5` | -1 (true) |
| <> | Not equal to | `5 <> 3` | -1 (true) |
| < | Less than | `5 < 3` | 0 (false) |
| > | Greater than | `5 > 3` | -1 (true) |
| <= | Less than or equal | `5 <= 5` | -1 (true) |
| >= | Greater than or equal | `5 >= 3` | -1 (true) |

**Result values:**
- **True:** -1 (all bits set)
- **False:** 0 (all bits clear)

**String comparisons:**
- Character-by-character from left to right
- Based on character code values (see PETSCII)
- Shorter string < longer string (all else equal)
- Leading/trailing spaces are significant

**Examples:**
```basic
"A" < "B"           (-1, true)
"ABC" < "ABD"       (-1, true)
"ABC" < "ABCD"      (-1, true)
"ABC  " <> "ABC"    (-1, true - spaces matter!)
```

### Logical Operators

| Operator | Operation | Example |
|----------|-----------|---------|
| AND | Logical AND | `A AND B` |
| OR | Logical OR | `A OR B` |
| NOT | Logical NOT | `NOT A` |

**Operands:** Must be integers (-32768 to +32767)
**Operation:** Bit-by-bit on binary representation
**Result:** Integer

**Truth Table:**

```
AND:               OR:                NOT:
1 AND 1 = 1       1 OR 1 = 1         NOT 1 = 0
1 AND 0 = 0       1 OR 0 = 1         NOT 0 = 1
0 AND 1 = 0       0 AND 1 = 1
0 AND 0 = 0       0 OR 0 = 0
```

**Exclusive OR (XOR):** Available in WAIT statement only
```
1 XOR 1 = 0
1 XOR 0 = 1
0 XOR 1 = 1
0 XOR 0 = 0
```

**Common uses:**
```basic
IF A=5 AND B=10 THEN 100     (both conditions)
IF A=5 OR B=10 THEN 100      (either condition)
IF NOT (A=5) THEN 100        (negation)

REM Bit masking:
A = PEEK(53265) AND 127      (clear bit 7)
A = PEEK(53265) OR 128       (set bit 7)
A = 96 AND 32                (result: 32)
```

---

## Operator Hierarchy

Operations performed in this order (highest to lowest precedence):

| Precedence | Operator | Operation |
|------------|----------|-----------|
| 1 | ↑ | Exponentiation |
| 2 | - | Unary minus (negation) |
| 3 | * / | Multiplication, Division |
| 4 | + - | Addition, Subtraction |
| 5 | = <> < > <= >= | Relational (left to right) |
| 6 | NOT | Logical NOT |
| 7 | AND | Logical AND |
| 8 | OR | Logical OR |

**Important rules:**
- Operations of equal precedence: left to right
- Parentheses override precedence
- Maximum nesting depth: 10 levels
- Relational operators have **no precedence** among themselves

**Examples:**
```basic
2 + 3 * 4          (14, not 20 - multiply first)
(2 + 3) * 4        (20 - parentheses override)
2 ↑ 3 * 4          (32 - exponent first)
10 / 2 * 5         (25 - left to right)

REM Complex expression:
((X - C ↑ (D+E) / 2) * 10) + 1
```

---

## String Operations

### String Concatenation

**Operator:** `+` (plus sign)
**Effect:** Appends right string to left string

**Examples:**
```basic
A$ = "FILE"
B$ = "NAME"
C$ = A$ + B$              (C$ = "FILENAME")
D$ = "NEW " + A$ + B$     (D$ = "NEW FILENAME")
```

**Limitations:**
- Result limited by memory
- Result limited by 80-character line length

### String Comparison

Uses relational operators: `=`, `<>`, `<`, `>`, `<=`, `>=`

**Comparison rules:**
1. Character-by-character, left to right
2. Based on PETSCII character codes
3. Comparison stops at end of shorter string
4. Shorter string < longer string (all else equal)

**Example:**
```basic
"APPLE" < "BANANA"    (-1, true)
"ABC" < "ABD"         (-1, true)
"ABC" = "ABC"         (-1, true)
"ABC" < "ABCD"        (-1, true)
```

---

## Data Conversions

### Automatic Conversions

**Arithmetic operations:**
- Integers → Floating-point (before operation)
- Result → Integer (if assigned to integer variable)

**Example:**
```basic
A% = 10              (integer)
B = 5.5              (floating-point)
C = A% + B           (A% converted to 10.0, result 15.5)
D% = A% + B          (result 15.5 truncated to 15)
```

**Floating-point to Integer:**
- Fractional part **truncated** (not rounded)
- `15.9` → `15`
- `−15.9` → `−15`

**Overflow check:**
- Result > 32767 → ?ILLEGAL QUANTITY
- Result < -32768 → ?ILLEGAL QUANTITY

**Logical operations:**
- Operands converted to integers
- Result is integer

### Type Mismatch Errors

**Forbidden:**
- Numeric compared to string → ?TYPE MISMATCH
- Numeric assigned to string → ?TYPE MISMATCH
- String assigned to numeric → ?TYPE MISMATCH

**Examples:**
```basic
10 A$ = "5"
20 B = 10
30 C = A$ + B        (?TYPE MISMATCH - can't mix types)

10 IF A$ = 10 THEN   (?TYPE MISMATCH - comparing string to number)
```

---

## Expressions

An expression produces a single value from constants, variables, and operators.

### Simple Expressions

```basic
5                    (constant)
A                    (variable)
A$                   (string variable)
A(5)                 (array element)
```

### Complex Expressions

```basic
A + B
C ↑ (D + E) / 2
((X - C ↑ (D+E) / 2) * 10) + 1
GG$ > HH$
JJ$ + "MORE"
K% = 1 AND M <> X
NOT (D = E)
```

### Expression Evaluation

**Order:**
1. Innermost parentheses first
2. Apply operator hierarchy
3. Left-to-right for equal precedence

**Result type:**
- Arithmetic expression → Number (integer or floating-point)
- String expression → String
- Relational expression → Integer (-1 or 0)
- Logical expression → Integer

---

## Programming Limits

### Memory Limits

**Default BASIC program area:** 2048-40959 (38912 bytes)
**String storage:** Dynamic, shares program area
**Variable storage:** After BASIC program

**Protection:**
```basic
POKE 52,32:POKE 56,32:CLR    (protect from 8192+)
POKE 52,48:POKE 56,48:CLR    (protect from 12288+)
```

### Line Limits

**Maximum line number:** 63999
**Logical line length:** 80 characters (includes line number + statements)
**Multiple statements:** Separated by colons (:)
**Physical screen lines:** 2 screen lines max before automatic wrap

**If line exceeds 80 characters:**
- Must split into new numbered line
- Or shorten using abbreviations

### Statement Limits

**Statements per line:** Limited by 80-character line length
**Parenthesis nesting:** 10 levels maximum
**FOR...NEXT nesting:** Limited by available memory

### Keyboard Buffer

**Size:** 10 characters
**Effect:** Characters beyond 10th are lost
**Cleared:** When read by INPUT or GET

---

## Common Errors

### ?SYNTAX ERROR
- Keyword misspelled
- Keyword embedded in variable name
- Invalid character in statement
- Mismatched parentheses

### ?TYPE MISMATCH
- Mixing numeric and string in operation
- Assigning wrong type to variable

### ?ILLEGAL QUANTITY
- Integer overflow (> 32767 or < -32768)
- Invalid array subscript
- Invalid function argument

### ?BAD SUBSCRIPT
- Array subscript < 0
- Array subscript > dimension size
- Wrong number of dimensions

### ?OVERFLOW ERROR
- Floating-point calculation > 1.70141183E+38

---

## Keyboard Buffer

**Size:** 10 characters maximum
**Behavior:** FIFO (First In, First Out)

**Demonstration:**
```basic
10 TI$ = "000000"
20 IF TI$ < "000015" THEN 20
```

While this runs, type "HELLO" - it appears after 15 seconds.

**Implications:**
- GET statement reads from buffer
- Characters beyond 10th lost
- Buffer cleared by INPUT, GET#

---

## Important Notes

### Variable Name Pitfall

**Most common error:** Only first two characters significant

```basic
SCORE = 10
SCREEN = 20
SCROLL = 30
PRINT SCORE        (prints 30! All are same variable: SC)
```

**Solution:** Plan variable names carefully
- `SC` for score
- `SR` for screen
- `SL` for scroll

### Numeric Precision

**Displayed:** 9 digits
**Internal:** 10 digits
**Rounding:** Based on 10th digit

**Example:**
```basic
? 1234567890       (displays 1234567890)
? 12345678901      (displays 1.23456789E+10, rounded)
```

### String Limitations

**Cannot contain:** Double quote (")
**Workaround:** Use `CHR$(34)`

```basic
A$ = "HE SAID " + CHR$(34) + "HELLO" + CHR$(34)
```

### Leading Zeros

**Ignored in numbers:**
```basic
A = 00123          (same as A = 123, wastes 2 bytes)
```

**Preserved in strings:**
```basic
A$ = "00123"       (5 characters)
```

---

## Best Practices

### Memory Conservation

1. **Use integers** for counters, subscripts
2. **Abbreviate keywords** in final version
3. **Remove REM statements** in final version
4. **Use short line numbers** (1, 2, 3 vs 100, 110, 120)
5. **Multiple statements per line** (colon separator)
6. **Remove spaces** (not required except between keywords)

### Variable Naming

1. **First two characters unique**
2. **Meaningful names during development**
3. **Short names in final version**
4. **Use % for integers** (speed + memory)

### Code Organization

1. **Initialize at program start**
2. **Protect memory** if using graphics/sprites
3. **Clear screen** at program start
4. **Handle errors** gracefully

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 1
- **Related:** See PETSCII-REFERENCE.md for character codes
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for memory layout

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

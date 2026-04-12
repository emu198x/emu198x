# BASIC V2 Vocabulary Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 2

---

## Overview

This reference documents all BASIC V2 keywords, their abbreviations, syntax, and behavior. Use this to verify accuracy of lesson content and ensure correct usage in code examples.

**Key facts:**
- 67 keywords in BASIC V2
- Abbreviations entered with SHIFT on last letter
- Abbreviations do NOT save memory (keywords tokenize to single bytes)
- 80-character logical line limit (Screen Editor limitation)
- Lines exceeding 80 chars with abbreviations cannot be edited when listed

---

## Keyword Quick Reference

### Complete Keyword List

| Keyword | Abbrev | Screen | Type |
|---------|--------|--------|------|
| ABS | A<shift>B | A▮ | Numeric Function |
| AND | A<shift>N | A✓ | Operator |
| ASC | A<shift>S | A♣ | Numeric Function |
| ATN | A<shift>T | A▮ | Numeric Function |
| CHR$ | C<shift>H | C▮ | String Function |
| CLOSE | CL<shift>O | CL▯ | I/O Statement |
| CLR | C<shift>L | C▂ | Statement |
| CMD | C<shift>M | C✎ | I/O Statement |
| CONT | C<shift>O | C▯ | Command |
| COS | NONE | COS | Numeric Function |
| DATA | D<shift>A | D♣ | Statement |
| DEF | D<shift>E | D▯ | Statement |
| DIM | D<shift>I | D✎ | Statement |
| END | E<shift>N | E✓ | Statement |
| EXP | E<shift>X | E✿ | Numeric Function |
| FN | NONE | FN | Numeric Function |
| FOR | F<shift>O | F▯ | Statement |
| FRE | F<shift>R | F▂ | Numeric Function |
| GET | G<shift>E | G▯ | Statement |
| GET# | NONE | GET# | I/O Statement |
| GOSUB | GO<shift>S | GO♣ | Statement |
| GOTO | G<shift>O | G▯ | Statement |
| IF | NONE | IF | Statement |
| INPUT | NONE | INPUT | Statement |
| INPUT# | I<shift>N | I✓ | I/O Statement |
| INT | NONE | INT | Numeric Function |
| LEFT$ | LE<shift>F | I▂ | String Function |
| LEN | NONE | LEN | Numeric Function |
| LET | L<shift>E | I▂ | Statement |
| LIST | L<shift>I | I✎ | Command |
| LOAD | L<shift>O | I▯ | Command |
| LOG | NONE | LOG | Numeric Function |
| MID$ | M<shift>I | I✎ | String Function |
| NEW | NONE | NEW | Command |
| NEXT | N<shift>E | I▂ | Statement |
| NOT | N<shift>O | I▯ | Operator |
| ON | NONE | ON | Statement |
| OPEN | O<shift>P | I▯ | I/O Statement |
| OR | NONE | OR | Operator |
| PEEK | P<shift>E | I▂ | Numeric Function |
| POKE | P<shift>O | I▯ | Statement |
| POS | NONE | POS | Numeric Function |
| PRINT | ? | ? | Statement |
| PRINT# | P<shift>R | P▂ | I/O Statement |
| READ | R<shift>E | R▂ | Statement |
| REM | NONE | REM | Statement |
| RESTORE | RE<shift>S | RE♣ | Statement |
| RETURN | RE<shift>T | RE▮ | Statement |
| RIGHT$ | R<shift>I | R✎ | String Function |
| RND | R<shift>N | R✓ | Numeric Function |
| RUN | R<shift>U | R✓ | Command |
| SAVE | S<shift>A | S♣ | Command |
| SGN | S<shift>G | S▮ | Numeric Function |
| SIN | S<shift>I | S✎ | Numeric Function |
| SPC | S<shift>P | S▯ | Special Function |
| SQR | S<shift>Q | S■ | Numeric Function |
| STATUS/ST | ST | ST | Numeric Function |
| STEP | ST<shift>E | ST▂ | Statement |
| STOP | S<shift>T | S▮ | Statement |
| STR$ | ST<shift>R | ST▂ | String Function |
| SYS | S<shift>Y | S▮ | Statement |
| TAB | T<shift>A | T♣ | Special Function |
| TAN | NONE | TAN | Numeric Function |
| THEN | T<shift>H | T▮ | Statement |
| TIME/TI | TI | TI | Numeric Function |
| TIME$/TI$ | TI$ | TI$ | String Function |
| TO | NONE | TO | Statement |
| USR | U<shift>S | U♣ | Numeric Function |
| VAL | V<shift>A | V♣ | Numeric Function |
| VERIFY | V<shift>E | V▂ | Command |
| WAIT | W<shift>A | W♣ | Statement |

---

## Function Types

BASIC V2 has several categories of keywords:

### Commands
Execute immediately in direct mode:
- CONT, LIST, LOAD, NEW, RUN, SAVE, VERIFY

### Statements
Used within programs:
- CLR, CMD, CLOSE, DATA, DEF, DIM, END, FOR/NEXT, GET, GOSUB, GOTO, IF/THEN, INPUT, LET, ON, OPEN, POKE, PRINT, READ, REM, RESTORE, RETURN, STOP, SYS, WAIT

### Numeric Functions
Return numeric values:
- ABS, ASC, ATN, COS, EXP, FN, FRE, INT, LEN, LOG, PEEK, POS, RND, SGN, SIN, SQR, STATUS/ST, TAN, TIME/TI, USR, VAL

### String Functions
Return string values:
- CHR$, LEFT$, MID$, RIGHT$, STR$, TIME$/TI$

### Operators
Used in expressions:
- AND, NOT, OR

### Special Functions
Used within PRINT statements:
- SPC, TAB

---

## Alphabetical Keyword Reference

### ABS

**Type:** Numeric Function
**Format:** `ABS(<expression>)`

Returns the absolute value (value without sign) of a number.

**Examples:**
```basic
10 x = abs(y)
20 print abs(x * j)
30 if x = abs(x) then print "positive"
```

---

### AND

**Type:** Operator
**Format:** `<expression> AND <expression>`

Boolean/bitwise AND operator. Works on integers -32768 to +32767.

**Truth table (1-bit):**
```
0 AND 0 = 0
0 AND 1 = 0
1 AND 0 = 0
1 AND 1 = 1
```

**Usage:**
- Bit masking: Extract specific bits from a value
- Logic: Both conditions must be true

**Examples:**
```basic
10 if x=7 and w=3 then goto 10
20 a = 17 and 194    : rem Result: 0
30 a = peek(53280) and 15  : rem Mask lower 4 bits
```

**Range:** -32768 to +32767 (16-bit signed integers)
**Error:** ?ILLEGAL QUANTITY if outside range

---

### ASC

**Type:** Numeric Function
**Format:** `ASC(<string>)`

Returns the Commodore ASCII (PETSCII) value of the first character in the string (0-255).

**Examples:**
```basic
10 print asc("z")      : rem Returns 90
20 x = asc("zebra")    : rem Returns 90 (first char)
30 j = asc(j$)
```

**Special case:** Empty string causes ?ILLEGAL QUANTITY error
**Workaround:** `j = asc(j$ + chr$(0))` to avoid error with GET/GET#

**See:** Appendix C for PETSCII values

---

### ATN

**Type:** Numeric Function
**Format:** `ATN(<number>)`

Returns the arctangent (inverse tangent) in radians. Result is always in range -π/2 to +π/2.

**Examples:**
```basic
10 print atn(0)        : rem Returns 0
20 x = atn(j) * 180 / π  : rem Convert to degrees
```

---

### CHR$

**Type:** String Function
**Format:** `CHR$(<number>)`

Converts a PETSCII code (0-255) to its character equivalent.

**Examples:**
```basic
10 print chr$(65)      : rem Prints "A"
20 a$ = chr$(13)       : rem RETURN key
30 print chr$(147)     : rem Clear screen
```

**Range:** 0-255
**Error:** ?ILLEGAL QUANTITY if outside range

**Common values:**
- 13: RETURN/carriage return
- 17: Cursor down
- 19: HOME
- 29: Cursor right
- 147: CLR/HOME (clear screen)
- 157: Cursor left
- 145: Cursor up

---

### CLOSE

**Type:** I/O Statement
**Format:** `CLOSE <file number>`

Closes a file opened with OPEN. Essential for tape/disk to write incomplete buffers.

**Examples:**
```basic
10 close 1
20 close x
30 close 9 * (1 + j)
```

**Important:** Always CLOSE files before program ends, especially tape/disk files.

---

### CLR

**Type:** Statement
**Format:** `CLR`

Clears all variables, arrays, FN definitions, FOR/NEXT loops, and GOSUB return addresses. Program remains untouched.

**Example:**
```basic
10 x = 25
20 clr
30 print x     : rem Prints 0
```

**Warning:** Does NOT properly close files. Data may be lost. Use CLOSE before CLR.

---

### CMD

**Type:** I/O Statement
**Format:** `CMD <file number>[,<string>]`

Redirects output from screen to specified file. All PRINT and LIST output goes to the file.

**Examples:**
```basic
open 4,4: cmd 4, "title": list : rem List to printer
print# 4: close 4                  : rem Un-listen device
```

**To restore screen output:** Send blank line with PRINT# before CLOSE
**Side effect:** Any error returns output to screen

---

### CONT

**Type:** Command
**Format:** `CONT`

Continues program execution after STOP, END, or RUN/STOP key.

**Example:**
```basic
10 pi=0: c=1
20 pi=pi+4/c-4/(c+2)
30 print pi
40 c=c+4: goto 20

run
[press RUN/STOP]
break in 20
cont        : rem Continues from line 20
```

**Limitations:**
- ?CAN'T CONTINUE if program edited
- ?CAN'T CONTINUE if error occurred
- ?CAN'T CONTINUE if error caused before typing CONT

---

### COS

**Type:** Numeric Function
**Format:** `COS(<number>)`

Returns the cosine of the argument (in radians).

**Examples:**
```basic
10 print cos(0)        : rem Returns 1
20 x = cos(y * π / 180)  : rem Convert degrees to radians
```

---

### DATA

**Type:** Statement
**Format:** `DATA <constant>[,<constant>]...`

Stores data constants in program. Read with READ statement.

**Examples:**
```basic
10 data 1, 10, 5, 8
20 data john, paul, george, ringo
30 data "dear mary, how are you, love, bill"
40 data -1.7e-9, 3.33
```

**Rules:**
- DATA statements don't need to be executed
- Usually placed at end of program
- All DATA forms one continuous list
- Quotes required for: comma, colon, spaces, shifted chars
- Type mismatch causes ?SYNTAX ERROR (shows DATA line number)

---

### DEF FN

**Type:** Statement
**Format:** `DEF FN<name>(<variable>) = <expression>`

Defines a user function. Must be executed once before use.

**Function naming:** FN + letter or letter+digit (FNA, FNAA, FNA9)

**Examples:**
```basic
10 def fna(x) = x + 7
20 def fnaa(x) = y * z
30 def fna9(q) = int(rnd(1)*q+1)

40 print fna(9)     : rem Calls function
50 r = fnaa(9)
60 g = g + fna9(10)
```

**Note:** Variable in parentheses may be ignored by formula

---

### DIM

**Type:** Statement
**Format:** `DIM <variable>(<subscripts>)[,<variable>(<subscripts>)]...`

Dimensions (allocates) array storage. Must execute once per array.

**Examples:**
```basic
10 dim a(100)          : rem 101 elements (0-100)
20 dim z(5,7), y(3,4,5)
30 dim y7%(q)          : rem Integer array
40 dim ph$(1000)       : rem String array
50 f(4)=9              : rem Auto-DIM to 11 elements (0-10)
```

**Rules:**
- Lowest subscript is 0
- Maximum subscript: 32767
- Re-executing causes ?REDIM'D ARRAY error
- Undimensioned arrays auto-DIM to 11 elements per dimension
- Maximum 255 subscripts per array

**Memory usage:**
- 5 bytes for array name
- 2 bytes per dimension
- 2 bytes/element (integer %)
- 5 bytes/element (numeric)
- 3 bytes/element (string $) + 1 byte per character

---

### END

**Type:** Statement
**Format:** `END`

Ends program execution, displays READY. Allows CONT to resume.

**Examples:**
```basic
10 print "do you really want to run this program"
20 input a$
30 if a$ = "no" then end
40 rem rest of program
999 end
```

**Difference from STOP:** END shows "READY", STOP shows "BREAK IN XX"

---

### EXP

**Type:** Numeric Function
**Format:** `EXP(<number>)`

Returns e (2.71828183) raised to the power of the argument.

**Examples:**
```basic
10 print exp(1)        : rem Returns 2.71828183
20 x = y * exp(z * q)
```

**Limit:** Values > 88.0296919 cause ?OVERFLOW error

---

### FN

**Type:** Numeric Function
**Format:** `FN<name>(<number>)`

Calls a user-defined function created with DEF FN.

**Examples:**
```basic
print fna(q)
1100 j = fnj(7) + fnj(9)
9990 if fnb7(i+1) = 6 then end
```

**Error:** ?UNDEF'D FUNCTION if FN called before DEF

---

### FOR...TO...STEP

**Type:** Statement
**Format:** `FOR <variable>=<start> TO <limit> [STEP <increment>]`

Creates a counting loop. Must pair with NEXT.

**Examples:**
```basic
100 for l = 1 to 10         : rem Step defaults to 1
110 print l
120 next l

100 for l = 100 to 0 step -1   : rem Count down
100 for l = π to 6*π step .01
100 for aa = 3 to 3             : rem Execute once
```

**Rules:**
- Loop always executes at least once
- STEP defaults to +1 if omitted
- Positive STEP: exits when variable > limit
- Negative STEP: exits when variable < limit
- Nested loops: Maximum 9 levels

---

### FRE

**Type:** Numeric Function
**Format:** `FRE(<dummy>)`

Returns available RAM in bytes. Argument ignored.

**Examples:**
```basic
print fre(0)
10 x = (fre(k)-1000) / 7
950 if fre(0) < 100 then print "not enough room"
```

**Note:** If negative, add 65536 for actual bytes available
**Always accurate:** `print fre(0)-(fre(0)<0)*65536`

---

### GET

**Type:** Statement
**Format:** `GET <variable list>`

Reads keyboard buffer one character at a time. Non-blocking (returns "" if no key).

**Examples:**
```basic
10 get a$: if a$ = "" then 10  : rem Wait for key
20 get a$, b$, c$, d$, e$      : rem Read 5 keys
30 get a, a$
```

**Important:** Use string variables to avoid ?SYNTAX ERROR with non-numeric keys

---

### GET#

**Type:** I/O Statement
**Format:** `GET# <file number>, <variable list>`

Reads one character from file/device. Returns "" if no data.

**Examples:**
```basic
5 get#1, a$
10 open 1,3: get#1, z7$        : rem Read from screen
20 get#1, a, b, c$, d$
```

**Screen device:** Moves cursor right, CHR$(13) at end of line

---

### GOSUB

**Type:** Statement
**Format:** `GOSUB <line number>`

Calls a subroutine. Returns with RETURN to line after GOSUB.

**Example:**
```basic
10 print "this is the program"
20 gosub 1000
30 print "program continues"
40 gosub 1000
50 print "more program"
60 end
1000 print "this is the gosub": return
```

**Stack:** Uses 256-byte stack. Too many nested GOSUBs cause ?OUT OF MEMORY

---

### GOTO

**Type:** Statement
**Format:** `GOTO <line number>` or `GO TO <line number>`

Jumps to specified line number.

**Examples:**
```basic
goto 100
10 go to 50
20 goto 999
```

**Warning:** Can create infinite loops (use RUN/STOP to break)

---

### IF...THEN

**Type:** Statement
**Format:**
- `IF <expression> THEN <line number>`
- `IF <expression> GOTO <line number>`
- `IF <expression> THEN <statements>`

Conditional execution based on expression truth.

**Examples:**
```basic
100 input "type a number"; n
110 if n <= 0 goto 200
120 print "square root=" sqr(n)
130 goto 100
200 print "number must be >0"
210 goto 100

110 if rnd(1)< .5 then x = x + 1 : goto 130
```

**Note:** "THEN GOTO" not needed, "GOTO" implies "THEN GOTO"

---

### INPUT

**Type:** Statement
**Format:** `INPUT ["<prompt>";] <variable list>`

Gets user input from keyboard. Displays "?" prompt.

**Examples:**
```basic
100 input a
110 input b, c, d
120 input "prompt"; e
```

**Prompts:**
- `?` - Ready for input
- `??` - Need more input (not enough items entered)
- `?REDO FROM START` - Type mismatch (string when number expected)
- `?EXTRA IGNORED` - Too many items entered

**Important:** Cannot use outside program (needs buffer space)

---

### INPUT#

**Type:** I/O Statement
**Format:** `INPUT# <file number>, <variable list>`

Reads data from file. Variables up to 80 characters.

**Delimiters:** RETURN (13), comma, semicolon, colon
**Quotes:** Can enclose delimiters in data

**Examples:**
```basic
10 input#1, a
20 input#2, a$, b$
```

**Errors:**
- ?BAD DATA if type mismatch
- ?STRING TOO LONG if > 80 chars

**Screen device:** Reads entire logical line, moves cursor down

---

### INT

**Type:** Numeric Function
**Format:** `INT(<number>)`

Returns integer part of number.
- Positive: Drops fraction
- Negative: Returns next lower integer

**Examples:**
```basic
120 print int(99.4343), int(-12.34)
run
99    -13
```

---

### LEFT$

**Type:** String Function
**Format:** `LEFT$(<string>, <count>)`

Returns leftmost `<count>` characters. Range 0-255.

**Examples:**
```basic
10 a$ = "commodore computers"
20 b$ = left$(a$, 9): print b$
run
commodore
```

**Special cases:**
- Count > length: Returns entire string
- Count = 0: Returns null string ""

---

### LEN

**Type:** Numeric Function
**Format:** `LEN(<string>)`

Returns length of string including spaces and non-printing characters.

**Example:**
```basic
cc$ = "commodore computer": print len(cc$)
18
```

---

### LET

**Type:** Statement
**Format:** `[LET] <variable> = <expression>`

Assigns value to variable. LET keyword is optional (rarely used).

**Examples:**
```basic
10 let d = 12       : rem Same as d=12
20 let e$ = "abc"
30 f$ = "words"
40 sum$ = e$ + f$   : rem Result: "abcwords"
```

---

### LIST

**Type:** Command
**Format:** `LIST [[<first>][-[<last>]]]`

Displays program lines. Can be redirected with CMD.

**Examples:**
```basic
list             : rem All lines
list 500         : rem Line 500 only
list 150-        : rem 150 to end
list -1000       : rem Start to 1000
list 150-1000    : rem Lines 150-1000
```

**Control:** Hold CTRL to slow scrolling, RUN/STOP to abort

**In program:** Returns to READY after listing

---

### LOAD

**Type:** Command
**Format:** `LOAD ["<filename>"][,<device>][,<address>]`

Loads program from tape or disk.

**Examples:**
```basic
load                    : rem Next program on tape
load "*",8              : rem First program on disk
load "",1,1             : rem Load to original address
load "star trek"        : rem Named program from tape
load "fun",8            : rem Named program from disk
```

**Defaults:**
- Device: 1 (tape) if omitted
- Address: 2048 unless secondary address = 1

**Effects:**
- Direct mode: Performs CLR before loading
- Program mode: RUNs after loading (chaining)

---

### LOG

**Type:** Numeric Function
**Format:** `LOG(<number>)`

Returns natural logarithm (base e).

**Examples:**
```basic
25 print log(45/7)
run
1.86075234

10 num = log(arg)/log(10)  : rem Convert to base 10
```

**Error:** ?ILLEGAL QUANTITY if argument ≤ 0

---

### MID$

**Type:** String Function
**Format:** `MID$(<string>, <start>[, <length>])`

Returns substring starting at position `<start>`. Range 0-255.

**Examples:**
```basic
10 a$ = "good"
20 b$ = "morning evening afternoon"
30 print a$ + mid$(b$, 8, 8)
run
good evening
```

**Special cases:**
- Start > length: Returns ""
- Length = 0: Returns ""
- Length omitted: Returns rest of string
- Length > available: Returns rest of string

---

### NEW

**Type:** Command
**Format:** `NEW`

Clears program and all variables.

**Examples:**
```basic
new            : rem Clear everything
10 new         : rem In program: clears and stops
```

**Warning:** Always NEW before typing a new program to avoid mixing with old code

---

### NEXT

**Type:** Statement
**Format:** `NEXT [<variable>][,<variable>]...`

Closes FOR loop. Increments counter and tests limit.

**Examples:**
```basic
10 for j=1 to 5: for k=10 to 20: for n=5 to-5 step-1
20 next n, k, j    : rem Close nested loops

10 for l=1 to 100
20 for m=1 to 10
30 next m
400 next l          : rem Loops don't cross

10 for a=1 to 10
20 for b=1 to 20
30 next
40 next             : rem Variable names optional
```

**Nesting:** Maximum 9 levels

---

### NOT

**Type:** Logical Operator
**Format:** `NOT <expression>`

Returns bitwise complement (two's complement). Inverts true/false values.

**Examples:**
```basic
10 if not aa = bb and not (bb=cc) then...
nn% = not 96: print nn%
-97
```

**Formula:** NOT X = -(X+1)
**Logic:** -1 (all 1-bits) = true, 0 (all 0-bits) = false

---

### ON

**Type:** Statement
**Format:** `ON <variable> GOTO/GOSUB <line>[,<line>]...`

Branches to one of several lines based on variable value.

**Examples:**
```basic
on -(a=7) - 2*(a=3) - 3*(a<3) - 4*(a>7) goto 400, 900, 1000, 100
on x goto 100, 130, 180, 220
on x+3 gosub 9000, 20, 9000
100 on num goto 150, 300, 320, 390
500 on sum/2 + 1 gosub 50, 80, 20
```

**Rules:**
- Value 1: First line, 2: Second line, etc.
- Value 0 or > count: Ignores statement, continues
- Negative value: ?ILLEGAL QUANTITY error
- Fractional values: Fraction dropped

---

### OPEN

**Type:** I/O Statement
**Format:** `OPEN <file#>[,<device>][,<address>][,"<filename>[,<type>][,<mode>]"]`

Opens file/device for I/O.

**Parameters:**
- file#: Logical file number (1-255, recommend 1-127)
- device: 0=keyboard, 1=tape, 3=screen, 4=printer, 8=disk
- address: Secondary address (device-specific)
- filename: Up to 16 characters
- type: PRG, SEQ, REL (disk only)
- mode: R=read, W=write (disk only)

**Examples:**
```basic
10 open 2, 8, 4, "disk-output,seq,w"   : rem Disk sequential write
10 open 1, 1, 2, "tape-write"           : rem Tape write + EOT marker
10 open 50, 0                           : rem Keyboard
10 open 12, 3                           : rem Screen
10 open 130, 4                          : rem Printer
10 open 1, 8, 15, "command"             : rem Disk command channel
```

**Tape addresses:**
- 0: Read
- 1: Write
- 2: Write with end-of-tape marker
- 3: Both

**Error:** ?FILE NOT OPEN if accessed before opening

---

### OR

**Type:** Logical Operator
**Format:** `<operand> OR <operand>`

Bitwise OR operation on 16-bit signed integers (-32768 to +32767).

**Truth table:**
```
0 OR 0 = 0
0 OR 1 = 1
1 OR 0 = 1
1 OR 1 = 1
```

**Examples:**
```basic
100 if (aa=bb) or (xx=20) then...
230 kk% = 64 or 32: print kk%
96
```

**Logic:** Result true (-1) if either operand non-zero

---

### PEEK

**Type:** Numeric Function
**Format:** `PEEK(<address>)`

Reads byte (0-255) from memory address (0-65535).

**Examples:**
```basic
10 print peek(53280) and 15     : rem Border color
5 a% = peek(45)+peek(46)*256    : rem BASIC variable table address
```

**Error:** ?ILLEGAL QUANTITY if address outside range

---

### POKE

**Type:** Statement
**Format:** `POKE <address>, <value>`

Writes byte (0-255) to memory address (0-65535).

**Examples:**
```basic
poke 1024, 1        : rem Put "A" at screen position 1
poke 2040, ptr      : rem Update sprite 0 data pointer
10 poke red, 32
20 poke 36879, 8
2050 poke a, b
```

**Ranges:**
- Address: 0-65535
- Value: 0-255

**Error:** ?ILLEGAL QUANTITY if outside range

---

### POS

**Type:** Numeric Function
**Format:** `POS(<dummy>)`

Returns cursor column position (0-79) on 80-character logical line.

**Example:**
```basic
1000 if pos(0) > 38 then print chr$(13)  : rem Prevent wrap
```

**Note:** Positions 40-79 refer to second physical screen line

---

### PRINT

**Type:** Statement
**Format:** `PRINT [<expression>][<,/;><expression>]...`

Outputs data to screen (or device if CMD active).

**Punctuation:**
- `,` (comma): Tab to next 10-space zone
- `;` (semicolon): No space between items
- Nothing or blank: Space between items
- End comma/semicolon: Suppress carriage return

**Examples:**
```basic
5 x = 5
10 print -5*x, x-5, x+5, x↑5
-25    0    10    3125

5 x = 9
10 print x; "squared is";x*x;"and";
20 print x "cubed is" x↑3
9 squared is 81 and 9 cubed is 729
```

**Quote mode:** Cursor controls become reverse characters in strings

**Abbreviation:** `?` can be used instead of PRINT

---

### PRINT#

**Type:** I/O Statement
**Format:** `PRINT# <file#>[,<expression>][<,/;><expression>]...`

Writes data to file.

**Examples:**
```basic
10 open 1, 1, 1, "tape file"
20 r$ = chr$(13)
30 print#1, 1;r$;2;r$;3;r$;4;r$;5
40 print#1, 6
50 print#1, 7
```

**Tape files:** Comma acts like semicolon (no zones)

**Important:** End with PRINT# before CLOSE to "un-listen" device

---

### READ

**Type:** Statement
**Format:** `READ <variable>[,<variable>]...`

Reads data from DATA statements.

**Examples:**
```basic
110 read a, b, c$
120 data 1, 2, hello

100 for x=1 to 10: read a(x): next
200 data 3.08, 5.19, 3.12, 3.98, 4.24
210 data 5.08, 5.55, 4.00, 3.16, 3.37
```

**Errors:**
- ?OUT OF DATA: More READs than DATA items
- ?SYNTAX ERROR: Type mismatch (shows DATA line number)

---

### REM

**Type:** Statement
**Format:** `REM [<remark>]`

Comment statement. Everything after REM is ignored.

**Examples:**
```basic
10 rem calculate average velocity
20 for x=1 to 20: rem loop for twenty values
30 sum=sum + vel(x): next
40 avg=sum/20
```

**Usage:** Can be target of GOTO/GOSUB (execution continues with next line)

---

### RESTORE

**Type:** Statement
**Format:** `RESTORE`

Resets DATA pointer to first DATA item in program.

**Examples:**
```basic
100 for x=1 to 10: read a(x): next
200 restore
300 for y=1 to 10: read b(y): next
4000 data 3.08, 5.19, 3.12, 3.98, 4.24
4100 data 5.08, 5.55, 4.00, 3.16, 3.37
```

---

### RETURN

**Type:** Statement
**Format:** `RETURN`

Returns from GOSUB to statement after GOSUB call.

**Example:**
```basic
10 print "this is the program"
20 gosub 1000
30 print "program continues"
40 gosub 1000
50 print "more program"
60 end
1000 print "this is the gosub": return
```

**Note:** First RETURN encountered exits subroutine (can have multiple)

---

### RIGHT$

**Type:** String Function
**Format:** `RIGHT$(<string>, <count>)`

Returns rightmost `<count>` characters. Range 0-255.

**Examples:**
```basic
10 msg$ = "commodore computers"
20 print right$(msg$, 9)
run
computers
```

**Special cases:**
- Count > length: Returns entire string
- Count = 0: Returns null string ""

---

### RND

**Type:** Numeric Function
**Format:** `RND(<number>)`

Returns pseudo-random number 0.0 to 1.0.

**Argument effects:**
- Positive: Repeatable sequence from seed
- Zero: Use hardware clock (true random)
- Negative: Re-seed with each call

**Examples:**
```basic
220 print int(rnd(0)*50)              : rem Random 0-49
100 x = int(rnd(1)*6)+int(rnd(1)*6)+2 : rem Simulate 2 dice
100 x = int(rnd(1)*1000)+1            : rem Random 1-1000
100 x = int(rnd(1)*150)+100           : rem Random 100-249
100 x = rnd(1)*(u-l)+l                : rem Random between limits
```

---

### RUN

**Type:** Command
**Format:** `RUN [<line number>]`

Starts program execution. Performs CLR first.

**Examples:**
```basic
run            : rem Start at first line
run 500        : rem Start at line 500
run x          : rem Start at line x
```

**Can be used in programs:** Causes automatic RUN after statement completes

---

### SAVE

**Type:** Command
**Format:** `SAVE ["<filename>"][,<device>][,<address>]`

Saves program to tape or disk.

**Examples:**
```basic
save                 : rem Tape, no name
save "alpha", 1      : rem Tape, named
save "alpha", 1, 2   : rem Tape + EOT marker
save "fun.disk", 8   : rem Disk
save a$              : rem Tape, name in variable
10 save "hi"         : rem In program
save "me", 1, 3      : rem Tape, save address + EOT
```

**Tape addresses:**
- 1: Load to save address later
- 2: Write end-of-tape marker
- 3: Both

**Note:** Tape programs saved twice automatically for error checking

---

### SGN

**Type:** Numeric Function
**Format:** `SGN(<number>)`

Returns sign of number: -1 (negative), 0 (zero), or 1 (positive).

**Example:**
```basic
90 on sgn(dv)+2 goto 100, 200, 300
rem Jump to 100 if negative, 200 if zero, 300 if positive
```

---

### SIN

**Type:** Numeric Function
**Format:** `SIN(<number>)`

Returns sine of argument (in radians).

**Example:**
```basic
235 aa = sin(1.5): print aa
.997494987
```

---

### SPC

**Type:** Special Function
**Format:** `SPC(<count>)`

Prints `<count>` spaces. Use only with PRINT or PRINT#. Range 0-255 (254 for disk).

**Example:**
```basic
10 print "right "; "here &";
20 print spc(5) "over" spc(14) "there"
run
right here &     over              there
```

---

### SQR

**Type:** Numeric Function
**Format:** `SQR(<number>)`

Returns square root of argument.

**Example:**
```basic
for j=2 to 5: print j*5, sqr(j*5): next
10    3.16227766
15    3.87298335
20    4.47213595
25    5
ready.
```

**Error:** ?ILLEGAL QUANTITY if argument negative

---

### STATUS (ST)

**Type:** Numeric Function
**Format:** `STATUS` or `ST`

Returns I/O status byte (0-255) for last operation.

**Status bits:**

| Bit | Value | Cassette | Serial Bus | Tape Verify |
|-----|-------|----------|------------|-------------|
| 0 | 1 | - | Timeout write | - |
| 1 | 2 | - | Timeout read | - |
| 2 | 4 | Short block | - | Short block |
| 3 | 8 | Long block | - | Long block |
| 4 | 16 | Unrecoverable read error | - | Any mismatch |
| 5 | 32 | Checksum error | - | Checksum error |
| 6 | 64 | End of file | EOI | - |
| 7 | 128 | End of tape | Device not present | End of tape |

**Example:**
```basic
10 open 1,4: open 2,8,4,"master file,seq,w"
20 gosub 100: rem check status
30 input#2, a$, b, c
40 if status and 64 then 80: rem handle end-of-file
50 gosub 100: rem check status
60 print#1, a$, b; c
70 goto 20
80 close1: close2
90 gosub 100: end
100 if st > 0 then 9000: rem handle i/o error
110 return
```

---

### STEP

**Type:** Statement
**Format:** `[STEP <increment>]`

Defines FOR loop increment. Defaults to +1 if omitted.

**Examples:**
```basic
25 for xx = 2 to 20 step 2     : rem Loop 10 times
35 for zz = 0 to -20 step -2   : rem Loop 11 times
```

**Note:** STEP value cannot be changed once in loop

---

### STOP

**Type:** Statement
**Format:** `STOP`

Halts program, displays "BREAK IN XX", returns to direct mode. Allows CONT to resume.

**Examples:**
```basic
10 input#1, aa, bb, cc
20 if aa = bb and bb = cc then stop
30 stop
```

**Difference from END:** STOP shows "BREAK IN XX", END shows "READY"

---

### STR$

**Type:** String Function
**Format:** `STR$(<number>)`

Converts number to string representation.

**Example:**
```basic
100 flt = 1.5e4: alpha$ = str$(flt)
110 print flt, alpha$
15000    15000
```

**Note:** Positive numbers preceded by space, all numbers followed by space

---

### SYS

**Type:** Statement
**Format:** `SYS <address>`

Calls machine language routine at specified address. Must end with RTS.

**Examples:**
```basic
sys 64738              : rem Jump to system cold start
10 poke 4400,96: sys 4400  : rem Execute RTS at 4400, return immediately
```

---

### TAB

**Type:** Special Function
**Format:** `TAB(<column>)`

Moves cursor to column position. Range 0-255. Use only with PRINT (not PRINT#).

**Example:**
```basic
100 print "name" tab(25) "amount": print
110 input#1, nam$, amt$
120 print nam$ tab(25) amt$

name                     amount
g.t. jones               25.
```

---

### TAN

**Type:** Numeric Function
**Format:** `TAN(<number>)`

Returns tangent of argument (in radians).

**Example:**
```basic
10 xx = .785398163: yy = tan(xx): print yy
1
```

**Error:** ?DIVISION BY ZERO if overflow

---

### THEN

**Type:** Statement
**Format:** Used with IF

See IF...THEN for details.

---

### TIME (TI)

**Type:** Numeric Function
**Format:** `TI`

Returns jiffy clock value (1/60 second intervals since power-on). Resets to 0 at power-up.

**Example:**
```basic
10 print ti/60 "seconds since power up"
```

**Note:** Stops during tape I/O

---

### TIME$ (TI$)

**Type:** String Function
**Format:** `TI$`

Returns/sets time as 6-digit string "HHMMSS".

**Example:**
```basic
1 ti$ = "000000": for j=1 to 10000: next: print ti$
000011
```

**Note:** Not accurate after tape I/O. Can be assigned new value.

---

### TO

**Type:** Statement
**Format:** Used with FOR

See FOR...TO...STEP for details.

---

### USR

**Type:** Numeric Function
**Format:** `USR(<number>)`

Calls machine language routine pointed to by locations 785-786. Must set pointer first.

**Examples:**
```basic
10 b = t * sin(y)
20 c = usr(b/2)
30 d = usr(b/3)
```

**Setup:** Use POKE 785,low-byte: POKE 786,high-byte before calling

**Error:** ?ILLEGAL QUANTITY if pointer not set

---

### VAL

**Type:** Numeric Function
**Format:** `VAL(<string>)`

Converts string to numeric value. Returns 0 if first character is not +, -, or digit.

**Example:**
```basic
10 input#1, nam$, zip$
20 if val(zip$) < 19400 or val(zip$) > 96699 then print nam$ tab(25) "greater philadelphia"
```

**Conversion stops:** At end of string or non-numeric character (except . or E)

---

### VERIFY

**Type:** Command
**Format:** `VERIFY ["<filename>"][,<device>]`

Compares program in memory with file on tape/disk.

**Examples:**
```basic
verify               : rem Check 1st program on tape
9000 save "me",8
9010 verify "me",8   : rem Verify disk file
```

**Default device:** 1 (tape)

**Error:** ?VERIFY ERROR if any differences found

**Use:** Typically after SAVE to ensure correct storage

---

### WAIT

**Type:** Statement
**Format:** `WAIT <location>, <mask1>[, <mask2>]`

Suspends execution until memory location bits match pattern.

**Operation:**
1. Read location
2. AND with mask1
3. XOR with mask2 (if present)
4. If result non-zero, continue; else repeat

**Examples:**
```basic
wait 1, 32, 32            : rem Wait for tape key press
wait 53273, 6, 6          : rem Wait for sprite collision
wait 36868, 144, 16       : rem Complex bit test
```

**Recovery:** RUN/STOP + RESTORE if infinite wait

**Warning:** Rarely needed in BASIC programs

---

## Special Topics

### Abbreviations

- Entered by typing enough letters to distinguish keyword, with SHIFT on last letter
- Do NOT save memory (keywords tokenize to single byte)
- Can exceed 80-char line limit, but line becomes uneditable when listed
- When listed, keywords appear fully spelled

**Example:** `P SHIFT-R` becomes `PRINT`

### 80-Character Line Limit

- Screen Editor works on 80-character logical lines (2 physical lines)
- Lines over 80 characters cannot be edited when listed
- Workaround: Retype with abbreviations or split into multiple lines
- Affects INPUT statement: Maximum 80 characters total input

### Quote Mode

Activated by odd number of `"` characters. In quote mode:
- Cursor controls show as reverse characters
- Color controls show as reverse characters
- INST/DEL key shows as reverse character
- Allows embedding control codes in strings

**To exit quote mode:**
- Type closing `"`
- Press RUN/STOP + RESTORE

### Insert Mode

Activated by SHIFT+INST. Similar to quote mode but:
- DEL key creates reverse T character
- INST key inserts spaces normally
- Allows creating strings with DEL characters

**To exit insert mode:**
- Fill all inserted spaces
- Press RETURN
- Press RUN/STOP + RESTORE

### Special Control Characters

**Type in reverse mode (CTRL+RVS/ON, then key):**

| Function | Type | Appears As |
|----------|------|------------|
| SHIFT-RETURN | SHIFT M | ▒ |
| Switch to lower case | N | ▒ |
| Switch to upper case | SHIFT N | ▒ |

### Color Control Characters

**Press CTRL or Commodore key + number:**

| Key | Color | Appears As |
|-----|-------|------------|
| CTRL 1 | Black | ■ |
| CTRL 2 | White | ▒ |
| CTRL 3 | Red | ▒ |
| CTRL 4 | Cyan | ▒ |
| CTRL 5 | Purple | ▒ |
| CTRL 6 | Green | ▒ |
| CTRL 7 | Blue | ▒ |
| CTRL 8 | Yellow | ▒ |
| CBM 1 | Orange | ▒ |
| CBM 2 | Brown | ▒ |
| CBM 3 | Light Red | ▒ |
| CBM 4 | Grey 1 | ▒ |
| CBM 5 | Grey 2 | ▒ |
| CBM 6 | Light Green | ▒ |
| CBM 7 | Light Blue | ▒ |
| CBM 8 | Grey 3 | ▒ |

---

## Operators

### Arithmetic Operators

| Operator | Operation | Example |
|----------|-----------|---------|
| + | Addition | `a + b` |
| - | Subtraction | `a - b` |
| * | Multiplication | `a * b` |
| / | Division | `a / b` |
| ↑ | Exponentiation | `a ↑ b` |

### Relational Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| = | Equal | `if a = b then` |
| < | Less than | `if a < b then` |
| > | Greater than | `if a > b then` |
| <= | Less or equal | `if a <= b then` |
| >= | Greater or equal | `if a >= b then` |
| <> | Not equal | `if a <> b then` |

### Logical Operators

| Operator | Operation | Returns |
|----------|-----------|---------|
| AND | Bitwise AND | Integer -32768 to +32767 |
| OR | Bitwise OR | Integer -32768 to +32767 |
| NOT | Bitwise complement | Integer -32768 to +32767 |

**Logic values:**
- False = 0
- True = -1 (all 1-bits)
- Non-zero values treated as true in conditions

---

## Variable Types

### Numeric Variables

**Floating-point (default):**
- Range: ±1.70141183E+38
- Precision: ~9 digits
- Storage: 5 bytes
- Example: `x`, `total`, `velocity`

**Integer:**
- Suffix: `%`
- Range: -32768 to +32767
- Storage: 2 bytes
- Example: `count%`, `index%`

### String Variables

- Suffix: `$`
- Maximum length: 255 characters
- Storage: 3 bytes + 1 per character
- Example: `name$`, `message$`

### Array Variables

- Must be dimensioned with DIM (except auto-DIM to 11 elements)
- Same type suffixes apply (`%` for integer, `$` for string)
- Subscripts start at 0
- Example: `scores(10)`, `names$(20)`

---

## Common Errors

| Error Message | Cause | Solution |
|---------------|-------|----------|
| ?SYNTAX ERROR | Invalid statement syntax | Check statement format |
| ?ILLEGAL QUANTITY | Number out of range | Check ranges for functions |
| ?OUT OF MEMORY | No RAM available | Remove variables/arrays, shorten program |
| ?UNDEF'D STATEMENT | GOTO/GOSUB to non-existent line | Check line number exists |
| ?UNDEF'D FUNCTION | FN called before DEF | Execute DEF before FN call |
| ?REDIM'D ARRAY | DIM executed twice | Execute DIM only once |
| ?OUT OF DATA | More READs than DATA | Add more DATA or check logic |
| ?FILE NOT OPEN | I/O without OPEN | OPEN file before use |
| ?FILE NOT FOUND | LOAD/VERIFY non-existent file | Check filename/device |
| ?CAN'T CONTINUE | CONT after edit/error | Can't continue after program edit |
| ?DIVISION BY ZERO | Divide by zero | Check divisor not zero |
| ?OVERFLOW | Number too large | Use smaller values |
| ?STRING TOO LONG | String > 255 chars | Shorten string |
| ??EXTRA IGNORED | Too much INPUT data | Enter fewer items |
| ?REDO FROM START | INPUT type mismatch | Enter correct data type |

---

## Memory Locations

### Keyboard

- **197 ($00C5):** Current key pressed (64 if none)
- **198 ($00C6):** Keyboard buffer count
- **631-640 ($0277-$0280):** Keyboard buffer (10 chars)

### Screen Editor

- **53272 ($D018):** VIC memory control
- **53280 ($D020):** Border color
- **53281 ($D021):** Background color

### Timers

- **TI:** Jiffy clock (1/60 second intervals)
- **TI$:** Time string "HHMMSS"

### BASIC Pointers

- **45-46:** Start of variable storage
- **785-786:** USR function vector

---

## Tips for Lesson Content

### Verify in lessons:

1. **Keywords exist:** Only use keywords from this reference
2. **Syntax correct:** Match FORMAT specifications exactly
3. **Ranges valid:** Check numeric ranges for functions
4. **Operators exist:** AND/OR/NOT are valid in C64 BASIC V2
5. **String functions:** Only LEFT$, RIGHT$, MID$, CHR$, STR$ available
6. **No line indentation:** BASIC V2 has no indentation
7. **80-char limit:** Keep lines under 80 characters when displayed
8. **Abbreviations:** Document that they don't save memory

### Common misconceptions to avoid:

- **FALSE:** "BASIC V2 has no AND/OR operators" → They exist!
- **FALSE:** "Abbreviations save memory" → They tokenize the same
- **FALSE:** "String operations are fast" → They're quite slow
- **FALSE:** "Arrays start at 1" → They start at 0
- **FALSE:** "BASIC has string slicing" → Only LEFT$, RIGHT$, MID$

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 2
- **Related:** PETSCII-SCREEN-CODES.md for character codes
- **Related:** BASIC-V2-REFERENCE.md for general BASIC programming guide

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore 64 Programmer's Reference Guide

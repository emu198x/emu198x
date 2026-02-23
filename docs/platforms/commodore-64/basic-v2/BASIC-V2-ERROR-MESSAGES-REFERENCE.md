# BASIC V2 Error Messages Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Appendix K

---

## Overview

Commodore 64 BASIC displays error messages when it encounters problems during program execution or in direct mode. Understanding these messages is essential for debugging programs and writing reliable code.

**Error message format:**
```
?ERROR MESSAGE
ERROR IN LINE
```

**Key principles:**
- All error messages begin with `?` (question mark)
- Error messages halt program execution
- Use CONT to continue after some errors (if possible)
- Some errors prevent CONT from working

---

## Complete Error Messages

### BAD DATA

**Message:** `?BAD DATA ERROR IN <line>`

**Cause:** String data was received from an open file, but the program was expecting numeric data.

**Common scenarios:**
- Reading from a file with INPUT# expecting a number, but file contains text
- Data type mismatch in file I/O operations

**Example:**
```basic
10 OPEN 1,8,2,"DATA,S,R"
20 INPUT#1,A : REM Expecting number
30 REM Error if file contains "HELLO" instead of "123"
```

**Solution:**
- Check file contents match expected data types
- Use string variables for text input, then convert with VAL() if needed
- Verify file format before reading

---

### BAD SUBSCRIPT

**Message:** `?BAD SUBSCRIPT ERROR IN <line>`

**Cause:** The program was trying to reference an element of an array whose number is outside of the range specified in the DIM statement.

**Common scenarios:**
- Array index less than 0
- Array index greater than dimensioned size
- Using negative subscripts

**Example:**
```basic
10 DIM A(10) : REM Array has elements 0-10
20 A(11) = 5 : REM Error: subscript too large
30 A(-1) = 3 : REM Error: negative subscript
```

**Solution:**
- Check array bounds before accessing
- Remember arrays start at subscript 0, not 1
- DIM A(10) creates 11 elements (0 through 10)

---

### BREAK

**Message:** `BREAK IN <line>`

**Cause:** Program execution was stopped because you hit the RUN/STOP key.

**Common scenarios:**
- User manually interrupts program
- Infinite loop that needs to be stopped
- Long-running calculation

**Example:**
```basic
10 FOR I = 1 TO 10000
20 PRINT I
30 NEXT I
REM User presses RUN/STOP during execution
```

**Solution:**
- Not an error - user-initiated stop
- Use CONT to resume if desired
- Use LIST to view program
- Use GOTO <line> to restart at specific point

---

### CAN'T CONTINUE

**Message:** `?CAN'T CONTINUE`

**Cause:** The CONT command will not work, either because the program was never RUN, there has been an error, or a line has been edited.

**Common scenarios:**
- Trying to CONT without previously running a program
- Editing a program line after a BREAK
- Attempting to continue after certain errors
- Program variables have been modified

**Example:**
```basic
10 PRINT "HELLO"
RUN
BREAK IN 10
LIST 10 : REM Viewing line is OK
10 PRINT "GOODBYE" : REM Editing prevents CONT
CONT
?CAN'T CONTINUE
```

**Solution:**
- Use RUN instead of CONT after editing
- Don't modify program during debugging
- Save program state if changes needed

---

### DEVICE NOT PRESENT

**Message:** `?DEVICE NOT PRESENT ERROR IN <line>`

**Cause:** The required I/O device was not available for an OPEN, CLOSE, CMD, PRINT#, INPUT#, or GET#.

**Common scenarios:**
- Disk drive not connected or powered off
- Wrong device number specified
- Printer not ready
- Device malfunction

**Example:**
```basic
10 OPEN 1,8,15 : REM Device 8 = disk drive
?DEVICE NOT PRESENT ERROR IN 10
```

**Solution:**
- Verify device is connected and powered on
- Check device number (8 = disk, 4 = printer, etc.)
- Test device with simple LOAD command
- Check serial cable connections

---

### DIVISION BY ZERO

**Message:** `?DIVISION BY ZERO ERROR IN <line>`

**Cause:** Division by zero is a mathematical oddity and not allowed.

**Common scenarios:**
- Dividing by a variable that equals zero
- Division in formula evaluation
- Mathematical calculations with unexpected zero values

**Example:**
```basic
10 A = 5
20 B = 0
30 C = A/B : REM Error: division by zero
```

**Solution:**
- Check denominator before division
- Add bounds checking for calculations
- Use IF statements to test for zero

**Prevention example:**
```basic
10 A = 5
20 B = 0
30 IF B <> 0 THEN C = A/B ELSE C = 0
```

---

### EXTRA IGNORED

**Message:** `?EXTRA IGNORED`

**Cause:** Too many items of data were typed in response to an INPUT statement. Only the first few items were accepted.

**Common scenarios:**
- User types more values than INPUT expects
- Comma-separated input with too many items
- Misunderstanding of how many values are requested

**Example:**
```basic
10 INPUT "ENTER TWO NUMBERS";A,B
? 1,2,3,4,5
?EXTRA IGNORED
```

**Solution:**
- Clear prompts indicating how many values needed
- Accept exact number of inputs
- Use multiple INPUT statements if needed

---

### FILE NOT FOUND

**Message:** `?FILE NOT FOUND ERROR IN <line>`

**Cause:**
- **Tape:** END-OF-TAPE marker was found before file
- **Disk:** No file with that name exists

**Common scenarios:**
- Misspelled filename
- Wrong disk inserted
- File doesn't exist
- Cassette tape not positioned correctly

**Example:**
```basic
10 LOAD "PROGRAM",8
?FILE NOT FOUND ERROR IN 10
```

**Solution:**
- Use disk directory to verify filename: `LOAD "$",8` then `LIST`
- Check spelling and capitalization
- Verify correct disk/tape is inserted
- Use correct device number

---

### FILE NOT OPEN

**Message:** `?FILE NOT OPEN ERROR IN <line>`

**Cause:** The file specified in a CLOSE, CMD, PRINT#, INPUT#, or GET# must first be OPENed.

**Common scenarios:**
- Forgetting to OPEN before reading/writing
- Using wrong file number
- File was already CLOSEd

**Example:**
```basic
10 INPUT#1,A$ : REM Error: file 1 not opened
```

**Solution:**
- Always OPEN before file operations
- Track which file numbers are in use
- Match file numbers between OPEN and I/O commands

**Correct example:**
```basic
10 OPEN 1,8,2,"DATA,S,R"
20 INPUT#1,A$
30 CLOSE 1
```

---

### FILE OPEN

**Message:** `?FILE OPEN ERROR IN <line>`

**Cause:** An attempt was made to open a file using the number of an already open file.

**Common scenarios:**
- Opening same file number twice
- Not closing files before reusing numbers
- Logic error in file handling

**Example:**
```basic
10 OPEN 1,8,2,"DATA1,S,R"
20 OPEN 1,8,2,"DATA2,S,R" : REM Error: file 1 already open
```

**Solution:**
- Close file before reopening same number
- Use different file numbers for multiple files
- Track open files in complex programs

**Prevention:**
```basic
10 OPEN 1,8,2,"DATA1,S,R"
20 CLOSE 1
30 OPEN 1,8,2,"DATA2,S,R" : REM OK now
```

---

### FORMULA TOO COMPLEX

**Message:** `?FORMULA TOO COMPLEX ERROR IN <line>`

**Cause:**
- String expression being evaluated should be split into at least two parts
- Formula has too many parentheses
- Expression exceeds internal stack limits

**Common scenarios:**
- Very long string concatenations
- Deeply nested mathematical expressions
- Complex string operations

**Example:**
```basic
10 A$ = "ONE" + "TWO" + "THREE" + "FOUR" + "FIVE" + "SIX" + "SEVEN" + "EIGHT"
```

**Solution:**
- Break complex expressions into multiple statements
- Use intermediate variables
- Simplify nested operations

**Fixed example:**
```basic
10 A$ = "ONE" + "TWO" + "THREE" + "FOUR"
20 A$ = A$ + "FIVE" + "SIX"
30 A$ = A$ + "SEVEN" + "EIGHT"
```

---

### ILLEGAL DIRECT

**Message:** `?ILLEGAL DIRECT`

**Cause:** The INPUT statement can only be used within a program, and not in direct mode.

**Common scenarios:**
- Typing INPUT directly without line number
- Testing INPUT statement outside program
- Attempting interactive input from immediate mode

**Example:**
```basic
INPUT A$ : REM Error: can't use INPUT in direct mode
```

**Solution:**
- Use INPUT only in program lines
- For direct mode testing, assign values directly
- Remember: INPUT requires program context

---

### ILLEGAL QUANTITY

**Message:** `?ILLEGAL QUANTITY ERROR IN <line>`

**Cause:** A number used as the argument of a function or statement is out of the allowable range.

**Common scenarios:**
- Negative value where positive required
- Value outside valid range for function
- Screen coordinates out of bounds
- Color values > 15
- Sound frequencies out of range

**Example:**
```basic
10 POKE 53280,-1 : REM Error: negative color value
20 PRINT CHR$(256) : REM Error: CHR$ range is 0-255
```

**Solution:**
- Check function parameter ranges
- Validate user input before using
- Refer to function documentation for valid ranges

**Valid ranges:**
- CHR$: 0-255
- POKE address: 0-65535
- POKE value: 0-255
- Color values: 0-15
- Sound frequencies: vary by register

---

### LOAD

**Message:** `?LOAD ERROR`

**Cause:** There is a problem with the program on tape.

**Common scenarios:**
- Tape read error
- Damaged cassette
- Wrong playback speed
- Tape head dirty or misaligned

**Solution:**
- Clean tape heads
- Adjust cassette azimuth
- Try different tape
- Reload from backup copy

---

### NEXT WITHOUT FOR

**Message:** `?NEXT WITHOUT FOR ERROR IN <line>`

**Cause:**
- Incorrectly nesting loops
- Variable name in NEXT doesn't correspond to one in FOR statement

**Common scenarios:**
- NEXT statement without matching FOR
- Misspelled loop variable
- Crossed nested loops

**Example:**
```basic
10 FOR I = 1 TO 10
20 NEXT J : REM Error: J doesn't match I
```

**Nested loop error:**
```basic
10 FOR I = 1 TO 10
20 FOR J = 1 TO 5
30 NEXT I : REM Error: should be NEXT J first
40 NEXT J
```

**Solution:**
- Match FOR and NEXT variables exactly
- Nest loops properly (inner loops complete before outer)
- Can omit variable in NEXT for simplicity

**Correct nesting:**
```basic
10 FOR I = 1 TO 10
20 FOR J = 1 TO 5
30 NEXT J : REM Inner loop closes first
40 NEXT I : REM Outer loop closes last
```

---

### NOT INPUT FILE

**Message:** `?NOT INPUT FILE ERROR IN <line>`

**Cause:** An attempt was made to INPUT or GET data from a file which was specified to be for output only.

**Common scenarios:**
- Opening file for write, then trying to read
- Wrong file mode in OPEN statement
- Logic error in file direction

**Example:**
```basic
10 OPEN 1,8,2,"DATA,S,W" : REM W = write only
20 INPUT#1,A$ : REM Error: can't input from write-only file
```

**Solution:**
- Use correct mode in OPEN: R = read, W = write
- Open separate channels for read and write
- Check file mode before I/O operations

---

### NOT OUTPUT FILE

**Message:** `?NOT OUTPUT FILE ERROR IN <line>`

**Cause:** An attempt was made to PRINT data to a file which was specified as input only.

**Common scenarios:**
- Opening file for read, then trying to write
- Wrong file mode in OPEN statement
- Trying to write to read-only file

**Example:**
```basic
10 OPEN 1,8,2,"DATA,S,R" : REM R = read only
20 PRINT#1,"HELLO" : REM Error: can't print to read-only file
```

**Solution:**
- Use W or A mode for writing
- Verify file mode matches operation
- Use separate channels for bidirectional I/O

---

### OUT OF DATA

**Message:** `?OUT OF DATA ERROR IN <line>`

**Cause:** A READ statement was executed but there is no data left unREAD in a DATA statement.

**Common scenarios:**
- More READ statements than DATA values
- Loop reading beyond available data
- Forgot to add all DATA statements

**Example:**
```basic
10 DATA 1,2,3
20 FOR I = 1 TO 5
30 READ A : REM Error on 4th iteration
40 NEXT I
```

**Solution:**
- Count DATA values to match READ statements
- Use RESTORE to reread data
- Add error checking before READ

**Prevention:**
```basic
10 DATA 1,2,3,4,5 : REM Now 5 values available
20 FOR I = 1 TO 5
30 READ A
40 NEXT I
```

---

### OUT OF MEMORY

**Message:** `?OUT OF MEMORY ERROR IN <line>`

**Cause:**
- No more RAM available for program or variables
- Too many FOR loops nested
- Too many GOSUBs in effect
- Arrays too large

**Common scenarios:**
- Program too large
- Large string arrays
- Deeply nested subroutines
- Memory fragmentation

**Example:**
```basic
10 DIM A$(1000) : REM May cause out of memory
20 FOR I = 1 TO 1000
30 A$(I) = STRING$(255,42) : REM Maximum string length
40 NEXT I
```

**Solution:**
- Reduce program size
- Use smaller arrays
- Clear unused variables
- Avoid string fragmentation
- Use NEW to start fresh
- Protect memory with POKE 52,48:POKE 56,48:CLR for graphics

**Memory limits:**
- BASIC program + variables: ~38K (default configuration)
- String variable: max 255 characters
- Array subscript: max 32767

---

### OVERFLOW

**Message:** `?OVERFLOW ERROR IN <line>`

**Cause:** The result of a computation is larger than the largest number allowed, which is 1.70141183E+38.

**Common scenarios:**
- Multiplying very large numbers
- Exponential growth calculations
- Powers of large numbers

**Example:**
```basic
10 A = 1E38
20 B = A * 100 : REM Error: overflow
```

**Solution:**
- Scale calculations to smaller ranges
- Use logarithms for very large numbers
- Check intermediate results
- Consider different algorithm

---

### REDIM'D ARRAY

**Message:** `?REDIM'D ARRAY ERROR IN <line>`

**Cause:**
- An array may only be DIMensioned once
- Array variable used before DIM creates automatic DIM of 10 elements
- Subsequent DIM attempts cause error

**Common scenarios:**
- Attempting to resize array
- Accidentally using array before DIM
- DIM statement appears after array usage

**Example:**
```basic
10 DIM A(10)
20 DIM A(20) : REM Error: already dimensioned
```

**Implicit DIM error:**
```basic
10 A(5) = 10 : REM Automatic DIM A(10)
20 DIM A(100) : REM Error: already dimensioned
```

**Solution:**
- DIM arrays only once
- Place all DIMs at program start
- Never reference array before DIM
- Use NEW to clear and restart if needed

**Correct approach:**
```basic
10 DIM A(100) : REM DIM first
20 A(5) = 10 : REM Use after
```

---

### REDO FROM START

**Message:** `?REDO FROM START`

**Cause:** Character data was typed in during an INPUT statement when numeric data was expected.

**Common scenarios:**
- Typing text when number expected
- Typing invalid numeric format
- Entering expressions instead of values

**Example:**
```basic
10 INPUT "ENTER A NUMBER";A
ENTER A NUMBER? HELLO
?REDO FROM START
```

**Solution:**
- Type valid numeric data
- Program continues automatically after correction
- Not a fatal error - user can retry
- Use string input then VAL() for more control

**Robust input:**
```basic
10 INPUT "ENTER A NUMBER";A$
20 A = VAL(A$)
30 IF A = 0 AND A$ <> "0" THEN PRINT "INVALID":GOTO 10
```

---

### RETURN WITHOUT GOSUB

**Message:** `?RETURN WITHOUT GOSUB ERROR IN <line>`

**Cause:** A RETURN statement was encountered, and no GOSUB command has been issued.

**Common scenarios:**
- RETURN without matching GOSUB
- Logic flow error
- Conditional RETURN executed incorrectly

**Example:**
```basic
10 PRINT "HELLO"
20 RETURN : REM Error: no GOSUB
```

**Solution:**
- Ensure every RETURN has a GOSUB
- Check program logic flow
- Use structured programming to avoid spaghetti code

**Correct subroutine:**
```basic
10 GOSUB 100
20 END
100 PRINT "SUBROUTINE"
110 RETURN
```

---

### STRING TOO LONG

**Message:** `?STRING TOO LONG ERROR IN <line>`

**Cause:** A string can contain up to 255 characters.

**Common scenarios:**
- String concatenation exceeds 255 characters
- Reading very long file data
- Building strings in loops without checking length

**Example:**
```basic
10 A$ = STRING$(256,42) : REM Error: max is 255
```

**Solution:**
- Limit strings to 255 characters
- Split long text into multiple strings
- Check string length before concatenation

**Prevention:**
```basic
10 A$ = ""
20 FOR I = 1 TO 100
30 IF LEN(A$) + LEN(B$) <= 255 THEN A$ = A$ + B$
40 NEXT I
```

---

### ?SYNTAX ERROR

**Message:** `?SYNTAX ERROR IN <line>`

**Cause:** A statement is unrecognizable by the Commodore 64. Missing or extra parenthesis, misspelled keywords, etc.

**Common scenarios:**
- Misspelled keywords
- Missing parentheses
- Extra parentheses
- Invalid variable names
- Incorrect statement structure

**Example:**
```basic
10 PRIT "HELLO" : REM Error: should be PRINT
20 IF A = 5 THEN GOTO 100 : REM Missing line 100
30 FOR I = 1 TO 10 STEP : REM Missing STEP value
```

**Solution:**
- Check spelling of keywords
- Count parentheses (must match)
- Verify statement syntax
- Use LIST to see how BASIC interpreted line

---

### TYPE MISMATCH

**Message:** `?TYPE MISMATCH ERROR IN <line>`

**Cause:** This error occurs when a number is used in place of a string, or vice-versa.

**Common scenarios:**
- Assigning string to numeric variable
- Assigning number to string variable
- Function parameter type mismatch
- Array type mismatch

**Example:**
```basic
10 A = "HELLO" : REM Error: A is numeric, "HELLO" is string
20 B$ = 123 : REM Error: B$ is string, 123 is numeric
```

**Solution:**
- Match variable types to data
- Use $ suffix for string variables
- Convert between types: STR$() for number→string, VAL() for string→number

**Correct conversions:**
```basic
10 A = 123
20 B$ = STR$(A) : REM Convert number to string
30 C$ = "456"
40 D = VAL(C$) : REM Convert string to number
```

---

### UNDEF'D FUNCTION

**Message:** `?UNDEF'D FUNCTION ERROR IN <line>`

**Cause:** A user defined function was referenced, but it has never been defined using the DEF FN statement.

**Common scenarios:**
- Using FN before DEF FN
- Misspelling function name
- Function not defined in program

**Example:**
```basic
10 A = FNDOUBLE(5) : REM Error: FNDOUBLE not defined
```

**Solution:**
- Define function with DEF FN before use
- Check function name spelling
- Place all DEF FN statements early in program

**Correct usage:**
```basic
10 DEF FNDOUBLE(X) = X * 2
20 A = FNDOUBLE(5) : REM OK now
```

---

### UNDEF'D STATEMENT

**Message:** `?UNDEF'D STATEMENT ERROR IN <line>`

**Cause:** An attempt was made to GOTO or GOSUB or RUN a line number that doesn't exist.

**Common scenarios:**
- GOTO nonexistent line number
- GOSUB to missing subroutine
- RUN with invalid line number
- Deleted line that's still referenced

**Example:**
```basic
10 GOTO 500 : REM Error: line 500 doesn't exist
```

**Solution:**
- Verify target line exists
- Use LIST to check line numbers
- Update GOTOs when renumbering
- Add missing line or change GOTO

---

### VERIFY

**Message:** `?VERIFY ERROR`

**Cause:** The program on tape or disk does not match the program currently in memory.

**Common scenarios:**
- Program modified after SAVE
- Disk/tape read error during verify
- Wrong file being verified
- Media corruption

**Example:**
```basic
VERIFY "PROGRAM"
?VERIFY ERROR
```

**Solution:**
- Indicates SAVE or media problem
- Resave program to different location
- Check media for errors
- Use different disk/tape

---

## Error Prevention Strategies

### General Best Practices

1. **Initialize variables before use:**
```basic
10 A = 0 : B$ = "" : DIM C(10)
```

2. **Check array bounds:**
```basic
10 DIM A(10)
20 I = 5
30 IF I >= 0 AND I <= 10 THEN A(I) = 0
```

3. **Validate user input:**
```basic
10 INPUT "ENTER 1-10";A
20 IF A < 1 OR A > 10 THEN PRINT "ERROR":GOTO 10
```

4. **Check for division by zero:**
```basic
10 IF B <> 0 THEN C = A/B ELSE C = 0
```

5. **Manage file operations carefully:**
```basic
10 OPEN 1,8,2,"DATA,S,R"
20 INPUT#1,A$
30 CLOSE 1
```

### Debugging Techniques

**Use STOP for breakpoints:**
```basic
10 FOR I = 1 TO 100
20 IF I = 50 THEN STOP : REM Pause to check variables
30 NEXT I
```

**Print variable values:**
```basic
10 A = 5 : B = 10
20 PRINT "A=";A;"B=";B : REM Debug output
30 C = A + B
```

**Isolate problems:**
- Comment out sections with REM
- Test small portions of code
- Add PRINT statements to trace execution

---

## Error Recovery

### Using CONT

**When CONT works:**
- After BREAK (if no lines edited)
- After STOP statement
- After some errors (if no editing)

**When CONT doesn't work:**
- After editing program lines
- After certain errors (SYNTAX, etc.)
- After NEW or RUN

### Error Trapping in Program

BASIC V2 has no ON ERROR GOTO, but you can:

**Check for invalid input:**
```basic
10 INPUT "NUMBER";A$
20 A = VAL(A$)
30 IF A = 0 AND A$ <> "0" THEN PRINT "INVALID":GOTO 10
```

**Validate before operations:**
```basic
10 IF LEN(A$) + LEN(B$) > 255 THEN PRINT "TOO LONG":GOTO 100
20 C$ = A$ + B$
```

**Check file operations:**
```basic
10 OPEN 1,8,15 : REM Command channel
20 OPEN 2,8,2,"DATA,S,R"
30 INPUT#1,EN,EM$,ET,ES : REM Read error channel
40 IF EN <> 0 THEN PRINT "DISK ERROR:";EM$
50 CLOSE 1 : CLOSE 2
```

---

## Quick Reference Table

| Error | Primary Cause | Quick Fix |
|-------|---------------|-----------|
| BAD DATA | File data type mismatch | Check file format |
| BAD SUBSCRIPT | Array index out of range | Check array bounds |
| BREAK | User stopped program | Use CONT or RUN |
| CAN'T CONTINUE | Edited after BREAK | Use RUN instead |
| DEVICE NOT PRESENT | Device off/disconnected | Check device |
| DIVISION BY ZERO | Dividing by 0 | Check denominator |
| EXTRA IGNORED | Too much INPUT data | Enter fewer values |
| FILE NOT FOUND | Missing file | Check filename |
| FILE NOT OPEN | Forgot OPEN | Add OPEN statement |
| FILE OPEN | File number in use | CLOSE first |
| FORMULA TOO COMPLEX | Expression too long | Break into parts |
| ILLEGAL DIRECT | INPUT in direct mode | Use in program |
| ILLEGAL QUANTITY | Parameter out of range | Check valid ranges |
| LOAD | Tape read error | Check tape |
| NEXT WITHOUT FOR | Loop mismatch | Fix loop structure |
| NOT INPUT FILE | Wrong file mode | Use read mode |
| NOT OUTPUT FILE | Wrong file mode | Use write mode |
| OUT OF DATA | Not enough DATA | Add DATA statements |
| OUT OF MEMORY | RAM full | Reduce size |
| OVERFLOW | Number too large | Scale down |
| REDIM'D ARRAY | DIM twice | DIM once only |
| REDO FROM START | Invalid INPUT | Retype correctly |
| RETURN WITHOUT GOSUB | Unmatched RETURN | Add GOSUB |
| STRING TOO LONG | String > 255 chars | Reduce length |
| ?SYNTAX ERROR | Invalid statement | Check spelling |
| TYPE MISMATCH | Wrong variable type | Match types |
| UNDEF'D FUNCTION | FN not defined | Add DEF FN |
| UNDEF'D STATEMENT | Line doesn't exist | Add line or fix GOTO |
| VERIFY | File mismatch | Resave program |

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix K
- **Related:** See BASIC-V2-VOCABULARY-REFERENCE.md for statement syntax

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

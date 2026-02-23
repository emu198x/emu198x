# BASIC V2 Abbreviations Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Appendix A

---

## Overview

Commodore 64 BASIC allows abbreviating most keywords as a time-saver when typing programs and commands. **Important:** Abbreviations don't save memory—keywords tokenize to single bytes regardless of how they're entered. When a program is LISTed, abbreviated keywords display in their full form.

**Abbreviation pattern:**
- Type the first one or two letters of the word
- Then type the NEXT letter while holding SHIFT
- Exception: PRINT abbreviates to `?`

**Keywords with NO abbreviation:**
COS, FN, GET#, IF, INPUT, INT, LEN, LOG, NEW, ON, OR, POS, REM, TAN

---

## Complete Abbreviations Table

| Keyword | Abbreviation | Entry Method | Screen Display |
|---------|--------------|--------------|----------------|
| ABS | A█ | A SHIFT+B | A with inverse B |
| AND | A▒ | A SHIFT+N | A with inverse N |
| ASC | A♣ | A SHIFT+S | A with inverse S |
| ATN | A█ | A SHIFT+T | A with inverse T |
| CHR$ | C█ | C SHIFT+H | C with inverse H |
| CLOSE | CL▐ | C L SHIFT+O | CL with inverse O |
| CLR | C█ | C SHIFT+L | C with inverse L |
| CMD | C♣ | C SHIFT+M | C with inverse M |
| CONT | C▐ | C SHIFT+O | C with inverse O |
| COS | — | (none) | COS |
| DATA | D♣ | D SHIFT+A | D with inverse A |
| DEF | D▬ | D SHIFT+E | D with inverse E |
| DIM | D♣ | D SHIFT+I | D with inverse I |
| END | E▒ | E SHIFT+N | E with inverse N |
| EXP | E✕ | E SHIFT+X | E with inverse X |
| FN | — | (none) | FN |
| FOR | F▐ | F SHIFT+O | F with inverse O |
| FRE | F▬ | F SHIFT+R | F with inverse R |
| GET | G▐ | G SHIFT+E | G with inverse E |
| GET# | — | (none) | GET# |
| GOSUB | GO♣ | G O SHIFT+S | GO with inverse S |
| GOTO | G▐ | G SHIFT+O | G with inverse O |
| IF | — | (none) | IF |
| INPUT | — | (none) | INPUT |
| INPUT# | I▒ | I SHIFT+N | I with inverse N |
| INT | — | (none) | INT |
| LEFT$ | LE▬ | L E SHIFT+F | LE with inverse F |
| LEN | — | (none) | LEN |
| LET | L▬ | L SHIFT+E | L with inverse E |
| LIST | L♣ | L SHIFT+I | L with inverse I |
| LOAD | L▐ | L SHIFT+O | L with inverse O |
| LOG | — | (none) | LOG |
| MID$ | M♣ | M SHIFT+I | M with inverse I |
| NEW | — | (none) | NEW |
| NEXT | N▬ | N SHIFT+E | N with inverse E |
| NOT | N▐ | N SHIFT+O | N with inverse O |
| ON | — | (none) | ON |
| OPEN | O▐ | O SHIFT+P | O with inverse P |
| OR | — | (none) | OR |
| PEEK | P▐ | P SHIFT+E | P with inverse E |
| POKE | P▐ | P SHIFT+O | P with inverse O |
| POS | — | (none) | POS |
| PRINT | ? | ? | ? |
| PRINT# | P▬ | P SHIFT+R | P with inverse R |
| READ | R▬ | R SHIFT+E | R with inverse E |
| REM | — | (none) | REM |
| RESTORE | RE♣ | R E SHIFT+S | RE with inverse S |
| RETURN | RE█ | R E SHIFT+T | RE with inverse T |
| RIGHT$ | R♣ | R SHIFT+I | R with inverse I |
| RND | R▒ | R SHIFT+N | R with inverse N |
| RUN | R▒ | R SHIFT+U | R with inverse U |
| SAVE | S♣ | S SHIFT+A | S with inverse A |
| SGN | S█ | S SHIFT+G | S with inverse G |
| SIN | S♣ | S SHIFT+I | S with inverse I |
| SPC | S▐ | S SHIFT+P | S with inverse P |
| SQR | S● | S SHIFT+Q | S with inverse Q |
| STATUS | ST | S T | ST |
| STEP | ST▬ | S T SHIFT+E | ST with inverse E |
| STOP | S█ | S SHIFT+T | S with inverse T |
| STR$ | ST▬ | S T SHIFT+R | ST with inverse R |
| SYS | S█ | S SHIFT+Y | S with inverse Y |
| TAB | T♣ | T SHIFT+A | T with inverse A |
| TAN | — | (none) | TAN |
| THEN | T█ | T SHIFT+H | T with inverse H |
| TIME | TI | T I | TI |
| TIME$ | TI$ | T I $ | TI$ |
| USR | U♣ | U SHIFT+S | U with inverse S |
| VAL | V♣ | V SHIFT+A | V with inverse A |
| VERIFY | V▬ | V SHIFT+E | V with inverse E |
| WAIT | W♣ | W SHIFT+A | W with inverse A |

---

## Grouped by First Letter

### A Commands
- **ABS** → A█ (A SHIFT+B)
- **AND** → A▒ (A SHIFT+N)
- **ASC** → A♣ (A SHIFT+S)
- **ATN** → A█ (A SHIFT+T)

### C Commands
- **CHR$** → C█ (C SHIFT+H)
- **CLOSE** → CL▐ (C L SHIFT+O)
- **CLR** → C█ (C SHIFT+L)
- **CMD** → C♣ (C SHIFT+M)
- **CONT** → C▐ (C SHIFT+O)
- **COS** → No abbreviation

### D Commands
- **DATA** → D♣ (D SHIFT+A)
- **DEF** → D▬ (D SHIFT+E)
- **DIM** → D♣ (D SHIFT+I)

### E Commands
- **END** → E▒ (E SHIFT+N)
- **EXP** → E✕ (E SHIFT+X)

### F Commands
- **FN** → No abbreviation
- **FOR** → F▐ (F SHIFT+O)
- **FRE** → F▬ (F SHIFT+R)

### G Commands
- **GET** → G▐ (G SHIFT+E)
- **GET#** → No abbreviation
- **GOSUB** → GO♣ (G O SHIFT+S)
- **GOTO** → G▐ (G SHIFT+O)

### I Commands
- **IF** → No abbreviation
- **INPUT** → No abbreviation
- **INPUT#** → I▒ (I SHIFT+N)
- **INT** → No abbreviation

### L Commands
- **LEFT$** → LE▬ (L E SHIFT+F)
- **LEN** → No abbreviation
- **LET** → L▬ (L SHIFT+E)
- **LIST** → L♣ (L SHIFT+I)
- **LOAD** → L▐ (L SHIFT+O)
- **LOG** → No abbreviation

### M-N Commands
- **MID$** → M♣ (M SHIFT+I)
- **NEW** → No abbreviation
- **NEXT** → N▬ (N SHIFT+E)
- **NOT** → N▐ (N SHIFT+O)

### O Commands
- **ON** → No abbreviation
- **OPEN** → O▐ (O SHIFT+P)
- **OR** → No abbreviation

### P Commands
- **PEEK** → P▐ (P SHIFT+E)
- **POKE** → P▐ (P SHIFT+O)
- **POS** → No abbreviation
- **PRINT** → ? (question mark)
- **PRINT#** → P▬ (P SHIFT+R)

### R Commands
- **READ** → R▬ (R SHIFT+E)
- **REM** → No abbreviation
- **RESTORE** → RE♣ (R E SHIFT+S)
- **RETURN** → RE█ (R E SHIFT+T)
- **RIGHT$** → R♣ (R SHIFT+I)
- **RND** → R▒ (R SHIFT+N)
- **RUN** → R▒ (R SHIFT+U)

### S Commands
- **SAVE** → S♣ (S SHIFT+A)
- **SGN** → S█ (S SHIFT+G)
- **SIN** → S♣ (S SHIFT+I)
- **SPC** → S▐ (S SHIFT+P)
- **SQR** → S● (S SHIFT+Q)
- **STATUS** → ST (S T)
- **STEP** → ST▬ (S T SHIFT+E)
- **STOP** → S█ (S SHIFT+T)
- **STR$** → ST▬ (S T SHIFT+R)
- **SYS** → S█ (S SHIFT+Y)

### T Commands
- **TAB** → T♣ (T SHIFT+A)
- **TAN** → No abbreviation
- **THEN** → T█ (T SHIFT+H)
- **TIME** → TI (T I)
- **TIME$** → TI$ (T I $)

### U-V-W Commands
- **USR** → U♣ (U SHIFT+S)
- **VAL** → V♣ (V SHIFT+A)
- **VERIFY** → V▬ (V SHIFT+E)
- **WAIT** → W♣ (W SHIFT+A)

---

## Special Cases

### PRINT is Special
The most commonly used keyword has its own unique abbreviation:
- **PRINT** → `?` (question mark)
- **PRINT#** → P▬ (P SHIFT+R)

### Two-Letter Base Abbreviations
Some keywords require typing two letters before the shifted letter:
- **CLOSE** → C L SHIFT+O
- **GOSUB** → G O SHIFT+S
- **LEFT$** → L E SHIFT+F
- **RESTORE** → R E SHIFT+S
- **RETURN** → R E SHIFT+T

### Keywords Beginning with "ST"
Four keywords start with ST and have no further abbreviation needed:
- **STATUS** → ST (just type S T)
- **STEP** → ST▬ (S T SHIFT+E)
- **STOP** → S█ (S SHIFT+T - uses only S!)
- **STR$** → ST▬ (S T SHIFT+R)

### Time Functions
- **TIME** → TI (just type T I)
- **TIME$** → TI$ (type T I $)

---

## Common Abbreviation Patterns

### Single Letter + SHIFT Pattern
Most abbreviations follow this pattern:

| First Letter | Examples |
|--------------|----------|
| A | A█(BS), A▒(ND), A♣(SC), A█(TN) |
| C | C█(HR$), C█(LR), C♣(MD), C▐(ONT) |
| D | D♣(ATA), D▬(EF), D♣(IM) |
| E | E▒(ND), E✕(XP) |
| F | F▐(OR), F▬(RE) |
| G | G▐(ET), G▐(OTO) |
| L | L▬(ET), L♣(IST), L▐(OAD) |
| M | M♣(ID$) |
| N | N▬(EXT), N▐(OT) |
| O | O▐(PEN) |
| P | P▐(EEK), P▐(OKE), P▬(RINT#) |
| R | R▬(EAD), R♣(IGHT$), R▒(ND), R▒(UN) |
| S | S♣(AVE), S█(GN), S♣(IN), S▐(PC), S● (SQR), S█(YS) |
| T | T♣(AB), T█(HEN) |
| U | U♣(SR) |
| V | V♣(AL), V▬(ERIFY) |
| W | W♣(AIT) |

---

## Usage Guidelines

### For Lesson Content

**In .bas files (petcat compatibility):**
- Use full lowercase keywords: `poke`, `print`, `for`
- Don't use abbreviations in source files

**In MDX lesson text:**
- Use full uppercase keywords: `POKE`, `PRINT`, `FOR`
- Show abbreviations when teaching typing efficiency
- Explain that abbreviations auto-expand when LISTed

### Teaching Abbreviations

When introducing abbreviations in lessons:

1. **Emphasize they're typing shortcuts only**
   - Don't save memory
   - Auto-expand when LISTed
   - Optional convenience feature

2. **Show the most useful ones first**
   - `?` for PRINT (most common)
   - `P▐` for POKE/PEEK
   - `L▐` for LOAD
   - `L♣` for LIST

3. **Demonstrate the SHIFT pattern**
   - Type first letter(s)
   - SHIFT on next letter
   - See inverse character on screen
   - Expands when you press RETURN

### Common Mistakes

**Same first letters cause confusion:**
- CLOSE vs CLR vs CMD vs CONT (all start with C)
- PEEK vs POKE vs POS (all start with P)
- Solution: Different SHIFT patterns distinguish them

**Keywords with no abbreviation:**
- Students may try to abbreviate IF, INT, LEN, etc.
- These must be typed in full
- No visual feedback when typing (no inverse characters)

---

## Quick Reference: Most Used Abbreviations

For experienced programmers, these are the most frequently used:

| Command | Abbrev | Why Useful |
|---------|--------|------------|
| PRINT | ? | Saves 5 characters, very common |
| POKE | P▐ | Hardware programming essential |
| PEEK | P▐ | Hardware programming essential |
| GOTO | G▐ | Flow control |
| GOSUB | GO♣ | Flow control |
| RETURN | RE█ | Flow control |
| FOR | F▐ | Loops |
| NEXT | N▬ | Loops |
| LIST | L♣ | Program viewing |
| LOAD | L▐ | Disk operations |
| SAVE | S♣ | Disk operations |
| RUN | R▒ | Program execution |

---

## Historical Context

The abbreviation system reflects the C64's design philosophy:
- Designed for typing on TV screens (limited visibility)
- Abbreviations speed up program entry
- Inverse video provides visual feedback
- Full expansion on LIST aids readability
- Tokenization makes memory usage identical

This dual system (short entry, long display) balanced programmer convenience with code clarity.

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix A
- **Related:** See BASIC-V2-VOCABULARY-REFERENCE.md for complete keyword documentation

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

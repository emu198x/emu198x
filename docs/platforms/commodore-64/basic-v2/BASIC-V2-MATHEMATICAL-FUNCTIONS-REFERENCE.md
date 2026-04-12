# BASIC V2 Mathematical Functions Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Appendix H

---

## Overview

Commodore 64 BASIC includes only four trigonometric functions: **SIN**, **COS**, **TAN**, and **ATN**. However, many other mathematical functions can be derived using combinations of built-in functions and arithmetic operations.

**Built-in math functions:**
- **SIN(X)** - Sine of X (X in radians)
- **COS(X)** - Cosine of X (X in radians)
- **TAN(X)** - Tangent of X (X in radians)
- **ATN(X)** - Arctangent of X (result in radians)
- **EXP(X)** - e raised to the power X
- **LOG(X)** - Natural logarithm of X
- **SQR(X)** - Square root of X
- **SGN(X)** - Sign of X (-1, 0, or 1)
- **ABS(X)** - Absolute value of X

**Constants:**
- **π** (pi) ≈ 3.14159265 - Use `3.14159265` or calculate as `4*ATN(1)`

---

## Reciprocal Trigonometric Functions

### Secant

**Function:** SEC(X) - Secant of X

**Formula:**
```basic
SEC(X) = 1/COS(X)
```

**Example:**
```basic
10 X = 1.0
20 SECANT = 1/COS(X)
30 PRINT "SEC(";X;") =";SECANT
```

### Cosecant

**Function:** CSC(X) - Cosecant of X

**Formula:**
```basic
CSC(X) = 1/SIN(X)
```

**Example:**
```basic
10 X = 1.0
20 COSECANT = 1/SIN(X)
30 PRINT "CSC(";X;") =";COSECANT
```

### Cotangent

**Function:** COT(X) - Cotangent of X

**Formula:**
```basic
COT(X) = 1/TAN(X)
```

**Example:**
```basic
10 X = 1.0
20 COTANGENT = 1/TAN(X)
30 PRINT "COT(";X;") =";COTANGENT
```

---

## Inverse Trigonometric Functions

### Inverse Sine (Arcsine)

**Function:** ARCSIN(X) - Arcsine of X

**Formula:**
```basic
ARCSIN(X) = ATN(X/SQR(-X*X+1))
```

**Domain:** -1 ≤ X ≤ 1
**Range:** -π/2 to π/2 radians

**Example:**
```basic
10 X = 0.5
20 ARCSINE = ATN(X/SQR(-X*X+1))
30 PRINT "ARCSIN(";X;") =";ARCSINE
```

### Inverse Cosine (Arccosine)

**Function:** ARCCOS(X) - Arccosine of X

**Formula:**
```basic
ARCCOS(X) = -ATN(X/SQR(-X*X+1)) + π/2
```

**Domain:** -1 ≤ X ≤ 1
**Range:** 0 to π radians

**Example:**
```basic
10 X = 0.5
20 PI = 3.14159265
30 ARCCOSINE = -ATN(X/SQR(-X*X+1)) + PI/2
40 PRINT "ARCCOS(";X;") =";ARCCOSINE
```

### Inverse Secant (Arcsecant)

**Function:** ARCSEC(X) - Arcsecant of X

**Formula:**
```basic
ARCSEC(X) = ATN(SQR(X*X-1)) + (SGN(X)-1)*π/2
```

**Domain:** X ≤ -1 or X ≥ 1

**Example:**
```basic
10 X = 2.0
20 PI = 3.14159265
30 ARCSECANT = ATN(SQR(X*X-1)) + (SGN(X)-1)*PI/2
40 PRINT "ARCSEC(";X;") =";ARCSECANT
```

### Inverse Cosecant (Arccosecant)

**Function:** ARCCSC(X) - Arccosecant of X

**Formula:**
```basic
ARCCSC(X) = ATN(1/SQR(X*X-1)) + (SGN(X)-1)*π/2
```

**Domain:** X ≤ -1 or X ≥ 1

**Example:**
```basic
10 X = 2.0
20 PI = 3.14159265
30 ARCCOSECANT = ATN(1/SQR(X*X-1)) + (SGN(X)-1)*PI/2
40 PRINT "ARCCSC(";X;") =";ARCCOSECANT
```

### Inverse Cotangent (Arccotangent)

**Function:** ARCOT(X) - Arccotangent of X

**Formula:**
```basic
ARCOT(X) = ATN(-X) + π/2
```

**Example:**
```basic
10 X = 1.0
20 PI = 3.14159265
30 ARCCOTANGENT = ATN(-X) + PI/2
40 PRINT "ARCOT(";X;") =";ARCCOTANGENT
```

---

## Hyperbolic Functions

### Hyperbolic Sine

**Function:** SINH(X) - Hyperbolic sine of X

**Formula:**
```basic
SINH(X) = (EXP(X) - EXP(-X))/2
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_SINE = (EXP(X) - EXP(-X))/2
30 PRINT "SINH(";X;") =";HYPERBOLIC_SINE
```

### Hyperbolic Cosine

**Function:** COSH(X) - Hyperbolic cosine of X

**Formula:**
```basic
COSH(X) = (EXP(X) + EXP(-X))/2
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_COSINE = (EXP(X) + EXP(-X))/2
30 PRINT "COSH(";X;") =";HYPERBOLIC_COSINE
```

### Hyperbolic Tangent

**Function:** TANH(X) - Hyperbolic tangent of X

**Formula:**
```basic
TANH(X) = (EXP(X) - EXP(-X))/(EXP(X) + EXP(-X))
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_TANGENT = (EXP(X) - EXP(-X))/(EXP(X) + EXP(-X))
30 PRINT "TANH(";X;") =";HYPERBOLIC_TANGENT
```

### Hyperbolic Secant

**Function:** SECH(X) - Hyperbolic secant of X

**Formula:**
```basic
SECH(X) = 2/(EXP(X) + EXP(-X))
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_SECANT = 2/(EXP(X) + EXP(-X))
30 PRINT "SECH(";X;") =";HYPERBOLIC_SECANT
```

### Hyperbolic Cosecant

**Function:** CSCH(X) - Hyperbolic cosecant of X

**Formula:**
```basic
CSCH(X) = 2/(EXP(X) - EXP(-X))
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_COSECANT = 2/(EXP(X) - EXP(-X))
30 PRINT "CSCH(";X;") =";HYPERBOLIC_COSECANT
```

### Hyperbolic Cotangent

**Function:** COTH(X) - Hyperbolic cotangent of X

**Formula:**
```basic
COTH(X) = EXP(-X)/(EXP(X) - EXP(-X))*2 + 1
```

**Example:**
```basic
10 X = 1.0
20 HYPERBOLIC_COTANGENT = EXP(-X)/(EXP(X) - EXP(-X))*2 + 1
30 PRINT "COTH(";X;") =";HYPERBOLIC_COTANGENT
```

---

## Inverse Hyperbolic Functions

### Inverse Hyperbolic Sine

**Function:** ARCSINH(X) - Inverse hyperbolic sine of X

**Formula:**
```basic
ARCSINH(X) = LOG(X + SQR(X*X + 1))
```

**Example:**
```basic
10 X = 1.0
20 ARC_HYPERBOLIC_SINE = LOG(X + SQR(X*X + 1))
30 PRINT "ARCSINH(";X;") =";ARC_HYPERBOLIC_SINE
```

### Inverse Hyperbolic Cosine

**Function:** ARCCOSH(X) - Inverse hyperbolic cosine of X

**Formula:**
```basic
ARCCOSH(X) = LOG(X + SQR(X*X - 1))
```

**Domain:** X ≥ 1

**Example:**
```basic
10 X = 2.0
20 ARC_HYPERBOLIC_COSINE = LOG(X + SQR(X*X - 1))
30 PRINT "ARCCOSH(";X;") =";ARC_HYPERBOLIC_COSINE
```

### Inverse Hyperbolic Tangent

**Function:** ARCTANH(X) - Inverse hyperbolic tangent of X

**Formula:**
```basic
ARCTANH(X) = LOG((1 + X)/(1 - X))/2
```

**Domain:** -1 < X < 1

**Example:**
```basic
10 X = 0.5
20 ARC_HYPERBOLIC_TANGENT = LOG((1 + X)/(1 - X))/2
30 PRINT "ARCTANH(";X;") =";ARC_HYPERBOLIC_TANGENT
```

### Inverse Hyperbolic Secant

**Function:** ARCSECH(X) - Inverse hyperbolic secant of X

**Formula:**
```basic
ARCSECH(X) = LOG((1 + SQR(1 - X*X))/X)
```

**Domain:** 0 < X ≤ 1

**Example:**
```basic
10 X = 0.5
20 ARC_HYPERBOLIC_SECANT = LOG((1 + SQR(1 - X*X))/X)
30 PRINT "ARCSECH(";X;") =";ARC_HYPERBOLIC_SECANT
```

### Inverse Hyperbolic Cosecant

**Function:** ARCCSCH(X) - Inverse hyperbolic cosecant of X

**Formula:**
```basic
ARCCSCH(X) = LOG((SGN(X) + SQR(X*X + 1))/X)
```

**Domain:** X ≠ 0

**Example:**
```basic
10 X = 2.0
20 ARC_HYPERBOLIC_COSECANT = LOG((SGN(X) + SQR(X*X + 1))/X)
30 PRINT "ARCCSCH(";X;") =";ARC_HYPERBOLIC_COSECANT
```

### Inverse Hyperbolic Cotangent

**Function:** ARCCOTH(X) - Inverse hyperbolic cotangent of X

**Formula:**
```basic
ARCCOTH(X) = LOG((SQR(X*X - 1))/(X - 1))
```

**Domain:** X < -1 or X > 1

**Example:**
```basic
10 X = 2.0
20 ARC_HYPERBOLIC_COTANGENT = LOG((SQR(X*X - 1))/(X - 1))
30 PRINT "ARCCOTH(";X;") =";ARC_HYPERBOLIC_COTANGENT
```

---

## Quick Reference Table

### Standard Trigonometric

| Function | Formula | Domain |
|----------|---------|--------|
| SEC(X) | 1/COS(X) | All X where COS(X) ≠ 0 |
| CSC(X) | 1/SIN(X) | All X where SIN(X) ≠ 0 |
| COT(X) | 1/TAN(X) | All X where TAN(X) ≠ 0 |

### Inverse Trigonometric

| Function | Formula | Domain |
|----------|---------|--------|
| ARCSIN(X) | ATN(X/SQR(-X*X+1)) | -1 ≤ X ≤ 1 |
| ARCCOS(X) | -ATN(X/SQR(-X*X+1))+π/2 | -1 ≤ X ≤ 1 |
| ARCSEC(X) | ATN(SQR(X*X-1))+(SGN(X)-1)*π/2 | X ≤ -1 or X ≥ 1 |
| ARCCSC(X) | ATN(1/SQR(X*X-1))+(SGN(X)-1)*π/2 | X ≤ -1 or X ≥ 1 |
| ARCOT(X) | ATN(-X)+π/2 | All X |

### Hyperbolic

| Function | Formula |
|----------|---------|
| SINH(X) | (EXP(X)-EXP(-X))/2 |
| COSH(X) | (EXP(X)+EXP(-X))/2 |
| TANH(X) | (EXP(X)-EXP(-X))/(EXP(X)+EXP(-X)) |
| SECH(X) | 2/(EXP(X)+EXP(-X)) |
| CSCH(X) | 2/(EXP(X)-EXP(-X)) |
| COTH(X) | EXP(-X)/(EXP(X)-EXP(-X))*2+1 |

### Inverse Hyperbolic

| Function | Formula | Domain |
|----------|---------|--------|
| ARCSINH(X) | LOG(X+SQR(X*X+1)) | All X |
| ARCCOSH(X) | LOG(X+SQR(X*X-1)) | X ≥ 1 |
| ARCTANH(X) | LOG((1+X)/(1-X))/2 | -1 < X < 1 |
| ARCSECH(X) | LOG((1+SQR(1-X*X))/X) | 0 < X ≤ 1 |
| ARCCSCH(X) | LOG((SGN(X)+SQR(X*X+1))/X) | X ≠ 0 |
| ARCCOTH(X) | LOG((SQR(X*X-1))/(X-1)) | X < -1 or X > 1 |

---

## Practical Implementation Tips

### Creating DEF FN Functions

For frequently used derived functions, define them once with DEF FN:

```basic
10 REM Define derived functions
20 DEF FNSEC(X) = 1/COS(X)
30 DEF FNCSC(X) = 1/SIN(X)
40 DEF FNCOT(X) = 1/TAN(X)
50 REM Use them in program
100 A = FNSEC(1.5)
110 B = FNCSC(2.0)
120 C = FNCOT(0.785)
```

### Defining Pi

Calculate pi once at the start of your program:

```basic
10 PI = 3.14159265
```

Or calculate it dynamically:

```basic
10 PI = 4*ATN(1)
```

### Optimization for Multiple Calls

If calling the same function many times, store intermediate calculations:

```basic
REM Instead of:
10 A = (EXP(X) - EXP(-X))/(EXP(X) + EXP(-X))
20 B = 2/(EXP(X) + EXP(-X))

REM Do this:
10 E1 = EXP(X)
20 E2 = EXP(-X)
30 SUM = E1 + E2
40 DIF = E1 - E2
50 A = DIF/SUM : REM TANH
60 B = 2/SUM : REM SECH
```

### Error Handling

Check domain restrictions before calculation:

```basic
10 REM ARCSIN requires -1 <= X <= 1
20 INPUT "X";X
30 IF ABS(X) > 1 THEN PRINT "OUT OF RANGE":GOTO 20
40 RESULT = ATN(X/SQR(-X*X+1))
50 PRINT "ARCSIN(";X;") =";RESULT
```

---

## Common Applications

### Navigation and Astronomy

Inverse trigonometric functions are essential for:
- Converting rectangular to polar coordinates
- Calculating angles from position data
- Celestial navigation calculations

### Engineering and Physics

Hyperbolic functions appear in:
- Catenary curves (hanging cables)
- Velocity calculations in special relativity
- Signal processing and control systems
- Thermodynamic calculations

### Game Programming

Derived trig functions useful for:
- 3D perspective calculations
- Camera angle conversions
- Physics simulations
- Trajectory calculations

---

## Performance Considerations

### Computational Cost

**Fast functions** (few operations):
- SEC, CSC, COT (one division)

**Moderate functions** (several operations):
- SINH, COSH, SECH, CSCH (2-4 EXP calls)

**Slow functions** (many operations):
- ARCSIN, ARCCOS, ARCSEC, ARCCSC (includes SQR, division, ATN)
- ARCTANH, ARCSECH, ARCCSCH, ARCCOTH (includes LOG, SQR, division)

### Memory Usage

Each derived function in a DEF FN statement consumes:
- ~20 bytes for the function definition
- Stack space during evaluation

For programs using many math functions, consider:
- Combining similar calculations
- Pre-calculating constant values
- Using lookup tables for repeated values

---

## Example: Complete Math Library

```basic
10 REM DERIVED MATH FUNCTIONS
20 PI = 3.14159265
30 REM
40 REM RECIPROCAL TRIG
50 DEF FNSEC(X) = 1/COS(X)
60 DEF FNCSC(X) = 1/SIN(X)
70 DEF FNCOT(X) = 1/TAN(X)
80 REM
90 REM INVERSE TRIG
100 DEF FNARCSIN(X) = ATN(X/SQR(-X*X+1))
110 DEF FNARCCOS(X) = -ATN(X/SQR(-X*X+1))+PI/2
120 DEF FNARCOT(X) = ATN(-X)+PI/2
130 REM
140 REM HYPERBOLIC
150 DEF FNSINH(X) = (EXP(X)-EXP(-X))/2
160 DEF FNCOSH(X) = (EXP(X)+EXP(-X))/2
170 DEF FNTANH(X) = (EXP(X)-EXP(-X))/(EXP(X)+EXP(-X))
180 REM
190 REM INVERSE HYPERBOLIC
200 DEF FNARCSINH(X) = LOG(X+SQR(X*X+1))
210 DEF FNARCCOSH(X) = LOG(X+SQR(X*X-1))
220 DEF FNARCTANH(X) = LOG((1+X)/(1-X))/2
230 REM
1000 REM YOUR PROGRAM STARTS HERE
```

---

## Limitations and Notes

### Accuracy

C64 BASIC uses Microsoft floating-point format:
- Approximately 9 decimal digits of precision
- Some accumulated error in complex formulas
- Very large or very small numbers may lose precision

### Overflow/Underflow

Watch for:
- **EXP(X)** overflow when X > 88
- **EXP(-X)** underflow when X > 88
- Division by zero in reciprocal functions
- Square root of negative numbers

### Angle Units

All trigonometric functions use **radians**, not degrees:
- To convert degrees to radians: `RADIANS = DEGREES * PI/180`
- To convert radians to degrees: `DEGREES = RADIANS * 180/PI`

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix H
- **Related:** See BASIC-V2-VOCABULARY-REFERENCE.md for built-in function details

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications

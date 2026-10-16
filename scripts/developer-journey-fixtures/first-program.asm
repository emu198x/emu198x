; A visible instruction. Stock 48K Spectrum, loaded from tape.
; The ROM loader supplies the initial machine environment.
; This hold loop is deliberately not a clock.
 org 32768
start:
 ld a,2
 out ($FE),a
hold:
 jr hold
 end start

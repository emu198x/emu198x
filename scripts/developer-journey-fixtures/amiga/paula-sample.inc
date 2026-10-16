; =============================================================================
; PAULA SAMPLE - start or stop a looping sample on audio channel 0
;
; play_sample
;   In:  a5 = $DFF000 (custom chip base)
;        a0 = sample address: chip RAM, even (word-aligned)
;        d0 = length in WORDS, 1-65535 (two 8-bit signed samples per word)
;        d1 = period, colour clocks per sample: 123-65535 PAL (124 NTSC)
;        d2 = volume, 0-64
;   Out: channel 0 plays the sample from its start and repeats it until
;        stop_sample. No registers change.
;
; The caller owns channel 0, the DMACON audio and master bits, and channel
; 0's interrupt. Channel 0 DMA must already be off, as at power-on or after
; stop_sample followed by at least two sample periods.
; =============================================================================

DMACON          equ $096        ; DMA control (write)
AUD0LC          equ $0a0        ; Channel 0 location, AUD0LCH/AUD0LCL as one long
AUD0LEN         equ $0a4        ; Channel 0 length, in words
AUD0PER         equ $0a6        ; Channel 0 period
AUD0VOL         equ $0a8        ; Channel 0 volume

DMAF_SETCLR     equ $8000       ; Write 1s: set the other 1 bits, leave the rest
DMAF_MASTER     equ $0200       ; DMAEN: no DMA happens without it
DMAF_AUD0       equ $0001       ; AUD0EN: channel 0 DMA

play_sample:
            move.l  a0,AUD0LC(a5)       ; Copied to the back-up pointer at start
            move.w  d0,AUD0LEN(a5)      ; and the length, reloaded every pass
            move.w  d1,AUD0PER(a5)      ; Colour clocks between output samples
            move.w  d2,AUD0VOL(a5)      ; 64 = 0 dB, 32 = -6 dB, 0 = silent
            move.w  #DMAF_SETCLR|DMAF_MASTER|DMAF_AUD0,DMACON(a5)
            rts

; stop_sample - clear AUD0EN, leaving the other DMA bits alone.
;   In: a5 = $DFF000. Leave the channel off for more than two sample periods
;   before play_sample, or it may continue where it stopped.
stop_sample:
            move.w  #DMAF_AUD0,DMACON(a5)
            rts

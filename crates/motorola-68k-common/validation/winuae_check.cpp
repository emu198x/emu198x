/* Reads lines: a_high a_low b_high b_low mode op rust_high rust_low rust_flags
 * (all hex). Re-runs WinUAE's SoftFloat (the silicon-validated 68881/2 FPSP
 * reference) with the same inputs and prints MISMATCH lines where the C++
 * result *value* differs from the Rust columns. Exit nonzero on any mismatch.
 *
 * Only the result VALUE (high:low) is compared, not the exception flags:
 * WinUAE's softfloat is the SoftFloat-2a lineage with a different
 * float_exception_flags bit layout from our 2b-derived port, so flags are
 * validated against softfloat.c (run.sh) and by unit test, not here.
 *
 * Bit-exact against WinUAE: 0=add 1=sub 2=mul 3=div 4=sqrt 5=to_int32 6=to_f32
 * 7=to_f64 8=int32_to 9=float32_to 10=float64_to 11=sglmul 12=sgldiv 13=rem
 * 14=mod 15=add@single 16=add@double 17=move@single 18=move@double 19=abs@single
 * 20=getexp 21=getman 22=scale. For rem/mod the flags column carries the FPSR
 * quotient byte (sign<<7 | low 7 bits), compared alongside the value.
 * (The transcendentals are the remaining ops.) */
#include <cstdint>
#include <cstdio>
#include "softfloat/softfloat.h"

int main(void) {
    unsigned int ah, bh, mode, op;
    unsigned long long al, bl, r_lo, r_flags;
    unsigned int r_hi;
    long mismatches = 0, total = 0;

    /* WinUAE 68k float_status: tininess before rounding, extended precision,
     * and the 68881/2 special-case flags (fp_init_softfloat's `else` branch:
     * addsub_swap_inf set, infinity_clear_intbit + cmp_signed_nan clear). The
     * 68040 and 68060 differ here; #112 targets the 68881/2. */
    float_status st = {};
    st.float_detect_tininess = float_tininess_before_rounding;
    st.floatx80_rounding_precision = 80;
    st.floatx80_special_flags = addsub_swap_inf;

    /* FPCR rounding bits (0=RN 1=RZ 2=RM 3=RP) -> WinUAE float_round_*. */
    static const signed char rmap[4] = {
        float_round_nearest_even, float_round_to_zero,
        float_round_down, float_round_up,
    };

    while (scanf("%x %llx %x %llx %x %x %x %llx %llx",
                 &ah, &al, &bh, &bl, &mode, &op, &r_hi, &r_lo, &r_flags) == 9) {
        total++;
        st.float_rounding_mode = rmap[mode & 3];
        st.float_exception_flags = 0;
        floatx80 a, b, z;
        a.high = (uint16_t)ah; a.low = al;
        b.high = (uint16_t)bh; b.low = bl;
        unsigned int c_hi = 0; unsigned long long c_lo = 0;
        unsigned long long c_qbyte = 0; int has_qbyte = 0;
        switch (op) {
            case 0:  z = floatx80_add(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 1:  z = floatx80_sub(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 2:  z = floatx80_mul(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 3:  z = floatx80_div(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 4:  z = floatx80_sqrt(a, &st);      c_hi = z.high; c_lo = z.low; break;
            case 5:  c_lo = (unsigned int)floatx80_to_int32(a, &st); break;
            case 6:  c_lo = floatx80_to_float32(a, &st); break;
            case 7:  c_lo = floatx80_to_float64(a, &st); break;
            case 8:  z = int32_to_floatx80((int32_t)(unsigned int)al); c_hi = z.high; c_lo = z.low; break;
            case 9:  z = float32_to_floatx80((float32)(unsigned int)al, &st); c_hi = z.high; c_lo = z.low; break;
            case 10: z = float64_to_floatx80((float64)al, &st); c_hi = z.high; c_lo = z.low; break;
            case 11: z = floatx80_sglmul(a, b, &st); c_hi = z.high; c_lo = z.low; break;
            case 12: z = floatx80_sgldiv(a, b, &st); c_hi = z.high; c_lo = z.low; break;
            case 15: st.floatx80_rounding_precision = 32; z = floatx80_add(a, b, &st);
                     st.floatx80_rounding_precision = 80; c_hi = z.high; c_lo = z.low; break;
            case 16: st.floatx80_rounding_precision = 64; z = floatx80_add(a, b, &st);
                     st.floatx80_rounding_precision = 80; c_hi = z.high; c_lo = z.low; break;
            case 17: st.floatx80_rounding_precision = 32; z = floatx80_move(a, &st);
                     st.floatx80_rounding_precision = 80; c_hi = z.high; c_lo = z.low; break;
            case 18: st.floatx80_rounding_precision = 64; z = floatx80_move(a, &st);
                     st.floatx80_rounding_precision = 80; c_hi = z.high; c_lo = z.low; break;
            case 19: st.floatx80_rounding_precision = 32; z = floatx80_abs(a, &st);
                     st.floatx80_rounding_precision = 80; c_hi = z.high; c_lo = z.low; break;
            case 20: z = floatx80_getexp(a, &st);    c_hi = z.high; c_lo = z.low; break;
            case 21: z = floatx80_getman(a, &st);    c_hi = z.high; c_lo = z.low; break;
            case 22: z = floatx80_scale(a, b, &st);  c_hi = z.high; c_lo = z.low; break;
            case 13: case 14: {
                uint64_t q = 0; flag s = 0;
                z = (op == 13) ? floatx80_rem(a, b, &q, &s, &st)
                               : floatx80_mod(a, b, &q, &s, &st);
                c_hi = z.high; c_lo = z.low;
                c_qbyte = (q & 0x7F) | ((unsigned long long)(s ? 1 : 0) << 7);
                has_qbyte = 1;
                break;
            }
            default: break;
        }
        int bad = (c_hi != r_hi || c_lo != r_lo) || (has_qbyte && c_qbyte != r_flags);
        if (bad) {
            mismatches++;
            if (mismatches <= 20)
                printf("MISMATCH op=%u mode=%u a=%04x:%016llx b=%04x:%016llx | "
                       "C=%04x:%016llx q=%02llx  RS=%04x:%016llx q=%02llx\n",
                       op, mode, ah, al, bh, bl, c_hi, c_lo, c_qbyte, r_hi, r_lo, r_flags);
        }
    }
    fprintf(stderr, "total=%ld value-mismatches=%ld\n", total, mismatches);
    return mismatches ? 1 : 0;
}

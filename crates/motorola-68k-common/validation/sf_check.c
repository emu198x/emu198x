/* Reads lines: a_high a_low b_high b_low mode op rust_high rust_low rust_flags
 * (all hex). Runs softfloat with the same inputs and prints MISMATCH lines
 * where the C (value, flags) differ from the Rust columns. Exit nonzero on
 * any mismatch. op: 0=add 1=sub 2=mul 3=div 4=sqrt(a) 5=floatx80_to_int32
 * 6=floatx80_to_float32 7=floatx80_to_float64 8=int32_to_floatx80(a_low)
 * 9=float32_to_floatx80(a_low) 10=float64_to_floatx80(a_low). */
#include "m68kcpu.h"
#include <stdio.h>
#include <stdlib.h>

extern int8 float_rounding_mode;
extern int8 float_exception_flags;

int main(void) {
    unsigned int ah, bh, mode, op;
    unsigned long long al, bl, r_lo, r_flags;
    unsigned int r_hi;
    long mismatches = 0, total = 0;
    while (scanf("%x %llx %x %llx %x %x %x %llx %llx",
                 &ah, &al, &bh, &bl, &mode, &op, &r_hi, &r_lo, &r_flags) == 9) {
        total++;
        floatx80 a, b, z; a.high = ah; a.low = al; b.high = bh; b.low = bl;
        float_rounding_mode = (int8)mode;
        float_exception_flags = 0;
        unsigned int c_hi = 0; unsigned long long c_lo = 0;
        switch (op) {
            case 0: z = floatx80_add(a, b); c_hi = z.high; c_lo = z.low; break;
            case 1: z = floatx80_sub(a, b); c_hi = z.high; c_lo = z.low; break;
            case 2: z = floatx80_mul(a, b); c_hi = z.high; c_lo = z.low; break;
            case 3: z = floatx80_div(a, b); c_hi = z.high; c_lo = z.low; break;
            case 4: z = floatx80_sqrt(a);   c_hi = z.high; c_lo = z.low; break;
            case 5: { sint32 i = floatx80_to_int32(a); c_lo = (unsigned int)i; break; }
            case 6: { float32 f = floatx80_to_float32(a); c_lo = f; break; }
            case 7: { float64 f = floatx80_to_float64(a); c_lo = f; break; }
            case 8: { z = int32_to_floatx80((sint32)(unsigned int)al); c_hi = z.high; c_lo = z.low; break; }
            case 9: { z = float32_to_floatx80((float32)(unsigned int)al); c_hi = z.high; c_lo = z.low; break; }
            case 10:{ z = float64_to_floatx80((float64)al); c_hi = z.high; c_lo = z.low; break; }
            default: break;
        }
        unsigned long long c_flags = (unsigned char)float_exception_flags;
        if (c_hi != r_hi || c_lo != r_lo || c_flags != r_flags) {
            mismatches++;
            if (mismatches <= 20)
                printf("MISMATCH op=%u mode=%u a=%04x:%016llx b=%04x:%016llx | "
                       "C=%04x:%016llx fl=%02llx  RS=%04x:%016llx fl=%02llx\n",
                       op, mode, ah, al, bh, bl, c_hi, c_lo, c_flags, r_hi, r_lo, r_flags);
        }
    }
    fprintf(stderr, "total=%ld mismatches=%ld\n", total, mismatches);
    return mismatches ? 1 : 0;
}

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
 * op: 21=floatx80_getman(a) (FGETMAN) — bit-exact today.
 *
 * Diagnostic ops for the pending SOFTFLOAT_68K re-base (these still diverge
 * from WinUAE because our floatx80 core is the generic-Berkeley lineage, which
 * differs from the 68881/2 on denormal-result retention, the infinity encoding
 * $7FFF:0 vs $7FFF:8000…, two-NaN propagation order, and the `-shiftCount`
 * subnormal exponent): 0=add 2=mul 20=getexp(a) (FGETEXP) 22=scale(a,b)
 * (FSCALE). */
#include <cstdint>
#include <cstdio>
#include "softfloat/softfloat.h"

int main(void) {
    unsigned int ah, bh, mode, op;
    unsigned long long al, bl, r_lo, r_flags;
    unsigned int r_hi;
    long mismatches = 0, total = 0;

    /* WinUAE 68k float_status: tininess before rounding, extended precision,
     * everything else default (matches fpp_softfloat.cpp::fp_set_mode). */
    float_status st = {};
    st.float_detect_tininess = float_tininess_before_rounding;
    st.floatx80_rounding_precision = 80;

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
        switch (op) {
            case 0:  z = floatx80_add(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 2:  z = floatx80_mul(a, b, &st);    c_hi = z.high; c_lo = z.low; break;
            case 20: z = floatx80_getexp(a, &st);    c_hi = z.high; c_lo = z.low; break;
            case 21: z = floatx80_getman(a, &st);    c_hi = z.high; c_lo = z.low; break;
            case 22: z = floatx80_scale(a, b, &st);  c_hi = z.high; c_lo = z.low; break;
            default: break;
        }
        if (c_hi != r_hi || c_lo != r_lo) {
            mismatches++;
            if (mismatches <= 20)
                printf("MISMATCH op=%u mode=%u a=%04x:%016llx b=%04x:%016llx | "
                       "C=%04x:%016llx  RS=%04x:%016llx\n",
                       op, mode, ah, al, bh, bl, c_hi, c_lo, r_hi, r_lo);
        }
    }
    fprintf(stderr, "total=%ld value-mismatches=%ld\n", total, mismatches);
    return mismatches ? 1 : 0;
}

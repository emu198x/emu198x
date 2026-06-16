/* Test-generator accessors for the 68881/2 FPU registers.
 *
 * Musashi keeps the FPU state (fpr[8], fpcr, fpsr, fpiar) as internal CPU
 * state — it is NOT reachable through m68k_get_reg / m68k_set_reg. These
 * thin accessors expose it so the Emu198x test generator can seed and
 * capture FP register state when producing 68881/2 fixtures (#112).
 *
 * Added file (not part of the vendored Musashi sources); see build.rs.
 */
#include "m68kcpu.h"

/* The SoftFloat rounding-mode global. Musashi's m68kfpu.c only syncs it
 * from FPCR when an FMOVE-to-FPCR runs, so when we seed FPCR directly we
 * must update it here too, or the arithmetic would round with a stale
 * mode. */
extern int8 float_rounding_mode;

void testgen_set_fpr(int i, unsigned short high, unsigned long long low)
{
	REG_FP[i].high = high;
	REG_FP[i].low = low;
}

void testgen_get_fpr(int i, unsigned short *high, unsigned long long *low)
{
	*high = REG_FP[i].high;
	*low = REG_FP[i].low;
}

void testgen_set_fpcr(unsigned int value)
{
	REG_FPCR = value;
	float_rounding_mode = (value >> 4) & 0x3;
}

void testgen_set_fpsr(unsigned int value)
{
	REG_FPSR = value;
}

void testgen_set_fpiar(unsigned int value)
{
	REG_FPIAR = value;
}

unsigned int testgen_get_fpcr(void)
{
	return REG_FPCR;
}

unsigned int testgen_get_fpsr(void)
{
	return REG_FPSR;
}

unsigned int testgen_get_fpiar(void)
{
	return REG_FPIAR;
}

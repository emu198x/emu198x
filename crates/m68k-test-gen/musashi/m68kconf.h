/* Custom m68kconf.h for m68k-test-gen single-step test vector generation.
 *
 * Key settings:
 * - All CPU models enabled (68000 through 68040)
 * - Instruction hook via OPT_SPECIFY_HANDLER to single-step
 * - Prefetch emulation ON (needed for 68000 IR/IRC)
 * - Address error emulation ON
 * - PMMU OFF (not needed for ISA testing)
 */

#ifndef M68KCONF__HEADER
#define M68KCONF__HEADER

#define OPT_OFF             0
#define OPT_ON              1
#define OPT_SPECIFY_HANDLER 2

#define M68K_COMPILE_FOR_MAME OPT_OFF

/* CPU models */
#define M68K_EMULATE_010    OPT_ON
#define M68K_EMULATE_EC020  OPT_ON
#define M68K_EMULATE_020    OPT_ON
#define M68K_EMULATE_030    OPT_ON
#define M68K_EMULATE_040    OPT_ON

/* Separate reads OFF — all reads go through m68k_read_memory_xx() */
#define M68K_SEPARATE_READS     OPT_OFF

/* Predecrement write splitting OFF — we handle all writes uniformly */
#define M68K_SIMULATE_PD_WRITES OPT_OFF

/* Interrupts: autovector, auto-clear (no callback needed for test gen) */
#define M68K_EMULATE_INT_ACK        OPT_OFF
#define M68K_INT_ACK_CALLBACK(A)    0

/* No breakpoint, reset, TAS, illegal, cmpild, rte callbacks */
#define M68K_EMULATE_BKPT_ACK       OPT_OFF
#define M68K_BKPT_ACK_CALLBACK()    do {} while(0)
#define M68K_EMULATE_TRACE          OPT_OFF
#define M68K_EMULATE_RESET          OPT_OFF
#define M68K_RESET_CALLBACK()       do {} while(0)
#define M68K_CMPILD_HAS_CALLBACK    OPT_OFF
#define M68K_CMPILD_CALLBACK(v,r)   do {} while(0)
#define M68K_RTE_HAS_CALLBACK       OPT_OFF
#define M68K_RTE_CALLBACK()         do {} while(0)
#define M68K_TAS_HAS_CALLBACK       OPT_OFF
#define M68K_TAS_CALLBACK()         do {} while(0)
#define M68K_ILLG_HAS_CALLBACK      OPT_OFF
#define M68K_ILLG_CALLBACK(opcode)  0

/* Function codes OFF (not needed for flat memory test environment) */
#define M68K_EMULATE_FC             OPT_OFF
#define M68K_SET_FC_CALLBACK(A)     do {} while(0)

/* PC change monitor OFF */
#define M68K_MONITOR_PC             OPT_OFF
#define M68K_SET_PC_CALLBACK(A)     do {} while(0)

/* Instruction hook: single-step mechanism.
 * The hook is called before each instruction. Our implementation calls
 * m68k_end_timeslice() after the first instruction to stop execution.
 */
#define M68K_INSTRUCTION_HOOK       OPT_SPECIFY_HANDLER
void testgen_instruction_hook(unsigned int pc);
#define M68K_INSTRUCTION_CALLBACK(pc) testgen_instruction_hook(pc)

/* Prefetch emulation ON — Musashi maintains a single-word lookahead.
 * After executing one instruction, Musashi's state maps to our DL convention:
 *   DL PC   = Musashi PC + 4
 *   DL IR   = Musashi PREF_DATA  (next instruction's opcode)
 *   DL IRC  = word at Musashi PC + 2
 */
#define M68K_EMULATE_PREFETCH       OPT_ON

/* Address error emulation ON — 68000 takes exception on unaligned access */
#define M68K_EMULATE_ADDRESS_ERROR  OPT_ON

/* Logging OFF */
#define M68K_LOG_ENABLE             OPT_OFF
#define M68K_LOG_1010_1111          OPT_OFF
#define M68K_LOG_FILEHANDLE         stderr

/* PMMU OFF (not needed for ISA testing) */
#define M68K_EMULATE_PMMU           OPT_OFF

/* 64-bit optimisations ON */
#define M68K_USE_64_BIT             OPT_ON

#endif /* M68KCONF__HEADER */

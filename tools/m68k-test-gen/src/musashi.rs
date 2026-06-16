//! FFI bindings to the Musashi 680x0 emulator.
//!
//! Musashi uses global state — only one CPU instance at a time.
//! All access must be serialised (single-threaded).

#![allow(dead_code)]

use std::ffi::c_uint;

// --- CPU type constants ---
pub const M68K_CPU_TYPE_68000: c_uint = 1;
pub const M68K_CPU_TYPE_68010: c_uint = 2;
pub const M68K_CPU_TYPE_68EC020: c_uint = 3;
pub const M68K_CPU_TYPE_68020: c_uint = 4;
pub const M68K_CPU_TYPE_68EC030: c_uint = 5;
pub const M68K_CPU_TYPE_68030: c_uint = 6;
pub const M68K_CPU_TYPE_68EC040: c_uint = 7;
pub const M68K_CPU_TYPE_68LC040: c_uint = 8;
pub const M68K_CPU_TYPE_68040: c_uint = 9;

// --- Register enum values (match m68k_register_t order in m68k.h) ---
pub const M68K_REG_D0: c_uint = 0;
pub const M68K_REG_A0: c_uint = 8;
pub const M68K_REG_A7: c_uint = 15;
pub const M68K_REG_PC: c_uint = 16;
pub const M68K_REG_SR: c_uint = 17;
pub const M68K_REG_USP: c_uint = 19;
pub const M68K_REG_ISP: c_uint = 20;
pub const M68K_REG_MSP: c_uint = 21;
pub const M68K_REG_SFC: c_uint = 22;
pub const M68K_REG_DFC: c_uint = 23;
pub const M68K_REG_VBR: c_uint = 24;
pub const M68K_REG_CACR: c_uint = 25;
pub const M68K_REG_CAAR: c_uint = 26;
pub const M68K_REG_PREF_ADDR: c_uint = 27;
pub const M68K_REG_PREF_DATA: c_uint = 28;
pub const M68K_REG_IR: c_uint = 30;

unsafe extern "C" {
    pub fn m68k_init();
    pub fn m68k_set_cpu_type(cpu_type: c_uint);
    pub fn m68k_pulse_reset();
    pub fn m68k_execute(num_cycles: i32) -> i32;
    pub fn m68k_end_timeslice();
    pub fn m68k_get_reg(context: *const (), reg: c_uint) -> c_uint;
    pub fn m68k_set_reg(reg: c_uint, value: c_uint);

    // 68881/2 FPU accessors (testgen_fpu_access.c — Musashi keeps FP state
    // internal, so these expose it for fixture generation).
    fn testgen_set_fpr(i: i32, high: u16, low: u64);
    fn testgen_get_fpr(i: i32, high: *mut u16, low: *mut u64);
    fn testgen_set_fpcr(value: c_uint);
    fn testgen_set_fpsr(value: c_uint);
    fn testgen_set_fpiar(value: c_uint);
    fn testgen_get_fpcr() -> c_uint;
    fn testgen_get_fpsr() -> c_uint;
    fn testgen_get_fpiar() -> c_uint;
}

/// Set FP data register `i` (0..7) to the extended `{high, low}` pattern.
pub fn set_fpr(i: usize, high: u16, low: u64) {
    unsafe { testgen_set_fpr(i as i32, high, low) }
}

/// Get FP data register `i` (0..7) as the extended `{high, low}` pattern.
pub fn get_fpr(i: usize) -> (u16, u64) {
    let mut high = 0u16;
    let mut low = 0u64;
    unsafe { testgen_get_fpr(i as i32, &mut high, &mut low) }
    (high, low)
}

/// Set FPCR (also syncs SoftFloat's rounding-mode global).
pub fn set_fpcr(value: u32) {
    unsafe { testgen_set_fpcr(value) }
}

/// Set FPSR.
pub fn set_fpsr(value: u32) {
    unsafe { testgen_set_fpsr(value) }
}

/// Set FPIAR.
pub fn set_fpiar(value: u32) {
    unsafe { testgen_set_fpiar(value) }
}

/// Get FPCR / FPSR / FPIAR.
pub fn get_fpcr() -> u32 {
    unsafe { testgen_get_fpcr() }
}
pub fn get_fpsr() -> u32 {
    unsafe { testgen_get_fpsr() }
}
pub fn get_fpiar() -> u32 {
    unsafe { testgen_get_fpiar() }
}

/// Get a register value from the currently running Musashi instance.
pub fn get_reg(reg: c_uint) -> u32 {
    // SAFETY: Musashi is single-threaded, context=NULL means current CPU.
    unsafe { m68k_get_reg(std::ptr::null(), reg) }
}

/// Set a register value on the currently running Musashi instance.
pub fn set_reg(reg: c_uint, value: u32) {
    // SAFETY: Musashi is single-threaded.
    unsafe { m68k_set_reg(reg, value) }
}

/// Initialise Musashi (call once at startup).
pub fn init() {
    unsafe { m68k_init() }
}

/// Set CPU type for subsequent operations.
pub fn set_cpu_type(cpu_type: c_uint) {
    unsafe { m68k_set_cpu_type(cpu_type) }
}

/// Execute instructions for up to `num_cycles` cycles.
/// Returns the number of cycles actually consumed.
pub fn execute(num_cycles: i32) -> i32 {
    unsafe { m68k_execute(num_cycles) }
}

/// Pulse the RESET pin. Reads SSP from address 0, PC from address 4.
pub fn pulse_reset() {
    unsafe { m68k_pulse_reset() }
}

/// Stop execution after the current instruction completes.
pub fn end_timeslice() {
    unsafe { m68k_end_timeslice() }
}

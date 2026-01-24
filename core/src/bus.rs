/// A bus that supports memory read/write operations.
///
/// This is the base trait for all systems. Memory-mapped I/O systems
/// (6502-based machines like the C64 or NES) use this directly.
///
/// All operations are cycle-accurate: reads and writes advance the system
/// clock, and `tick()` is used for internal CPU operations that don't
/// access memory.
pub trait Bus {
    /// Read a byte from the given address.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 3 T-states for Z80, 1 cycle for 6502), plus any additional
    /// delay from bus contention.
    fn read(&mut self, address: u32) -> u8;

    /// Write a byte to the given address.
    ///
    /// This advances the system clock by the appropriate number of cycles
    /// (typically 3 T-states for Z80, 1 cycle for 6502), plus any additional
    /// delay from bus contention.
    fn write(&mut self, address: u32, value: u8);

    /// Advance the system clock without performing a memory operation.
    ///
    /// Used for internal CPU operations (register transfers, ALU operations,
    /// etc.) that consume cycles but don't access the bus.
    fn tick(&mut self, cycles: u32);

    /// Fetch an opcode byte (M1 cycle for Z80).
    ///
    /// This is separate from `read()` because the Z80's M1 cycle has different
    /// timing characteristics, particularly for bus contention. On the Spectrum,
    /// M1 cycles check contention twice (at T1 and T2) rather than once.
    ///
    /// The default implementation just calls `read()`, which is correct for
    /// systems without special M1 timing requirements.
    ///
    /// Note: This only covers the memory access portion (3 T-states on Z80).
    /// The caller is responsible for the refresh cycle timing.
    fn fetch(&mut self, address: u32) -> u8 {
        self.read(address)
    }

    /// Advance the clock during internal CPU operations that reference an address.
    ///
    /// Some systems (like the ZX Spectrum) apply bus contention during internal
    /// CPU cycles if those cycles are associated with a contended memory address.
    /// For example, `INC (HL)` has an internal cycle between the read and write
    /// that should be contended if HL points to contended memory.
    ///
    /// The default implementation just calls `tick()`, which is correct for
    /// systems without this contention behavior.
    ///
    /// # Arguments
    /// * `address` - The memory address associated with this operation
    /// * `cycles` - Number of cycles to advance
    fn tick_address(&mut self, _address: u32, cycles: u32) {
        self.tick(cycles)
    }

    /// Handle the refresh cycle after an M1 fetch (Z80 specific).
    ///
    /// During the refresh cycle, the Z80 outputs the IR register (I << 8 | R)
    /// on the address bus. On systems like the ZX Spectrum, if IR points to
    /// contended memory, this cycle should also be contended.
    ///
    /// The default implementation just ticks 1 cycle, which is correct for
    /// systems without refresh contention or non-Z80 systems.
    ///
    /// # Arguments
    /// * `ir` - The IR register value (I << 8 | R)
    fn refresh(&mut self, _ir: u16) {
        self.tick(1)
    }

    /// Handle the interrupt acknowledge cycle (Z80 specific).
    ///
    /// During interrupt acknowledge, the Z80 performs a special I/O-like cycle
    /// where both IORQ and M1 are active. The IR register is output on the
    /// address bus during this cycle.
    ///
    /// On systems like the ZX Spectrum, this cycle should be contended if IR
    /// points to contended memory. The timing is approximately 7 T-states
    /// (5 for acknowledge + 2 internal) before the stack push begins.
    ///
    /// The default implementation just ticks 7 cycles, which is correct for
    /// systems without this contention behavior.
    ///
    /// # Arguments
    /// * `ir` - The IR register value (I << 8 | R)
    fn interrupt_acknowledge(&mut self, _ir: u16) {
        self.tick(7)
    }
}

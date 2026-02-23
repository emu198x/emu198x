//! Reactive M68k Bus Trait.

/// Function codes used by the 68000 to indicate the type of bus cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCode {
    UserData = 1,
    UserProgram = 2,
    SupervisorData = 5,
    SupervisorProgram = 6,
    InterruptAcknowledge = 7,
}

/// The status of a bus request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusStatus {
    /// The bus cycle is complete. For a read, contains the data word.
    Ready(u16),
    /// The bus is not ready yet (e.g. DMA is active or peripheral is slow).
    Wait,
    /// A bus error (/BERR) occurred.
    Error,
}

/// The reactive bus trait.
/// 
/// Instead of "Read this now", the CPU says "I am starting a cycle at this address".
/// The bus returns whether it's ready or needs to wait.
pub trait M68kBus {
    /// Start or continue a bus cycle.
    /// 
    /// - `addr`: The 24-bit address.
    /// - `fc`: The function code.
    /// - `is_read`: True for read, false for write.
    /// - `size`: 1 for byte, 2 for word (long is handled as two words).
    /// - `data`: For writes, the data being written.
    fn poll_cycle(
        &mut self,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus;
}

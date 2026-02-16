//! Reactive M68k Bus Trait.

/// Function code values from the 68000's FC0-FC2 pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCode {
    UserData = 1,
    UserProgram = 2,
    SupervisorData = 5,
    SupervisorProgram = 6,
    InterruptAck = 7,
}

/// The status of a bus request in the reactive polling model.
/// Matches the 68000's /DTACK and /BERR mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusStatus {
    /// The bus cycle is complete. For a read, contains the data word.
    Ready(u16),
    /// The bus is not ready yet (/DTACK not asserted).
    Wait,
    /// A bus error (/BERR) occurred.
    Error,
}

/// The reactive bus trait.
pub trait M68kBus {
    /// Poll for the completion of a bus cycle.
    /// 
    /// - `addr`: The 24-bit address.
    /// - `fc`: The function code.
    /// - `is_read`: True for read, false for write.
    /// - `is_word`: True for word (16-bit), false for byte (8-bit).
    /// - `data`: For writes, the data being written.
    fn poll_cycle(
        &mut self,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus;

    /// Poll the current interrupt priority level (IPL0-IPL2).
    fn poll_ipl(&mut self) -> u8;

    /// Poll for the completion of an interrupt acknowledge cycle.
    fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus;

    /// Assert the RESET line on the bus.
    fn reset(&mut self);
}

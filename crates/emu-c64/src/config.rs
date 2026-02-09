//! C64 configuration: model selection and ROM images.

/// C64 model variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C64Model {
    /// PAL C64 (6569 VIC-II, 985,248 Hz CPU).
    C64Pal,
    /// NTSC C64 (6567 VIC-II, 1,022,727 Hz CPU).
    C64Ntsc,
}

/// Configuration for constructing a C64 instance.
pub struct C64Config {
    /// Model variant.
    pub model: C64Model,
    /// Kernal ROM (8,192 bytes).
    pub kernal_rom: Vec<u8>,
    /// BASIC ROM (8,192 bytes).
    pub basic_rom: Vec<u8>,
    /// Character ROM (4,096 bytes).
    pub char_rom: Vec<u8>,
}

//! Spectrum model configuration.

/// Supported Spectrum models.
///
/// The emulator uses trait objects (`Box<dyn SpectrumMemory>`, `Box<dyn SpectrumVideo>`)
/// internally, selected by this enum at construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectrumModel {
    // Sinclair
    Spectrum48K,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus3,
    // Timex
    TimexTC2048,
    TimexTS2068,
    // Russian/Eastern European
    Pentagon128,
    ScorpionZS256,
    // Modern
    SpectrumNext,
}

/// Configuration for creating a Spectrum instance.
pub struct SpectrumConfig {
    pub model: SpectrumModel,
    /// ROM data. Must be the correct size for the model (16,384 bytes for 48K).
    pub rom: Vec<u8>,
}

//! Amiga system configuration.

/// Configuration for an Amiga 500 PAL system.
pub struct AmigaConfig {
    /// Kickstart ROM data (256K).
    pub kickstart: Vec<u8>,
}

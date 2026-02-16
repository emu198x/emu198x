//! Configuration for the Amiga Rock emulator.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
}

pub struct AmigaConfig {
    pub model: AmigaModel,
    pub kickstart: Vec<u8>,
}

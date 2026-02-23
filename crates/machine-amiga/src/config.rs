//! Configuration for the Amiga machine crate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
}

#[derive(Debug, Clone)]
pub struct AmigaConfig {
    pub model: AmigaModel,
    pub kickstart: Vec<u8>,
}

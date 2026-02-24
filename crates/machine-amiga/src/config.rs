//! Configuration for the Amiga machine crate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaModel {
    A500,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmigaChipset {
    Ocs,
    Ecs,
}

impl Default for AmigaChipset {
    fn default() -> Self {
        Self::Ocs
    }
}

#[derive(Debug, Clone)]
pub struct AmigaConfig {
    pub model: AmigaModel,
    pub chipset: AmigaChipset,
    pub kickstart: Vec<u8>,
}

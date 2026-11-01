//! Fixed PAL configuration over the shared SNES engine.
//! Specific PCB/chip populations and CIC execution remain unqualified.
use common_nintendo_snes::{Region, owner::Configuration as BoardConfiguration};
#[derive(Clone, Debug)]
pub struct Configuration;
impl BoardConfiguration for Configuration {
    const REGION: Region = Region::Pal;
    const MACHINE_ID: &'static str = "nintendo-snes-pal";
}
pub type Snes = common_nintendo_snes::owner::SnesCore<Configuration>;

#[cfg(test)]
mod tests;

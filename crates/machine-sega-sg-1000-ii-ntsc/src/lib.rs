//! Fixed sega-sg-1000-ii-ntsc configuration. Later PCB/chip populations are unqualified.
use common_sega_sg_1000::{
    Sg1000Region,
    owner::{Configuration, Sg1000Core},
};
pub struct Board;
impl Configuration for Board {
    const REGION: Sg1000Region = Sg1000Region::Ntsc;
    const MACHINE_ID: &'static str = "sega-sg-1000-ii-ntsc";
}
pub const MACHINE_ID: &str = "sega-sg-1000-ii-ntsc";
pub type Sg1000 = Sg1000Core<Board>;
#[cfg(test)]
mod tests;

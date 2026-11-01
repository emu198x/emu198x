//! Closed dispatch between the implemented NTSC and PAL owners.
use crate::Model;
use common_nintendo_snes::{Apu, Buttons, Cartridge, Pad, Snes};
#[derive(Clone, Debug)]
pub enum SnesMachine {
    Ntsc(machine_nintendo_snes_ntsc::Snes),
    Pal(machine_nintendo_snes_pal::Snes),
}
impl SnesMachine {
    pub(crate) fn new(model: Model, cart: Cartridge, ipl: Option<[u8; 64]>) -> Self {
        match model {
            Model::SnesPal => Self::Pal(machine_nintendo_snes_pal::Snes::new(cart, ipl)),
            Model::SnesNtsc | Model::SuperFamicom => {
                Self::Ntsc(machine_nintendo_snes_ntsc::Snes::new(cart, ipl))
            }
        }
    }
    pub(crate) fn from_state(model: Model, state: Snes) -> Result<Self, String> {
        Ok(match model {
            Model::SnesPal => Self::Pal(machine_nintendo_snes_pal::Snes::from_state(state)?),
            Model::SnesNtsc | Model::SuperFamicom => {
                Self::Ntsc(machine_nintendo_snes_ntsc::Snes::from_state(state)?)
            }
        })
    }
    pub(crate) fn state(&self) -> &Snes {
        match self {
            Self::Ntsc(m) => m,
            Self::Pal(m) => m,
        }
    }
    pub fn load_cartridge_ram(&mut self, bytes: &[u8]) {
        match self {
            Self::Ntsc(m) => m.load_cartridge_ram(bytes),
            Self::Pal(m) => m.load_cartridge_ram(bytes),
        }
    }
    pub fn apu_mut(&mut self) -> &mut Apu {
        match self {
            Self::Ntsc(m) => m.apu_mut(),
            Self::Pal(m) => m.apu_mut(),
        }
    }
    pub fn pads_mut(&mut self) -> &mut [Pad; 2] {
        match self {
            Self::Ntsc(m) => m.pads_mut(),
            Self::Pal(m) => m.pads_mut(),
        }
    }
    pub fn tick(&mut self) {
        match self {
            Self::Ntsc(m) => m.tick(),
            Self::Pal(m) => m.tick(),
        }
    }
    pub fn reset(&mut self) {
        match self {
            Self::Ntsc(m) => m.reset(),
            Self::Pal(m) => m.reset(),
        }
    }
    pub fn run_frame(&mut self) -> u64 {
        match self {
            Self::Ntsc(m) => m.run_frame(),
            Self::Pal(m) => m.run_frame(),
        }
    }
    pub fn take_audio(&mut self) -> Vec<f32> {
        match self {
            Self::Ntsc(m) => m.take_audio(),
            Self::Pal(m) => m.take_audio(),
        }
    }
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        match self {
            Self::Ntsc(m) => m.set_buttons(port, buttons),
            Self::Pal(m) => m.set_buttons(port, buttons),
        }
    }
    pub fn poke(&mut self, address: u32, value: u8) {
        match self {
            Self::Ntsc(m) => m.poke(address, value),
            Self::Pal(m) => m.poke(address, value),
        }
    }
    pub fn step_instruction(&mut self, limit: u64) -> u64 {
        match self {
            Self::Ntsc(m) => m.step_instruction(limit),
            Self::Pal(m) => m.step_instruction(limit),
        }
    }
}
impl std::ops::Deref for SnesMachine {
    type Target = Snes;
    fn deref(&self) -> &Snes {
        self.state()
    }
}

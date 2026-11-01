//! Closed dispatch between fixed console configurations.
use crate::Model;
use common_sega_sg_1000::{ControllerState, IoEvent, Sg1000};
use emu198x_zilog_z80::{Z80, Z80Stepper};
pub enum Sg1000Machine {
    Ntsc(machine_sega_sg_1000_ntsc::Sg1000),
    Pal(machine_sega_sg_1000_pal::Sg1000),
    Ii(machine_sega_sg_1000_ii_ntsc::Sg1000),
}
impl Sg1000Machine {
    pub fn machine_id(&self) -> &'static str {
        match self {
            Self::Ntsc(m) => m.machine_id(),
            Self::Pal(m) => m.machine_id(),
            Self::Ii(m) => m.machine_id(),
        }
    }
    pub(crate) fn new(model: Model, cart: Vec<u8>) -> Self {
        match model {
            Model::Sg1000Ntsc => Self::Ntsc(machine_sega_sg_1000_ntsc::Sg1000::new(cart)),
            Model::Sg1000Pal => Self::Pal(machine_sega_sg_1000_pal::Sg1000::new(cart)),
            Model::Sg1000IiNtsc => Self::Ii(machine_sega_sg_1000_ii_ntsc::Sg1000::new(cart)),
        }
    }
    pub(crate) fn from_state(model: Model, state: Sg1000) -> Result<Self, String> {
        Ok(match model {
            Model::Sg1000Ntsc => Self::Ntsc(machine_sega_sg_1000_ntsc::Sg1000::from_state(state)?),
            Model::Sg1000Pal => Self::Pal(machine_sega_sg_1000_pal::Sg1000::from_state(state)?),
            Model::Sg1000IiNtsc => {
                Self::Ii(machine_sega_sg_1000_ii_ntsc::Sg1000::from_state(state)?)
            }
        })
    }
    pub(crate) fn state(&self) -> &Sg1000 {
        match self {
            Self::Ntsc(m) => m,
            Self::Pal(m) => m,
            Self::Ii(m) => m,
        }
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        match self {
            Self::Ntsc(m) => m.run_native_frame(),
            Self::Pal(m) => m.run_native_frame(),
            Self::Ii(m) => m.run_native_frame(),
        }
    }
    pub fn run_frame(&mut self) -> u64 {
        match self {
            Self::Ntsc(m) => m.run_frame(),
            Self::Pal(m) => m.run_frame(),
            Self::Ii(m) => m.run_frame(),
        }
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        match self {
            Self::Ntsc(m) => m.take_audio_buffer(),
            Self::Pal(m) => m.take_audio_buffer(),
            Self::Ii(m) => m.take_audio_buffer(),
        }
    }
    pub fn controller1_mut(&mut self) -> &mut ControllerState {
        match self {
            Self::Ntsc(m) => m.controller1_mut(),
            Self::Pal(m) => m.controller1_mut(),
            Self::Ii(m) => m.controller1_mut(),
        }
    }
    pub fn controller2_mut(&mut self) -> &mut ControllerState {
        match self {
            Self::Ntsc(m) => m.controller2_mut(),
            Self::Pal(m) => m.controller2_mut(),
            Self::Ii(m) => m.controller2_mut(),
        }
    }
    pub fn set_pause_pressed(&mut self, pressed: bool) {
        match self {
            Self::Ntsc(m) => m.set_pause_pressed(pressed),
            Self::Pal(m) => m.set_pause_pressed(pressed),
            Self::Ii(m) => m.set_pause_pressed(pressed),
        }
    }
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        match self {
            Self::Ntsc(m) => m.cpu_mut(),
            Self::Pal(m) => m.cpu_mut(),
            Self::Ii(m) => m.cpu_mut(),
        }
    }
    pub fn poke(&mut self, address: u16, value: u8) {
        match self {
            Self::Ntsc(m) => m.poke(address, value),
            Self::Pal(m) => m.poke(address, value),
            Self::Ii(m) => m.poke(address, value),
        }
    }
    pub fn start_io_trace(&mut self) {
        match self {
            Self::Ntsc(m) => m.start_io_trace(),
            Self::Pal(m) => m.start_io_trace(),
            Self::Ii(m) => m.start_io_trace(),
        }
    }
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        match self {
            Self::Ntsc(m) => m.take_io_trace(),
            Self::Pal(m) => m.take_io_trace(),
            Self::Ii(m) => m.take_io_trace(),
        }
    }
}
impl std::ops::Deref for Sg1000Machine {
    type Target = Sg1000;
    fn deref(&self) -> &Sg1000 {
        self.state()
    }
}

impl Z80Stepper for Sg1000Machine {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu().instructions_retired()
    }
    fn step_tick(&mut self) {
        match self {
            Self::Ntsc(m) => m.step_tick(),
            Self::Pal(m) => m.step_tick(),
            Self::Ii(m) => m.step_tick(),
        }
    }
}

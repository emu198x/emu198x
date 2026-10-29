//! Family-owned dispatch for the two statically wired ColecoVision boards.
use crate::profiles::Model;
use common_coleco_colecovision::{CvController, CvRegion, CvState, IoEvent, Tms9918, Z80};
use emu198x_zilog_z80::Z80Stepper;
use machine_coleco_colecovision_ntsc::ColecoVision as Ntsc;
use machine_coleco_colecovision_pal::ColecoVision as Pal;
use serde::{Deserialize, Serialize};
/// Concrete boards served by the family runtime.
pub enum CvMachine {
    Ntsc(Ntsc),
    Pal(Pal),
}
macro_rules! dispatch {
    ($s:expr, $m:ident => $body:expr) => {
        match $s {
            CvMachine::Ntsc($m) => $body,
            CvMachine::Pal($m) => $body,
        }
    };
}
impl CvMachine {
    pub(crate) fn new(model: Model, bios: Vec<u8>, cart: Vec<u8>) -> Self {
        match model {
            Model::CvNtsc => Self::Ntsc(Ntsc::new(bios, cart)),
            Model::CvPal => Self::Pal(Pal::new(bios, cart)),
        }
    }
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self, m => m.step_instruction())
    }
    pub fn cartridge_rom(&self) -> &[u8] {
        dispatch!(self, m => m.cartridge_rom())
    }
    pub fn firmware_rom(&self) -> &[u8] {
        dispatch!(self, m => m.firmware_rom())
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self, m => m.run_native_frame())
    }
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self, m => m.run_frame())
    }
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self, m => m.framebuffer())
    }
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_width())
    }
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_height())
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self, m => m.take_audio_buffer())
    }
    pub fn controller1_mut(&mut self) -> &mut CvController {
        dispatch!(self, m => m.controller1_mut())
    }
    pub fn controller2_mut(&mut self) -> &mut CvController {
        dispatch!(self, m => m.controller2_mut())
    }
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self, m => m.cpu())
    }
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self, m => m.cpu_mut())
    }
    pub fn vdp(&self) -> &Tms9918 {
        dispatch!(self, m => m.vdp())
    }
    pub fn region(&self) -> CvRegion {
        dispatch!(self, m => m.region())
    }
    pub fn cpu_cycles(&self) -> u64 {
        dispatch!(self, m => m.cpu_cycles())
    }
    pub fn frame_count(&self) -> u64 {
        dispatch!(self, m => m.frame_count())
    }
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self, m => m.peek(addr))
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self, m => m.poke(addr, value))
    }
    pub fn start_io_trace(&mut self) {
        dispatch!(self, m => m.start_io_trace())
    }
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self, m => m.take_io_trace())
    }
}
impl Z80Stepper for CvMachine {
    fn step_tick(&mut self) {
        dispatch!(self, m => m.step_tick())
    }
    fn z80_instructions_retired(&self) -> u64 {
        dispatch!(self, m => m.z80_instructions_retired())
    }
}
impl Serialize for CvMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self, m => m.serialize(s))
    }
}
impl<'de> Deserialize<'de> for CvMachine {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let state = CvState::deserialize(d)?;
        match state.region() {
            CvRegion::Ntsc => Ntsc::from_state(state)
                .map(Self::Ntsc)
                .map_err(serde::de::Error::custom),
            CvRegion::Pal => Pal::from_state(state)
                .map(Self::Pal)
                .map_err(serde::de::Error::custom),
        }
    }
}

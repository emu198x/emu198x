//! Family-owned dispatch; concrete machine crates cannot select another board.
use crate::profiles::Model;
use common_sord_m5::{Ctc, IoEvent, M5Region, M5State, NUM_KEY_ROWS, Tms9918, Z80};
use machine_sord_m5_ntsc::SordM5 as NtscM5;
use machine_sord_m5_pal::SordM5 as PalM5;
use serde::{Deserialize, Serialize};
/// The two concrete boards served by the shared runtime.
pub enum M5Machine {
    Ntsc(NtscM5),
    Pal(PalM5),
}
macro_rules! dispatch {
    ($self:expr,$m:ident=>$body:expr) => {
        match $self {
            M5Machine::Ntsc($m) => $body,
            M5Machine::Pal($m) => $body,
        }
    };
}
impl M5Machine {
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self, machine => emu198x_zilog_z80::Z80Stepper::step_instruction(machine))
    }

    pub(crate) fn new(model: Model, rom: Vec<u8>, cart: Vec<u8>) -> Self {
        match model {
            Model::M5Ntsc => Self::Ntsc(NtscM5::new(rom, cart)),
            Model::M5Pal => Self::Pal(PalM5::new(rom, cart)),
        }
    }
    pub fn cartridge_rom(&self) -> &[u8] {
        dispatch!(self, machine => machine.cartridge_rom())
    }
    pub fn firmware_rom(&self) -> &[u8] {
        dispatch!(self, machine => machine.firmware_rom())
    }
    pub fn irq_acks_by_channel(&self) -> [u64; 4] {
        dispatch!(self, machine => machine.irq_acks_by_channel())
    }
    pub fn irq_acks(&self) -> u64 {
        dispatch!(self, machine => machine.irq_acks())
    }
    pub fn set_cart_ram_size(&mut self, bytes: usize) {
        dispatch!(self, machine => machine.set_cart_ram_size(bytes))
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self, machine => machine.run_native_frame())
    }
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self, machine => machine.run_frame())
    }
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self, machine => machine.framebuffer())
    }
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self, machine => machine.framebuffer_width())
    }
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self, machine => machine.framebuffer_height())
    }
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self, machine => machine.peek(addr))
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self, machine => machine.poke(addr, value))
    }
    pub fn start_io_trace(&mut self) {
        dispatch!(self, machine => machine.start_io_trace())
    }
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self, machine => machine.take_io_trace())
    }
    pub fn run_until_pc(&mut self, target_pc: u16, max_tstates: u64) -> (u64, bool) {
        dispatch!(self, machine => machine.run_until_pc(target_pc, max_tstates))
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self, machine => machine.take_audio_buffer())
    }
    pub fn press_key(&mut self, row: usize, bit: u8) {
        dispatch!(self, machine => machine.press_key(row, bit))
    }
    pub fn release_key(&mut self, row: usize, bit: u8) {
        dispatch!(self, machine => machine.release_key(row, bit))
    }
    pub fn set_joystick(&mut self, port: u8, up: bool, down: bool, left: bool, right: bool) {
        dispatch!(self, machine => machine.set_joystick(port, up, down, left, right))
    }
    pub fn joystick_byte(&self) -> u8 {
        dispatch!(self, machine => machine.joystick_byte())
    }
    pub fn key_matrix_mut(&mut self) -> &mut [u8; NUM_KEY_ROWS] {
        dispatch!(self, machine => machine.key_matrix_mut())
    }
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self, machine => machine.cpu())
    }
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self, machine => machine.cpu_mut())
    }
    pub fn vdp(&self) -> &Tms9918 {
        dispatch!(self, machine => machine.vdp())
    }
    pub fn ctc(&self) -> &Ctc {
        dispatch!(self, machine => machine.ctc())
    }
    pub fn region(&self) -> M5Region {
        dispatch!(self, machine => machine.region())
    }
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self, machine => machine.cpu_tstates())
    }
    pub fn frame_count(&self) -> u64 {
        dispatch!(self, machine => machine.frame_count())
    }
    pub fn tstates_per_frame(&self) -> u64 {
        dispatch!(self, machine => machine.tstates_per_frame())
    }
}
// No enum tag is added to the v5 bytes. The legacy state already carries region.
impl Serialize for M5Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl<'de> Deserialize<'de> for M5Machine {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let state = M5State::deserialize(d)?;
        match state.region() {
            M5Region::Ntsc => NtscM5::from_state(state)
                .map(Self::Ntsc)
                .map_err(serde::de::Error::custom),
            M5Region::Pal => PalM5::from_state(state)
                .map(Self::Pal)
                .map_err(serde::de::Error::custom),
        }
    }
}

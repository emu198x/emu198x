//! Family-owned dispatch for the two statically wired SVI-328 boards.
use crate::profiles::Model;
use common_spectravideo_svi_328::CassetteMedia;
use common_spectravideo_svi_328::{AyWriteRecord, IoEvent, SviRegion, SviState, Tms9918, Z80};
use emu198x_zilog_z80::Z80Stepper;
use machine_spectravideo_svi_328_ntsc::Svi328 as Ntsc;
use machine_spectravideo_svi_328_pal::Svi328 as Pal;
use serde::{Deserialize, Serialize};
/// Concrete boards served by the family runtime.
pub enum SviMachine {
    Ntsc(Ntsc),
    Pal(Pal),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            SviMachine::Ntsc($m) => $body,
            SviMachine::Pal($m) => $body,
        }
    };
}
impl SviMachine {
    pub fn firmware_rom(&self) -> &[u8] {
        dispatch!(self,m=>m.firmware_rom())
    }
    pub fn cartridge_rom(&self) -> &[u8] {
        dispatch!(self,m=>m.cartridge_rom())
    }
    pub fn cassette_media(&self) -> Option<CassetteMedia> {
        dispatch!(self,m=>m.cassette_media())
    }
    pub fn insert_cassette_media(&mut self, media: &CassetteMedia) {
        dispatch!(self,m=>m.insert_cassette_media(media));
    }

    pub(crate) fn new(model: Model, rom: Vec<u8>) -> Self {
        match model {
            Model::Svi328Ntsc => Self::Ntsc(Ntsc::new(rom)),
            Model::Svi328Pal => Self::Pal(Pal::new(rom)),
        }
    }
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        dispatch!(self,m=>m.insert_cart(rom))
    }
    pub fn insert_cassette(&mut self, blocks: &[Vec<u8>]) {
        dispatch!(self,m=>m.insert_cassette(blocks))
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    pub fn press_key(&mut self, row: usize, bit: u8) {
        dispatch!(self,m=>m.press_key(row, bit))
    }
    pub fn release_key(&mut self, row: usize, bit: u8) {
        dispatch!(self,m=>m.release_key(row, bit))
    }
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        dispatch!(self,m=>m.set_joystick(port, up, down, left, right, fire))
    }
    pub fn joystick_dirs(&self) -> u8 {
        dispatch!(self,m=>m.joystick_dirs())
    }
    pub fn key_row(&self, row: usize) -> u8 {
        dispatch!(self,m=>m.key_row(row))
    }
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self,m=>m.cpu())
    }
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self,m=>m.cpu_mut())
    }
    pub fn vdp(&self) -> &Tms9918 {
        dispatch!(self,m=>m.vdp())
    }
    pub fn region(&self) -> SviRegion {
        dispatch!(self,m=>m.region())
    }
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self,m=>m.cpu_tstates())
    }
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    pub fn memory_control(&self) -> (bool, bool) {
        dispatch!(self,m=>m.memory_control())
    }
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    pub fn start_io_trace(&mut self) {
        dispatch!(self,m=>m.start_io_trace())
    }
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self,m=>m.take_io_trace())
    }
    pub fn start_ay_write_watch(&mut self) -> u32 {
        dispatch!(self,m=>m.start_ay_write_watch())
    }
    pub fn stop_ay_write_watch(&mut self) {
        dispatch!(self,m=>m.stop_ay_write_watch())
    }
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        dispatch!(self,m=>m.ay_write_watch_records())
    }
    pub fn clear_ay_write_watch_records(&mut self) {
        dispatch!(self,m=>m.clear_ay_write_watch_records())
    }
}
impl Z80Stepper for SviMachine {
    fn step_tick(&mut self) {
        dispatch!(self,m=>m.step_tick())
    }
    fn z80_instructions_retired(&self) -> u64 {
        dispatch!(self,m=>m.z80_instructions_retired())
    }
}
impl Serialize for SviMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl<'de> Deserialize<'de> for SviMachine {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let state = SviState::deserialize(d)?;
        match state.region() {
            SviRegion::Ntsc => Ntsc::from_state(state)
                .map(Self::Ntsc)
                .map_err(serde::de::Error::custom),
            SviRegion::Pal => Pal::from_state(state)
                .map(Self::Pal)
                .map_err(serde::de::Error::custom),
        }
    }
}

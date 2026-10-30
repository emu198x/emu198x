//! Closed dispatch over fixed regional Aquarius boards.
use crate::Model;
use common_mattel_aquarius::{AquariusRegion, AquariusState, AyWriteRecord, IoEvent};
use emu198x_zilog_z80::Z80;
/// Concrete board held by the family runtime.
pub enum Aquarius {
    Ntsc(machine_mattel_aquarius::Aquarius),
    Pal(machine_mattel_aquarius_pal::Aquarius),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Aquarius::Ntsc($m) => $body,
            Aquarius::Pal($m) => $body,
        }
    };
}
impl serde::Serialize for Aquarius {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>serde::Serialize::serialize(m,s))
    }
}
impl Aquarius {
    pub(crate) fn new(rom: Vec<u8>, expansion_kb: usize, model: Model) -> Self {
        match model {
            Model::Aquarius => {
                Self::Ntsc(machine_mattel_aquarius::Aquarius::new(rom, expansion_kb))
            }
            Model::AquariusPal => Self::Pal(machine_mattel_aquarius_pal::Aquarius::new(
                rom,
                expansion_kb,
            )),
        }
    }
    pub(crate) fn from_state(state: AquariusState, model: Model) -> Result<Self, String> {
        match model {
            Model::Aquarius => machine_mattel_aquarius::Aquarius::from_state(state).map(Self::Ntsc),
            Model::AquariusPal => {
                machine_mattel_aquarius_pal::Aquarius::from_state(state).map(Self::Pal)
            }
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Advance the selected engine by T states.
    pub fn advance_tstates(&mut self, tstates: u64) {
        dispatch!(self,m=>m.advance_tstates(tstates));
    }
    /// Delegate to the selected fixed owner.
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        dispatch!(self,m=>m.insert_cart(rom))
    }
    /// Delegate to the selected fixed owner.
    pub fn set_char_rom(&mut self, char_rom: Vec<u8>) {
        dispatch!(self,m=>m.set_char_rom(char_rom))
    }
    /// Delegate to the selected fixed owner.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Delegate to the selected fixed owner.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the selected fixed owner.
    pub fn region(&self) -> AquariusRegion {
        dispatch!(self,m=>m.region())
    }
    /// Delegate to the selected fixed owner.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    /// Delegate to the selected fixed owner.
    pub fn run_until_pc(&mut self, target: u16, max_tstates: u64) -> bool {
        dispatch!(self,m=>m.run_until_pc(target,max_tstates))
    }
    /// Delegate to the selected fixed owner.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the selected fixed owner.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Delegate to the selected fixed owner.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Delegate to the selected fixed owner.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Delegate to the selected fixed owner.
    pub fn set_key(&mut self, row: usize, col: u8, pressed: bool) {
        dispatch!(self,m=>m.set_key(row,col,pressed))
    }
    /// Delegate to the selected fixed owner.
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        dispatch!(self,m=>m.set_joystick(port,up,down,left,right,fire))
    }
    /// Delegate to the selected fixed owner.
    pub fn controller_bytes(&self) -> [u8; 2] {
        dispatch!(self,m=>m.controller_bytes())
    }
    /// Delegate to the selected fixed owner.
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the selected fixed owner.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Delegate to the selected fixed owner.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the selected fixed owner.
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self,m=>m.cpu_tstates())
    }
    /// Delegate to the selected fixed owner.
    pub fn speaker_bit(&self) -> bool {
        dispatch!(self,m=>m.speaker_bit())
    }
    /// Delegate to the selected fixed owner.
    pub fn start_ay_write_watch(&mut self) -> u32 {
        dispatch!(self,m=>m.start_ay_write_watch())
    }
    /// Delegate to the selected fixed owner.
    pub fn stop_ay_write_watch(&mut self) {
        dispatch!(self,m=>m.stop_ay_write_watch())
    }
    /// Delegate to the selected fixed owner.
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        dispatch!(self,m=>m.ay_write_watch_records())
    }
    /// Delegate to the selected fixed owner.
    pub fn clear_ay_write_watch_records(&mut self) {
        dispatch!(self,m=>m.clear_ay_write_watch_records())
    }
    /// Delegate to the selected fixed owner.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr,value))
    }
    /// Delegate to the selected fixed owner.
    pub fn start_io_trace(&mut self) {
        dispatch!(self,m=>m.start_io_trace())
    }
    /// Delegate to the selected fixed owner.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self,m=>m.take_io_trace())
    }
    /// Delegate to the selected fixed owner.
    pub fn bios(&self) -> &[u8] {
        dispatch!(self,m=>m.bios())
    }
    /// Delegate to the selected fixed owner.
    pub fn character_rom(&self) -> &[u8] {
        dispatch!(self,m=>m.character_rom())
    }
    /// Delegate to the selected fixed owner.
    pub fn cartridge(&self) -> &[u8] {
        dispatch!(self,m=>m.cartridge())
    }
    /// Delegate to the selected fixed owner.
    pub fn expansion_kb(&self) -> usize {
        dispatch!(self,m=>m.expansion_kb())
    }
}
impl emu198x_zilog_z80::Z80Stepper for Aquarius {
    fn z80_instructions_retired(&self) -> u64 {
        dispatch!(self,m=>emu198x_zilog_z80::Z80Stepper::z80_instructions_retired(m))
    }
    fn step_tick(&mut self) {
        dispatch!(self,m=>emu198x_zilog_z80::Z80Stepper::step_tick(m));
    }
}

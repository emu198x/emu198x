//! Family selection among fixed regional boards.
use crate::Model;
use common_atari_7800::*;
use machine_atari_7800::Atari7800 as Ntsc;
use machine_atari_7800_pal::Atari7800 as Pal;
/// Concrete physical owner held by the family runtime.
pub enum Atari7800Machine {
    Ntsc(Ntsc),
    Pal(Pal),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Atari7800Machine::Ntsc($m) => $body,
            Atari7800Machine::Pal($m) => $body,
        }
    };
}
impl Atari7800Machine {
    pub(crate) fn new(rom: Vec<u8>, region: Atari7800Region) -> Result<Self, String> {
        match region {
            Atari7800Region::Ntsc => Ntsc::new(rom).map(Self::Ntsc),
            Atari7800Region::Pal => Pal::new(rom).map(Self::Pal),
        }
    }
    pub(crate) fn from_state(model: Model, state: Atari7800State) -> Result<Self, String> {
        match model {
            Model::A7800Ntsc => Ntsc::from_state(state).map(Self::Ntsc),
            Model::A7800Pal => Pal::from_state(state).map(Self::Pal),
        }
    }
    pub(crate) fn cold_boot(&self, region: Atari7800Region) -> Self {
        match region {
            Atari7800Region::Ntsc => {
                dispatch!(self,m=>Self::Ntsc(m.cold_boot_for::<machine_atari_7800::Board>()))
            }
            Atari7800Region::Pal => {
                dispatch!(self,m=>Self::Pal(m.cold_boot_for::<machine_atari_7800_pal::Board>()))
            }
        }
    }
    /// Immutable physical board identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Delegate to the active board.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Delegate to the active board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the active board.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the active board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Delegate to the active board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Delegate to the active board.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_samples())
    }
    /// Delegate to the active board.
    pub fn take_pokey_audio_samples(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_pokey_audio_samples())
    }
    /// Delegate to the active board.
    pub fn audio_sample_rate(&self) -> u32 {
        dispatch!(self,m=>m.audio_sample_rate())
    }
    /// Delegate to the active board.
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        dispatch!(self,m=>m.set_joystick(up, down, left, right))
    }
    /// Delegate to the active board.
    pub fn set_console(&mut self, reset: bool, select: bool, pause: bool) {
        dispatch!(self,m=>m.set_console(reset, select, pause))
    }
    /// Delegate to the active board.
    pub fn set_fire(&mut self, pressed: bool) {
        dispatch!(self,m=>m.set_fire(pressed))
    }
    /// Delegate to the active board.
    pub fn set_fire2(&mut self, pressed: bool) {
        dispatch!(self,m=>m.set_fire2(pressed))
    }
    /// Delegate to the active board.
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the active board.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Delegate to the active board.
    pub fn maria(&self) -> &Maria {
        dispatch!(self,m=>m.maria())
    }
    /// Delegate to the active board.
    pub fn region(&self) -> Atari7800Region {
        dispatch!(self,m=>m.region())
    }
    /// Delegate to the active board.
    pub fn master_clock(&self) -> u64 {
        dispatch!(self,m=>m.master_clock())
    }
    /// Delegate to the active board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the active board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Delegate to the active board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    /// Delegate to the active board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
}
impl serde::Serialize for Atari7800Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>serde::Serialize::serialize(m,s))
    }
}

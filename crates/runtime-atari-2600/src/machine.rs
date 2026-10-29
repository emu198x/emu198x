//! Family selection among fixed regional boards.
use crate::Model;
use common_atari_2600::*;
use machine_atari_2600::Atari2600 as Ntsc;
use machine_atari_2600_pal::Atari2600 as Pal;
/// Concrete physical owner held by the family runtime.
pub enum Atari2600Machine {
    Ntsc(Ntsc),
    Pal(Pal),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Atari2600Machine::Ntsc($m) => $body,
            Atari2600Machine::Pal($m) => $body,
        }
    };
}
impl Atari2600Machine {
    pub(crate) fn new(rom: Vec<u8>, region: Atari2600Region) -> Result<Self, String> {
        match region {
            Atari2600Region::Ntsc => Ntsc::new(rom).map(Self::Ntsc),
            Atari2600Region::Pal => Pal::new(rom).map(Self::Pal),
        }
    }
    pub(crate) fn from_state(model: Model, state: Atari2600State) -> Result<Self, String> {
        match model {
            Model::Vcs2600Ntsc => Ntsc::from_state(state).map(Self::Ntsc),
            Model::Vcs2600Pal => Pal::from_state(state).map(Self::Pal),
        }
    }
    pub(crate) fn cold_boot(&self, region: Atari2600Region) -> Self {
        match region {
            Atari2600Region::Ntsc => {
                dispatch!(self,m=>Self::Ntsc(m.cold_boot_for::<machine_atari_2600::Board>()))
            }
            Atari2600Region::Pal => {
                dispatch!(self,m=>Self::Pal(m.cold_boot_for::<machine_atari_2600_pal::Board>()))
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
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_samples())
    }
    /// Delegate to the active board.
    pub fn audio_sample_rate(&self) -> u32 {
        dispatch!(self,m=>m.audio_sample_rate())
    }
    /// Delegate to the active board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Delegate to the active board.
    pub fn visible_framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.visible_framebuffer_width())
    }
    /// Delegate to the active board.
    pub fn hblank_clocks(&self) -> u32 {
        dispatch!(self,m=>m.hblank_clocks())
    }
    /// Delegate to the active board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Delegate to the active board.
    pub fn visible_framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.visible_framebuffer_height())
    }
    /// Delegate to the active board.
    pub fn visible_first_line(&self) -> u32 {
        dispatch!(self,m=>m.visible_first_line())
    }
    /// Delegate to the active board.
    pub fn set_joystick_input(&mut self, value: u8) {
        dispatch!(self,m=>m.set_joystick_input(value))
    }
    /// Delegate to the active board.
    pub fn set_switch_input(&mut self, value: u8) {
        dispatch!(self,m=>m.set_switch_input(value))
    }
    /// Delegate to the active board.
    pub fn set_paddle(&mut self, index: u8, value: u8) {
        dispatch!(self,m=>m.set_paddle(index, value))
    }
    /// Delegate to the active board.
    pub fn set_fire(&mut self, port: u8, pressed: bool) {
        dispatch!(self,m=>m.set_fire(port, pressed))
    }
    /// Delegate to the active board.
    pub fn set_booster_button(&mut self, port: u8, booster: bool, pressed: bool) {
        dispatch!(self,m=>m.set_booster_button(port, booster, pressed))
    }
    /// Delegate to the active board.
    pub fn set_genesis_button_c(&mut self, port: u8, pressed: bool) {
        dispatch!(self,m=>m.set_genesis_button_c(port, pressed))
    }
    /// Delegate to the active board.
    pub fn set_keypad_key(&mut self, port: u8, row: u8, col: u8, pressed: bool) {
        dispatch!(self,m=>m.set_keypad_key(port, row, col, pressed))
    }
    /// Delegate to the active board.
    pub fn detach_keypad(&mut self, port: u8) {
        dispatch!(self,m=>m.detach_keypad(port))
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
    pub fn tia(&self) -> &Tia {
        dispatch!(self,m=>m.tia())
    }
    /// Delegate to the active board.
    pub fn riot(&self) -> &Riot6532 {
        dispatch!(self,m=>m.riot())
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
    pub fn region(&self) -> Atari2600Region {
        dispatch!(self,m=>m.region())
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
impl serde::Serialize for Atari2600Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>serde::Serialize::serialize(m,s))
    }
}

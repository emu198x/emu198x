//! Family dispatch for the statically wired VIC-20 motherboards.
use common_commodore_vic_20::{
    M6502, Vic20Key, Vic20Model, Vic20RamExpansion, Vic20State, Vic6560,
};
use machine_commodore_vic_20_ntsc::Vic20 as Ntsc;
use machine_commodore_vic_20_pal::Vic20 as Pal;
use serde::{Deserialize, Serialize};
/// Concrete boards selected by the family runtime.
pub enum Vic20Machine {
    Ntsc(Ntsc),
    Pal(Pal),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Vic20Machine::Ntsc($m) => $body,
            Vic20Machine::Pal($m) => $body,
        }
    };
}
impl Vic20Machine {
    /// Construct the selected concrete board; model selection belongs to this family layer.
    pub fn new(
        kernal: Vec<u8>,
        basic: Vec<u8>,
        chars: Vec<u8>,
        model: Vic20Model,
        expansion: Vic20RamExpansion,
    ) -> Self {
        match model {
            Vic20Model::Ntsc => Self::Ntsc(Ntsc::new(kernal, basic, chars, expansion)),
            Vic20Model::Pal => Self::Pal(Pal::new(kernal, basic, chars, expansion)),
        }
    }
    /// Cold-boot the same concrete board with its installed ROMs and cartridge.
    pub fn cold_boot(&self, expansion: Vic20RamExpansion) -> Self {
        match self {
            Self::Ntsc(m) => Self::Ntsc(m.cold_boot(expansion)),
            Self::Pal(m) => Self::Pal(m.cold_boot(expansion)),
        }
    }
    /// Forward this operation to the active concrete board.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool, fire: bool) {
        dispatch!(self,m=>m.set_joystick(up, down, left, right, fire))
    }
    /// Forward this operation to the active concrete board.
    pub fn set_user_port_pb0(&mut self, high: bool) {
        dispatch!(self,m=>m.set_user_port_pb0(high))
    }
    /// Forward this operation to the active concrete board.
    pub fn user_port_pb0(&self) -> bool {
        dispatch!(self,m=>m.user_port_pb0())
    }
    /// Forward this operation to the active concrete board.
    pub fn user_port_cb2(&self) -> bool {
        dispatch!(self,m=>m.user_port_cb2())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_frame_with_user_port<F>(&mut self, exchange: &mut F) -> u64
    where
        F: FnMut(bool) -> bool,
    {
        dispatch!(self,m=>m.run_frame_with_user_port(exchange))
    }
    /// Forward this operation to the active concrete board.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_native_frame_with_user_port<F>(&mut self, exchange: &mut F) -> (u64, bool)
    where
        F: FnMut(bool) -> bool,
    {
        dispatch!(self,m=>m.run_native_frame_with_user_port(exchange))
    }
    /// Forward this operation to the active concrete board.
    pub fn expansion(&self) -> Vic20RamExpansion {
        dispatch!(self,m=>m.expansion())
    }
    /// Forward this operation to the active concrete board.
    pub fn insert_cartridge_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        dispatch!(self,m=>m.insert_cartridge_bytes(bytes))
    }
    /// Forward this operation to the active concrete board.
    pub fn remove_cartridge(&mut self) {
        dispatch!(self,m=>m.remove_cartridge())
    }
    /// Forward this operation to the active concrete board.
    pub fn cartridge_is_autostart(&self) -> bool {
        dispatch!(self,m=>m.cartridge_is_autostart())
    }
    /// Forward this operation to the active concrete board.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Forward this operation to the active concrete board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Forward this operation to the active concrete board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Forward this operation to the active concrete board.
    pub fn press_key(&mut self, key: Vic20Key) {
        dispatch!(self,m=>m.press_key(key))
    }
    /// Forward this operation to the active concrete board.
    pub fn release_key(&mut self, key: Vic20Key) {
        dispatch!(self,m=>m.release_key(key))
    }
    /// Forward this operation to the active concrete board.
    pub fn release_all_keys(&mut self) {
        dispatch!(self,m=>m.release_all_keys())
    }
    /// Forward this operation to the active concrete board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Forward this operation to the active concrete board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    /// Forward this operation to the active concrete board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
    /// Forward this operation to the active concrete board.
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self,m=>m.cpu())
    }
    /// Forward this operation to the active concrete board.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Forward this operation to the active concrete board.
    pub fn model(&self) -> Vic20Model {
        dispatch!(self,m=>m.model())
    }
    /// Forward this operation to the active concrete board.
    pub fn vic(&self) -> &Vic6560 {
        dispatch!(self,m=>m.vic())
    }
    /// Forward this operation to the active concrete board.
    pub fn take_vic_audio(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_vic_audio())
    }
    /// Forward this operation to the active concrete board.
    pub fn master_clock(&self) -> u64 {
        dispatch!(self,m=>m.master_clock())
    }
    /// Forward this operation to the active concrete board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
}
impl Serialize for Vic20Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl<'de> Deserialize<'de> for Vic20Machine {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let state = Vic20State::deserialize(d)?;
        match state.model() {
            Vic20Model::Ntsc => Ntsc::from_state(state)
                .map(Self::Ntsc)
                .map_err(serde::de::Error::custom),
            Vic20Model::Pal => Pal::from_state(state)
                .map(Self::Pal)
                .map_err(serde::de::Error::custom),
        }
    }
}

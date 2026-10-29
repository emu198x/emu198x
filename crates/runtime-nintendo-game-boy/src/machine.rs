//! Family-owned selection of fixed Game Boy execution boards.
use common_nintendo_game_boy::{
    ApuChannel, AudioControls, BootProfile, Cartridge, CartridgeHeader, GameBoyHardware,
    GameBoyState, JoypadButton, MemoryBus, Sm83,
};
use machine_nintendo_game_boy::GameBoy as Dmg;
use machine_nintendo_game_boy_advance::GameBoy as Agb;
use machine_nintendo_game_boy_color::GameBoy as Cgb;
use machine_nintendo_game_boy_pocket::GameBoy as Mgb;
use machine_nintendo_super_game_boy::GameBoy as Sgb;
use machine_nintendo_super_game_boy_2::GameBoy as Sgb2;
use serde::Serialize;
/// Concrete physical owners served by one family runtime.
#[derive(Clone)]
pub enum GameBoyMachine {
    Dmg(Dmg),
    Mgb(Mgb),
    Sgb(Sgb),
    Sgb2(Sgb2),
    Cgb(Cgb),
    Agb(Agb),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            GameBoyMachine::Dmg($m) => $body,
            GameBoyMachine::Mgb($m) => $body,
            GameBoyMachine::Sgb($m) => $body,
            GameBoyMachine::Sgb2($m) => $body,
            GameBoyMachine::Cgb($m) => $body,
            GameBoyMachine::Agb($m) => $body,
        }
    };
}
impl GameBoyMachine {
    /// Bind the selected existing profile to its concrete board.
    pub(crate) fn try_from_rom_with_boot_rom(
        rom: Vec<u8>,
        profile: BootProfile,
        boot: Option<Vec<u8>>,
    ) -> Result<(CartridgeHeader, Self), String> {
        match profile {
            BootProfile::Dmg0 | BootProfile::DmgAbc => {
                Dmg::try_from_rom_with_boot_rom(rom, profile, boot).map(|(h, m)| (h, Self::Dmg(m)))
            }
            BootProfile::Mgb => {
                Mgb::try_from_rom_with_boot_rom(rom, profile, boot).map(|(h, m)| (h, Self::Mgb(m)))
            }
            BootProfile::Sgb => {
                Sgb::try_from_rom_with_boot_rom(rom, profile, boot).map(|(h, m)| (h, Self::Sgb(m)))
            }
            BootProfile::Sgb2 => Sgb2::try_from_rom_with_boot_rom(rom, profile, boot)
                .map(|(h, m)| (h, Self::Sgb2(m))),
            BootProfile::Cgb => {
                Cgb::try_from_rom_with_boot_rom(rom, profile, boot).map(|(h, m)| (h, Self::Cgb(m)))
            }
            BootProfile::Agb => {
                Agb::try_from_rom_with_boot_rom(rom, profile, boot).map(|(h, m)| (h, Self::Agb(m)))
            }
        }
    }
    /// Bind the selected existing profile to its concrete board.
    pub(crate) fn try_new_with_boot_rom(
        cartridge: Cartridge,
        profile: BootProfile,
        boot: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        match profile {
            BootProfile::Dmg0 | BootProfile::DmgAbc => {
                Dmg::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Dmg)
            }
            BootProfile::Mgb => Mgb::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Mgb),
            BootProfile::Sgb => Sgb::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Sgb),
            BootProfile::Sgb2 => {
                Sgb2::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Sgb2)
            }
            BootProfile::Cgb => Cgb::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Cgb),
            BootProfile::Agb => Agb::try_new_with_boot_rom(cartridge, profile, boot).map(Self::Agb),
        }
    }
    /// Bind the selected existing profile to its concrete board.
    pub(crate) fn from_state(profile: BootProfile, state: GameBoyState) -> Result<Self, String> {
        match profile {
            BootProfile::Dmg0 | BootProfile::DmgAbc => {
                Dmg::from_state(profile, state).map(Self::Dmg)
            }
            BootProfile::Mgb => Mgb::from_state(profile, state).map(Self::Mgb),
            BootProfile::Sgb => Sgb::from_state(profile, state).map(Self::Sgb),
            BootProfile::Sgb2 => Sgb2::from_state(profile, state).map(Self::Sgb2),
            BootProfile::Cgb => Cgb::from_state(profile, state).map(Self::Cgb),
            BootProfile::Agb => Agb::from_state(profile, state).map(Self::Agb),
        }
    }
    /// The active concrete owner.
    pub const fn hardware(&self) -> GameBoyHardware {
        dispatch!(self,m=>m.hardware())
    }
    /// Forward this operation to the active concrete board.
    pub fn boot_rom(&self) -> Option<&[u8]> {
        dispatch!(self,m=>m.boot_rom())
    }
    /// Forward this operation to the active concrete board.
    pub fn framebuffer(&self) -> &[u8] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Forward this operation to the active concrete board.
    pub fn color_framebuffer(&self) -> Option<&[u16]> {
        dispatch!(self,m=>m.color_framebuffer())
    }
    /// Forward this operation to the active concrete board.
    pub fn is_cgb(&self) -> bool {
        dispatch!(self,m=>m.is_cgb())
    }
    /// Forward this operation to the active concrete board.
    pub fn cgb_mode(&self) -> bool {
        dispatch!(self,m=>m.cgb_mode())
    }
    /// Forward this operation to the active concrete board.
    pub fn double_speed(&self) -> bool {
        dispatch!(self,m=>m.double_speed())
    }
    /// Forward this operation to the active concrete board.
    pub const fn cpu_pc(&self) -> u16 {
        dispatch!(self,m=>m.cpu_pc())
    }
    /// Forward this operation to the active concrete board.
    pub fn drain_audio(&mut self, dest: &mut [f32]) -> usize {
        dispatch!(self,m=>m.drain_audio(dest))
    }
    /// Forward this operation to the active concrete board.
    pub const fn audio_controls(&self) -> AudioControls {
        dispatch!(self,m=>m.audio_controls())
    }
    /// Forward this operation to the active concrete board.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        dispatch!(self,m=>m.set_audio_controls(controls))
    }
    /// Forward this operation to the active concrete board.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        dispatch!(self,m=>m.set_audio_channel_enabled(channel,enabled))
    }
    /// Forward this operation to the active concrete board.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        dispatch!(self,m=>m.set_audio_channel_gain(channel,gain))
    }
    /// Forward this operation to the active concrete board.
    pub fn drain_serial(&mut self) -> Vec<u8> {
        dispatch!(self,m=>m.drain_serial())
    }
    /// Forward this operation to the active concrete board.
    pub fn set_button(&mut self, button: JoypadButton, pressed: bool) {
        dispatch!(self,m=>m.set_button(button,pressed))
    }
    /// Forward this operation to the active concrete board.
    pub fn cartridge(&self) -> &Cartridge {
        dispatch!(self,m=>m.cartridge())
    }
    /// Forward this operation to the active concrete board.
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        dispatch!(self,m=>m.cartridge_mut())
    }
    /// Forward this operation to the active concrete board.
    pub fn cpu(&self) -> &Sm83 {
        dispatch!(self,m=>m.cpu())
    }
    /// Forward this operation to the active concrete board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Forward this operation to the active concrete board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr,value))
    }
    /// Forward this operation to the active concrete board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
    /// Forward this operation to the active concrete board.
    pub fn step_m_cycle(&mut self) {
        dispatch!(self,m=>m.step_m_cycle())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_frame(&mut self) -> u32 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_frame_dots(&mut self) -> u32 {
        dispatch!(self,m=>m.run_frame_dots())
    }
    /// Forward this operation to the active concrete board.
    pub fn run_frame_dots_bounded(&mut self) -> (u32, bool) {
        dispatch!(self,m=>m.run_frame_dots_bounded())
    }
    /// Forward this operation to the active concrete board.
    pub fn validate_boot_state(&self, profile: BootProfile) -> Result<(), String> {
        dispatch!(self,m=>m.validate_boot_state(profile))
    }
}
impl Serialize for GameBoyMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl MemoryBus for GameBoyMachine {
    fn read(&mut self, addr: u16) -> u8 {
        dispatch!(self,m=>m.read(addr))
    }
    fn write(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.write(addr,value));
    }
}

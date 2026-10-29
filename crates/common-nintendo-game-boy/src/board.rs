//! Statically bound physical Game Boy owners over the legacy state engine.
use crate::{
    ApuChannel, AudioControls, BootProfile, Cartridge, CartridgeHeader, GameBoyState, HeaderError,
    JoypadButton, MemoryBus, Sm83,
};
use serde::Serialize;
/// Physical board identity; implemented execution surfaces are described by profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameBoyHardware {
    Dmg,
    Mgb,
    Sgb,
    Sgb2,
    Cgb,
    Agb,
}
/// Physical construction policy, supplied by one concrete machine crate.
pub trait GameBoyBoard {
    const HARDWARE: GameBoyHardware;
    const DEFAULT_BOOT_PROFILE: BootProfile;
    fn accepts_profile(profile: BootProfile) -> bool {
        profile == Self::DEFAULT_BOOT_PROFILE
    }
}
/// A fixed physical owner of the shared Game Boy state engine.
/// Legacy state needs its declared profile on restore; no board tag is added.
/// This type deliberately has no standalone `Deserialize` implementation:
/// the old payload cannot identify every board without its envelope profile.
pub struct GameBoyCore<B: GameBoyBoard> {
    state: GameBoyState,
    board: std::marker::PhantomData<B>,
}
impl<B: GameBoyBoard> Clone for GameBoyCore<B> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            board: std::marker::PhantomData,
        }
    }
}
impl<B: GameBoyBoard> Serialize for GameBoyCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: GameBoyBoard> GameBoyCore<B> {
    /// Construct this board from its normal skipped-boot state.
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            state: GameBoyState::new_with_boot_profile(cartridge, B::DEFAULT_BOOT_PROFILE),
            board: std::marker::PhantomData,
        }
    }
    /// Parse a cartridge for this fixed board.
    pub fn from_rom(rom: Vec<u8>) -> Result<(CartridgeHeader, Self), HeaderError> {
        GameBoyState::from_rom_with_boot_profile(rom, B::DEFAULT_BOOT_PROFILE).map(|(h, state)| {
            (
                h,
                Self {
                    state,
                    board: std::marker::PhantomData,
                },
            )
        })
    }
    /// Build only a boot profile belonging to this physical board.
    pub fn try_new_with_boot_rom(
        cartridge: Cartridge,
        profile: BootProfile,
        boot: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        if !B::accepts_profile(profile) {
            return Err("boot profile belongs to another Game Boy board".into());
        }
        let state = GameBoyState::try_new_with_boot_rom(cartridge, profile, boot)?;
        Self::from_state(profile, state)
    }
    /// Parse a cartridge and optional firmware without selecting another board.
    pub fn try_from_rom_with_boot_rom(
        rom: Vec<u8>,
        profile: BootProfile,
        boot: Option<Vec<u8>>,
    ) -> Result<(CartridgeHeader, Self), String> {
        if !B::accepts_profile(profile) {
            return Err("boot profile belongs to another Game Boy board".into());
        }
        let (h, state) = GameBoyState::try_from_rom_with_boot_rom(rom, profile, boot)?;
        Ok((h, Self::from_state(profile, state)?))
    }
    /// Bind exact legacy state using its declared profile, without reinitialising it.
    /// The payload alone cannot distinguish every monochrome physical board.
    pub fn from_state(profile: BootProfile, state: GameBoyState) -> Result<Self, String> {
        if !B::accepts_profile(profile) {
            return Err("snapshot profile belongs to another Game Boy board".into());
        }
        state.validate_boot_state(profile)?;
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// This owner's fixed hardware identity.
    pub const fn hardware(&self) -> GameBoyHardware {
        B::HARDWARE
    }
    /// Check declared profile and live hardware consistency.
    pub fn validate_boot_state(&self, profile: BootProfile) -> Result<(), String> {
        if !B::accepts_profile(profile) {
            return Err("snapshot profile belongs to another Game Boy board".into());
        }
        self.state.validate_boot_state(profile)
    }
    /// Forward to this board's unchanged state engine.
    pub fn boot_rom(&self) -> Option<&[u8]> {
        self.state.boot_rom()
    }
    /// Forward to this board's unchanged state engine.
    pub fn framebuffer(&self) -> &[u8] {
        self.state.framebuffer()
    }
    /// Forward to this board's unchanged state engine.
    pub fn color_framebuffer(&self) -> Option<&[u16]> {
        self.state.color_framebuffer()
    }
    /// Forward to this board's unchanged state engine.
    pub fn is_cgb(&self) -> bool {
        self.state.is_cgb()
    }
    /// Forward to this board's unchanged state engine.
    pub fn cgb_mode(&self) -> bool {
        self.state.cgb_mode()
    }
    /// Forward to this board's unchanged state engine.
    pub fn double_speed(&self) -> bool {
        self.state.double_speed()
    }
    /// Forward to this board's unchanged state engine.
    pub const fn cpu_pc(&self) -> u16 {
        self.state.cpu_pc()
    }
    /// Forward to this board's unchanged state engine.
    pub fn drain_audio(&mut self, dest: &mut [f32]) -> usize {
        self.state.drain_audio(dest)
    }
    /// Forward to this board's unchanged state engine.
    pub const fn audio_controls(&self) -> AudioControls {
        self.state.audio_controls()
    }
    /// Forward to this board's unchanged state engine.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.state.set_audio_controls(controls)
    }
    /// Forward to this board's unchanged state engine.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.state.set_audio_channel_enabled(channel, enabled)
    }
    /// Forward to this board's unchanged state engine.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.state.set_audio_channel_gain(channel, gain)
    }
    /// Forward to this board's unchanged state engine.
    pub fn drain_serial(&mut self) -> Vec<u8> {
        self.state.drain_serial()
    }
    /// Forward to this board's unchanged state engine.
    pub fn set_button(&mut self, button: JoypadButton, pressed: bool) {
        self.state.set_button(button, pressed)
    }
    /// Forward to this board's unchanged state engine.
    pub fn cartridge(&self) -> &Cartridge {
        self.state.cartridge()
    }
    /// Forward to this board's unchanged state engine.
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        self.state.cartridge_mut()
    }
    /// Forward to this board's unchanged state engine.
    pub fn cpu(&self) -> &Sm83 {
        self.state.cpu()
    }
    /// Forward to this board's unchanged state engine.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Forward to this board's unchanged state engine.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Forward to this board's unchanged state engine.
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
    /// Forward to this board's unchanged state engine.
    pub fn step_m_cycle(&mut self) {
        self.state.step_m_cycle()
    }
    /// Forward to this board's unchanged state engine.
    pub fn run_frame(&mut self) -> u32 {
        self.state.run_frame()
    }
    /// Forward to this board's unchanged state engine.
    pub fn run_frame_dots(&mut self) -> u32 {
        self.state.run_frame_dots()
    }
    /// Forward to this board's unchanged state engine.
    pub fn run_frame_dots_bounded(&mut self) -> (u32, bool) {
        self.state.run_frame_dots_bounded()
    }
}
impl<B: GameBoyBoard> MemoryBus for GameBoyCore<B> {
    fn read(&mut self, addr: u16) -> u8 {
        self.state.read(addr)
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.state.write(addr, value);
    }
}

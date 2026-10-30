//! Closed dispatch over three fixed physical boards.
use crate::Model;
use common_sega_mega_drive::{Buttons, Cpu68000, MegaDriveState, Region, Vdp, Z80, io};
/// The concrete board held by the family runtime.
pub enum MegaDrive {
    Japan(machine_sega_mega_drive_japan::MegaDrive),
    Americas(machine_sega_genesis::MegaDrive),
    Europe(machine_sega_mega_drive_europe::MegaDrive),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            MegaDrive::Japan($m) => $body,
            MegaDrive::Americas($m) => $body,
            MegaDrive::Europe($m) => $body,
        }
    };
}
impl serde::Serialize for MegaDrive {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>serde::Serialize::serialize(m,s))
    }
}
impl MegaDrive {
    pub(crate) fn new(rom: Vec<u8>, model: Model) -> Self {
        match model {
            Model::MegaDriveJapan => {
                Self::Japan(machine_sega_mega_drive_japan::MegaDrive::new(rom))
            }
            Model::Genesis => Self::Americas(machine_sega_genesis::MegaDrive::new(rom)),
            Model::MegaDriveEurope => {
                Self::Europe(machine_sega_mega_drive_europe::MegaDrive::new(rom))
            }
        }
    }
    pub(crate) fn new_power_base(rom: Vec<u8>, model: Model) -> Self {
        match model {
            Model::MegaDriveJapan => Self::Japan(
                machine_sega_mega_drive_japan::MegaDrive::new_power_base(rom),
            ),
            Model::Genesis => Self::Americas(machine_sega_genesis::MegaDrive::new_power_base(rom)),
            Model::MegaDriveEurope => Self::Europe(
                machine_sega_mega_drive_europe::MegaDrive::new_power_base(rom),
            ),
        }
    }
    pub(crate) fn from_state(state: MegaDriveState, model: Model) -> Result<Self, String> {
        match model {
            Model::MegaDriveJapan => {
                machine_sega_mega_drive_japan::MegaDrive::from_state(state).map(Self::Japan)
            }
            Model::Genesis => {
                machine_sega_genesis::MegaDrive::from_state(state).map(Self::Americas)
            }
            Model::MegaDriveEurope => {
                machine_sega_mega_drive_europe::MegaDrive::from_state(state).map(Self::Europe)
            }
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Access the selected board’s hardware.
    pub fn cpu(&self) -> &Cpu68000 {
        dispatch!(self,m=>m.cpu())
    }
    /// Access the selected board’s hardware.
    pub fn z80(&self) -> &Z80 {
        dispatch!(self,m=>m.z80())
    }
    /// Access the selected board’s hardware.
    pub fn vdp(&self) -> &Vdp {
        dispatch!(self,m=>m.vdp())
    }
    /// Access the selected board’s hardware.
    pub fn ports(&self) -> &[io::Port; 3] {
        dispatch!(self,m=>m.ports())
    }
    /// Access the selected board’s hardware.
    pub fn ports_mut(&mut self) -> &mut [io::Port; 3] {
        dispatch!(self,m=>m.ports_mut())
    }
    /// Access the selected board’s hardware.
    pub fn region(&self) -> Region {
        dispatch!(self,m=>m.region())
    }
    /// Delegate to the selected fixed owner.
    pub fn master_system_mode(&self) -> bool {
        dispatch!(self,m=>m.master_system_mode())
    }
    /// Delegate to the selected fixed owner.
    pub fn reset(&mut self) {
        dispatch!(self,m=>m.reset())
    }
    /// Delegate to the selected fixed owner.
    pub fn tick(&mut self) {
        dispatch!(self,m=>m.tick())
    }
    /// Delegate to the selected fixed owner.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the selected fixed owner.
    pub fn master_hz(&self) -> u64 {
        dispatch!(self,m=>m.master_hz())
    }
    /// Delegate to the selected fixed owner.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the selected fixed owner.
    pub fn frame_width(&self) -> u32 {
        dispatch!(self,m=>m.frame_width())
    }
    /// Delegate to the selected fixed owner.
    pub fn frame_height(&self) -> u32 {
        dispatch!(self,m=>m.frame_height())
    }
    /// Delegate to the selected fixed owner.
    pub fn take_audio(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio())
    }
    /// Delegate to the selected fixed owner.
    pub fn mclk(&self) -> u64 {
        dispatch!(self,m=>m.mclk())
    }
    /// Delegate to the selected fixed owner.
    pub fn ram(&self) -> &[u8] {
        dispatch!(self,m=>m.ram())
    }
    /// Delegate to the selected fixed owner.
    pub fn z80_ram(&self) -> &[u8] {
        dispatch!(self,m=>m.z80_ram())
    }
    /// Delegate to the selected fixed owner.
    pub fn peek(&self, address: u32) -> u8 {
        dispatch!(self,m=>m.peek(address))
    }
    /// Delegate to the selected fixed owner.
    pub fn poke(&mut self, address: u32, value: u8) {
        dispatch!(self,m=>m.poke(address,value))
    }
    /// Delegate to the selected fixed owner.
    pub fn rom(&self) -> &[u8] {
        dispatch!(self,m=>m.rom())
    }
    /// Delegate to the selected fixed owner.
    pub fn z80_bus_acknowledged(&self) -> bool {
        dispatch!(self,m=>m.z80_bus_acknowledged())
    }
    /// Delegate to the selected fixed owner.
    pub fn z80_in_reset(&self) -> bool {
        dispatch!(self,m=>m.z80_in_reset())
    }
    /// Delegate to the selected fixed owner.
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        dispatch!(self,m=>m.set_buttons(port,buttons))
    }
    /// Delegate to the selected fixed owner.
    pub fn finish_restore(&mut self) {
        dispatch!(self,m=>m.finish_restore())
    }
}

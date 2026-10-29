//! Family-owned dispatch among fixed Sega boards.
use common_sega_master_system::{
    CartridgeHeader, CartridgeMapper, FmUnit, IoEvent, SmsHardware, SmsState, SmsVariant, Ym2413,
};
use emu198x_sega_315_5124::SegaVdp;
use emu198x_zilog_z80::{Z80, Z80Stepper};
use machine_sega_game_gear::Sms as GameGear;
use machine_sega_mark_iii::Sms as MarkIII;
use machine_sega_master_system::Sms as MasterSystemNtsc;
use machine_sega_master_system_1_ntsc::Sms as MasterSystem1Ntsc;
use machine_sega_master_system_1_pal::Sms as MasterSystem1Pal;
use machine_sega_master_system_2_ntsc::Sms as MasterSystem2Ntsc;
use machine_sega_master_system_2_pal::Sms as MasterSystem2Pal;
use machine_sega_master_system_japan::Sms as MasterSystemJapan;
use machine_sega_master_system_pal::Sms as MasterSystemPal;
use serde::Serialize;
use std::sync::mpsc::{Receiver, Sender};
/// Physical board selected by the family catalogue.
pub enum Sms {
    MasterSystemNtsc(MasterSystemNtsc),
    MasterSystemPal(MasterSystemPal),
    MasterSystemJapan(MasterSystemJapan),
    MasterSystem1Ntsc(MasterSystem1Ntsc),
    MasterSystem1Pal(MasterSystem1Pal),
    MasterSystem2Ntsc(MasterSystem2Ntsc),
    MasterSystem2Pal(MasterSystem2Pal),
    MarkIII(MarkIII),
    GameGear(GameGear),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Sms::MasterSystemNtsc($m) => $body,
            Sms::MasterSystemPal($m) => $body,
            Sms::MasterSystemJapan($m) => $body,
            Sms::MasterSystem1Ntsc($m) => $body,
            Sms::MasterSystem1Pal($m) => $body,
            Sms::MasterSystem2Ntsc($m) => $body,
            Sms::MasterSystem2Pal($m) => $body,
            Sms::MarkIII($m) => $body,
            Sms::GameGear($m) => $body,
        }
    };
}
impl Sms {
    /// Construct the selected concrete physical board.
    pub fn new(rom: Vec<u8>, hardware: SmsHardware) -> Self {
        match hardware {
            SmsHardware::MasterSystemNtsc => Self::MasterSystemNtsc(MasterSystemNtsc::new(rom)),
            SmsHardware::MasterSystemPal => Self::MasterSystemPal(MasterSystemPal::new(rom)),
            SmsHardware::MasterSystemJapan => Self::MasterSystemJapan(MasterSystemJapan::new(rom)),
            SmsHardware::MasterSystem1Ntsc => Self::MasterSystem1Ntsc(MasterSystem1Ntsc::new(rom)),
            SmsHardware::MasterSystem1Pal => Self::MasterSystem1Pal(MasterSystem1Pal::new(rom)),
            SmsHardware::MasterSystem2Ntsc => Self::MasterSystem2Ntsc(MasterSystem2Ntsc::new(rom)),
            SmsHardware::MasterSystem2Pal => Self::MasterSystem2Pal(MasterSystem2Pal::new(rom)),
            SmsHardware::MarkIII => Self::MarkIII(MarkIII::new(rom)),
            SmsHardware::GameGear => Self::GameGear(GameGear::new(rom)),
        }
    }
    /// Bind the legacy execution variant using the envelope's physical model.
    pub(crate) fn from_state(hardware: SmsHardware, state: SmsState) -> Result<Self, String> {
        match hardware {
            SmsHardware::MasterSystemNtsc => {
                MasterSystemNtsc::from_state(state).map(Self::MasterSystemNtsc)
            }
            SmsHardware::MasterSystemPal => {
                MasterSystemPal::from_state(state).map(Self::MasterSystemPal)
            }
            SmsHardware::MasterSystemJapan => {
                MasterSystemJapan::from_state(state).map(Self::MasterSystemJapan)
            }
            SmsHardware::MasterSystem1Ntsc => {
                MasterSystem1Ntsc::from_state(state).map(Self::MasterSystem1Ntsc)
            }
            SmsHardware::MasterSystem1Pal => {
                MasterSystem1Pal::from_state(state).map(Self::MasterSystem1Pal)
            }
            SmsHardware::MasterSystem2Ntsc => {
                MasterSystem2Ntsc::from_state(state).map(Self::MasterSystem2Ntsc)
            }
            SmsHardware::MasterSystem2Pal => {
                MasterSystem2Pal::from_state(state).map(Self::MasterSystem2Pal)
            }
            SmsHardware::MarkIII => MarkIII::from_state(state).map(Self::MarkIII),
            SmsHardware::GameGear => GameGear::from_state(state).map(Self::GameGear),
        }
    }
    /// Active physical board.
    pub const fn hardware(&self) -> SmsHardware {
        dispatch!(self,m=>m.hardware())
    }
    /// Delegate to the active concrete board.
    pub fn attach_virtual_uart(&mut self, tx: Sender<u8>, rx: Receiver<u8>) {
        dispatch!(self,m=>m.attach_virtual_uart(tx, rx))
    }
    /// Delegate to the active concrete board.
    pub fn set_fm_unit(&mut self, unit: FmUnit) {
        dispatch!(self,m=>m.set_fm_unit(unit))
    }
    /// Delegate to the active concrete board.
    pub fn fm_unit(&self) -> FmUnit {
        dispatch!(self,m=>m.fm_unit())
    }
    /// Delegate to the active concrete board.
    pub fn ym2413(&self) -> Option<&Ym2413> {
        dispatch!(self,m=>m.ym2413())
    }
    /// Delegate to the active concrete board.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Delegate to the active concrete board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the active concrete board.
    pub fn set_light_phaser_aim(&mut self, port: u8, aim: Option<(u16, u16)>) {
        dispatch!(self,m=>m.set_light_phaser_aim(port, aim))
    }
    /// Delegate to the active concrete board.
    pub fn light_phaser_aim(&self, port: u8) -> Option<(u16, u16)> {
        dispatch!(self,m=>m.light_phaser_aim(port))
    }
    /// Delegate to the active concrete board.
    pub fn set_light_phaser_trigger(&mut self, port: u8, pressed: bool) {
        dispatch!(self,m=>m.set_light_phaser_trigger(port, pressed))
    }
    /// Delegate to the active concrete board.
    pub fn read_controller_port(&self, port: u8) -> u8 {
        dispatch!(self,m=>m.read_controller_port(port))
    }
    /// Delegate to the active concrete board.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the active concrete board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Delegate to the active concrete board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Delegate to the active concrete board.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    /// Delegate to the active concrete board.
    pub fn set_port_dc(&mut self, value: u8) {
        dispatch!(self,m=>m.set_port_dc(value))
    }
    /// Delegate to the active concrete board.
    pub fn set_port_dd(&mut self, value: u8) {
        dispatch!(self,m=>m.set_port_dd(value))
    }
    /// Delegate to the active concrete board.
    pub fn set_gg_start(&mut self, value: u8) {
        dispatch!(self,m=>m.set_gg_start(value))
    }
    /// Delegate to the active concrete board.
    pub fn set_pause_pressed(&mut self, pressed: bool) {
        dispatch!(self,m=>m.set_pause_pressed(pressed))
    }
    /// Delegate to the active concrete board.
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the active concrete board.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Delegate to the active concrete board.
    pub fn vdp(&self) -> &SegaVdp {
        dispatch!(self,m=>m.vdp())
    }
    /// Delegate to the active concrete board.
    pub fn variant(&self) -> SmsVariant {
        dispatch!(self,m=>m.variant())
    }
    /// Delegate to the active concrete board.
    pub fn cartridge_mapper(&self) -> CartridgeMapper {
        dispatch!(self,m=>m.cartridge_mapper())
    }
    /// Delegate to the active concrete board.
    pub fn mapper_regs(&self) -> &[u8; 4] {
        dispatch!(self,m=>m.mapper_regs())
    }
    /// Delegate to the active concrete board.
    pub fn cartridge_ram(&self) -> &[u8] {
        dispatch!(self,m=>m.cartridge_ram())
    }
    /// Delegate to the active concrete board.
    pub const fn cartridge_ram_dirty(&self) -> bool {
        dispatch!(self,m=>m.cartridge_ram_dirty())
    }
    /// Delegate to the active concrete board.
    pub fn restore_cartridge_ram(&mut self, bytes: &[u8], dirty: bool) -> bool {
        dispatch!(self,m=>m.restore_cartridge_ram(bytes, dirty))
    }
    /// Delegate to the active concrete board.
    pub fn cartridge_header(&self) -> Option<&CartridgeHeader> {
        dispatch!(self,m=>m.cartridge_header())
    }
    /// Delegate to the active concrete board.
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self,m=>m.cpu_tstates())
    }
    /// Delegate to the active concrete board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Delegate to the active concrete board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the active concrete board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    /// Delegate to the active concrete board.
    pub fn start_io_trace(&mut self) {
        dispatch!(self,m=>m.start_io_trace())
    }
    /// Delegate to the active concrete board.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self,m=>m.take_io_trace())
    }
}
impl Serialize for Sms {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}

impl Z80Stepper for Sms {
    fn z80_instructions_retired(&self) -> u64 {
        dispatch!(self,m=>m.z80_instructions_retired())
    }
    fn step_tick(&mut self) {
        dispatch!(self,m=>m.step_tick());
    }
}

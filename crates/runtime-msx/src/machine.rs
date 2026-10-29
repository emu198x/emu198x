//! Family selection among fixed manufacturer boards.
use crate::Model;
use common_msx::*;
use emu198x_zilog_z80::Z80Stepper;
use machine_mitsubishi_ml_f80::Msx as MitsubishiMlF80;
use machine_mitsubishi_ml_fx1::Msx as MitsubishiMlFx1;
use machine_panasonic_fs_a1::Msx as PanasonicFsA1;
use machine_philips_vg_8235_00::Msx as PhilipsVg8235;
use machine_sony_hb_10::Msx as SonyHb10;
use machine_sony_hb_10p::Msx as SonyHb10p;
use machine_sony_hb_f1::Msx as SonyHbF1;
use machine_toshiba_hx_10::Msx as ToshibaHx10;
use machine_yamaha_cx5m::Msx as YamahaCx5m;
/// Concrete physical owners retained by the family runtime.
pub enum MsxMachine {
    SonyHb10p(SonyHb10p),
    ToshibaHx10(ToshibaHx10),
    MitsubishiMlF80(MitsubishiMlF80),
    YamahaCx5m(YamahaCx5m),
    MitsubishiMlFx1(MitsubishiMlFx1),
    SonyHb10(SonyHb10),
    PhilipsVg8235(PhilipsVg8235),
    SonyHbF1(SonyHbF1),
    PanasonicFsA1(PanasonicFsA1),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            MsxMachine::SonyHb10p($m) => $body,
            MsxMachine::ToshibaHx10($m) => $body,
            MsxMachine::MitsubishiMlF80($m) => $body,
            MsxMachine::YamahaCx5m($m) => $body,
            MsxMachine::MitsubishiMlFx1($m) => $body,
            MsxMachine::SonyHb10($m) => $body,
            MsxMachine::PhilipsVg8235($m) => $body,
            MsxMachine::SonyHbF1($m) => $body,
            MsxMachine::PanasonicFsA1($m) => $body,
        }
    };
}
impl MsxMachine {
    /// Select the fixed owner for a retained named profile.
    pub(crate) fn try_new(
        model: Model,
        bios: Vec<u8>,
        sub: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        match model {
            Model::SonyHb10p => SonyHb10p::try_new(bios, sub).map(Self::SonyHb10p),
            Model::ToshibaHx10 => ToshibaHx10::try_new(bios, sub).map(Self::ToshibaHx10),
            Model::MitsubishiMlF80 => {
                MitsubishiMlF80::try_new(bios, sub).map(Self::MitsubishiMlF80)
            }
            Model::YamahaCx5m => YamahaCx5m::try_new(bios, sub).map(Self::YamahaCx5m),
            Model::MitsubishiMlFx1 => {
                MitsubishiMlFx1::try_new(bios, sub).map(Self::MitsubishiMlFx1)
            }
            Model::SonyHb10 => SonyHb10::try_new(bios, sub).map(Self::SonyHb10),
            Model::PhilipsVg8235 => PhilipsVg8235::try_new(bios, sub).map(Self::PhilipsVg8235),
            Model::SonyHbF1 => SonyHbF1::try_new(bios, sub).map(Self::SonyHbF1),
            Model::PanasonicFsA1 => PanasonicFsA1::try_new(bios, sub).map(Self::PanasonicFsA1),
        }
    }
    /// Bind original state using its existing named model ID.
    pub(crate) fn from_state(model: Model, state: MsxState) -> Result<Self, String> {
        match model {
            Model::SonyHb10p => SonyHb10p::from_state(state).map(Self::SonyHb10p),
            Model::ToshibaHx10 => ToshibaHx10::from_state(state).map(Self::ToshibaHx10),
            Model::MitsubishiMlF80 => MitsubishiMlF80::from_state(state).map(Self::MitsubishiMlF80),
            Model::YamahaCx5m => YamahaCx5m::from_state(state).map(Self::YamahaCx5m),
            Model::MitsubishiMlFx1 => MitsubishiMlFx1::from_state(state).map(Self::MitsubishiMlFx1),
            Model::SonyHb10 => SonyHb10::from_state(state).map(Self::SonyHb10),
            Model::PhilipsVg8235 => PhilipsVg8235::from_state(state).map(Self::PhilipsVg8235),
            Model::SonyHbF1 => SonyHbF1::from_state(state).map(Self::SonyHbF1),
            Model::PanasonicFsA1 => PanasonicFsA1::from_state(state).map(Self::PanasonicFsA1),
        }
    }
    /// Fixed physical owner.
    pub const fn hardware(&self) -> MsxHardware {
        dispatch!(self,m=>m.hardware())
    }
    /// Delegate to the active concrete board.
    pub fn install_msx_music(&mut self, slot: SlotAddress, rom: Vec<u8>) {
        dispatch!(self,m=>m.install_msx_music(slot, rom))
    }
    /// Delegate to the active concrete board.
    pub fn msx_music(&self) -> Option<&Ym2413> {
        dispatch!(self,m=>m.msx_music())
    }
    /// Delegate to the active concrete board.
    pub fn cartridge_scc(&self, slot: u8) -> Option<&Scc> {
        dispatch!(self,m=>m.cartridge_scc(slot))
    }
    /// Delegate to the active concrete board.
    pub fn cartridge_ym2413(&self, slot: u8) -> Option<&Ym2413> {
        dispatch!(self,m=>m.cartridge_ym2413(slot))
    }
    /// Delegate to the active concrete board.
    pub fn fm_pac_sram(&self, slot: u8) -> Option<&[u8]> {
        dispatch!(self,m=>m.fm_pac_sram(slot))
    }
    /// Delegate to the active concrete board.
    pub fn fm_pac_sram_mut(&mut self, slot: u8) -> Option<&mut [u8]> {
        dispatch!(self,m=>m.fm_pac_sram_mut(slot))
    }
    /// Delegate to the active concrete board.
    pub fn end_audio_frame(&mut self, out: &mut [f32]) -> usize {
        dispatch!(self,m=>m.end_audio_frame(out))
    }
    /// Delegate to the active concrete board.
    pub fn is_msx2(&self) -> bool {
        dispatch!(self,m=>m.is_msx2())
    }
    /// Delegate to the active concrete board.
    pub fn sub_rom(&self) -> Option<&[u8]> {
        dispatch!(self,m=>m.sub_rom())
    }
    /// Delegate to the active concrete board.
    pub fn mapper_registers(&self) -> Option<[u8; 4]> {
        dispatch!(self,m=>m.mapper_registers())
    }
    /// Delegate to the active concrete board.
    pub fn rtc(&self) -> Option<&Rp5c01> {
        dispatch!(self,m=>m.rtc())
    }
    /// Delegate to the active concrete board.
    pub fn rtc_mut(&mut self) -> Option<&mut Rp5c01> {
        dispatch!(self,m=>m.rtc_mut())
    }
    /// Delegate to the active concrete board.
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
    /// Delegate to the active concrete board.
    pub fn joystick_byte(&self, port: u8) -> u8 {
        dispatch!(self,m=>m.joystick_byte(port))
    }
    /// Delegate to the active concrete board.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        dispatch!(self,m=>m.insert_cart1(rom, mapper))
    }
    /// Delegate to the active concrete board.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        dispatch!(self,m=>m.insert_cart2(rom, mapper))
    }
    /// Delegate to the active concrete board.
    pub fn bios_rom(&self) -> &[u8] {
        dispatch!(self,m=>m.bios_rom())
    }
    /// Delegate to the active concrete board.
    pub fn cartridge(&self, slot: u8) -> Option<(&[u8], MapperType)> {
        dispatch!(self,m=>m.cartridge(slot))
    }
    /// Delegate to the active concrete board.
    pub fn cart1_mapper(&self) -> MapperType {
        dispatch!(self,m=>m.cart1_mapper())
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
    pub fn layout(&self) -> &MsxLayout {
        dispatch!(self,m=>m.layout())
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
    pub fn press_key(&mut self, row: usize, bit: u8) {
        dispatch!(self,m=>m.press_key(row, bit))
    }
    /// Delegate to the active concrete board.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        dispatch!(self,m=>m.release_key(row, bit))
    }
    /// Delegate to the active concrete board.
    pub fn keyboard_mut(&mut self) -> &mut [u8; 11] {
        dispatch!(self,m=>m.keyboard_mut())
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
    pub fn tms9918(&self) -> Option<&Tms9918> {
        dispatch!(self,m=>m.tms9918())
    }
    /// Delegate to the active concrete board.
    pub fn v9938(&self) -> Option<&V9938> {
        dispatch!(self,m=>m.v9938())
    }
    /// Delegate to the active concrete board.
    pub fn vdp_scanline(&self) -> u16 {
        dispatch!(self,m=>m.vdp_scanline())
    }
    /// Delegate to the active concrete board.
    pub fn psg(&self) -> &Ay3_8912 {
        dispatch!(self,m=>m.psg())
    }
    /// Delegate to the active concrete board.
    pub fn psg_mut(&mut self) -> &mut Ay3_8912 {
        dispatch!(self,m=>m.psg_mut())
    }
    /// Delegate to the active concrete board.
    pub fn caps_led_on(&self) -> bool {
        dispatch!(self,m=>m.caps_led_on())
    }
    /// Delegate to the active concrete board.
    pub fn cassette_motor_on(&self) -> bool {
        dispatch!(self,m=>m.cassette_motor_on())
    }
    /// Delegate to the active concrete board.
    pub fn cassette_output(&self) -> bool {
        dispatch!(self,m=>m.cassette_output())
    }
    /// Delegate to the active concrete board.
    pub fn key_click_output(&self) -> bool {
        dispatch!(self,m=>m.key_click_output())
    }
    /// Delegate to the active concrete board.
    pub fn ppi(&self) -> &Ppi8255 {
        dispatch!(self,m=>m.ppi())
    }
    /// Delegate to the active concrete board.
    pub fn region(&self) -> MsxRegion {
        dispatch!(self,m=>m.region())
    }
    /// Delegate to the active concrete board.
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self,m=>m.cpu_tstates())
    }
    /// Delegate to the active concrete board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the active concrete board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
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
    /// Delegate to the active concrete board.
    pub fn start_ay_write_watch(&mut self) -> u32 {
        dispatch!(self,m=>m.start_ay_write_watch())
    }
    /// Delegate to the active concrete board.
    pub fn stop_ay_write_watch(&mut self) {
        dispatch!(self,m=>m.stop_ay_write_watch())
    }
    /// Delegate to the active concrete board.
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        dispatch!(self,m=>m.ay_write_watch_records())
    }
    /// Delegate to the active concrete board.
    pub fn clear_ay_write_watch_records(&mut self) {
        dispatch!(self,m=>m.clear_ay_write_watch_records())
    }
}
impl serde::Serialize for MsxMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl Z80Stepper for MsxMachine {
    fn z80_instructions_retired(&self) -> u64 {
        dispatch!(self,m=>m.z80_instructions_retired())
    }
    fn step_tick(&mut self) {
        dispatch!(self,m=>m.step_tick());
    }
}

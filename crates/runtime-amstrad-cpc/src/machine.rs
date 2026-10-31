//! Closed dispatch over the three concrete CPC model owners.
use common_amstrad_cpc::{AmstradCpc as CpcState, CpcModel, *};
/// Concrete board held by the shared CPC runtime.
pub enum AmstradCpc {
    Cpc464(Box<machine_amstrad_cpc464::AmstradCpc>),
    Cpc664(Box<machine_amstrad_cpc664::AmstradCpc>),
    Cpc6128(Box<machine_amstrad_cpc6128::AmstradCpc>),
}
macro_rules! dispatch {
    ($s:expr, $m:ident => $body:expr) => {
        match $s {
            AmstradCpc::Cpc464($m) => $body,
            AmstradCpc::Cpc664($m) => $body,
            AmstradCpc::Cpc6128($m) => $body,
        }
    };
}
impl serde::Serialize for AmstradCpc {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        dispatch!(self, m => serde::Serialize::serialize(m, serializer))
    }
}
impl AmstradCpc {
    pub(crate) fn with_model(firmware: &[u8], model: CpcModel) -> Result<Self, String> {
        match model {
            CpcModel::Cpc464 => {
                machine_amstrad_cpc464::AmstradCpc::new(firmware).map(|m| Self::Cpc464(Box::new(m)))
            }
            CpcModel::Cpc664 => {
                machine_amstrad_cpc664::AmstradCpc::new(firmware).map(|m| Self::Cpc664(Box::new(m)))
            }
            CpcModel::Cpc6128 => machine_amstrad_cpc6128::AmstradCpc::new(firmware)
                .map(|m| Self::Cpc6128(Box::new(m))),
        }
    }
    pub(crate) fn from_state(state: CpcState, model: CpcModel) -> Result<Self, String> {
        match model {
            CpcModel::Cpc464 => machine_amstrad_cpc464::AmstradCpc::from_state(state)
                .map(|m| Self::Cpc464(Box::new(m))),
            CpcModel::Cpc664 => machine_amstrad_cpc664::AmstradCpc::from_state(state)
                .map(|m| Self::Cpc664(Box::new(m))),
            CpcModel::Cpc6128 => machine_amstrad_cpc6128::AmstradCpc::from_state(state)
                .map(|m| Self::Cpc6128(Box::new(m))),
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self, m => m.board_id())
    }
    /// Step the selected board's CPU.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self, m => m.step_instruction())
    }
    /// Delegate to the selected physical owner.
    pub fn firmware(&self) -> Vec<u8> {
        dispatch!(self, m => m.firmware())
    }
    /// Delegate to the selected physical owner.
    pub fn upper_roms(&self) -> &std::collections::BTreeMap<u8, Vec<u8>> {
        dispatch!(self, m => m.upper_roms())
    }
    /// Delegate to the selected physical owner.
    pub fn cpu_tstates(&self) -> u64 {
        dispatch!(self, m => m.cpu_tstates())
    }
    /// Delegate to the selected physical owner.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self, m => m.frame_count())
    }
    /// Delegate to the selected physical owner.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self, m => m.peek(addr))
    }
    /// Delegate to the selected physical owner.
    pub fn model(&self) -> CpcModel {
        dispatch!(self, m => m.model())
    }
    /// Delegate to the selected physical owner.
    pub fn ram_config(&self) -> u8 {
        dispatch!(self, m => m.ram_config())
    }
    /// Delegate to the selected physical owner.
    pub fn ram_byte(&self, addr: u16) -> u8 {
        dispatch!(self, m => m.ram_byte(addr))
    }
    /// Delegate to the selected physical owner.
    pub fn ram_byte_at(&self, offset: usize) -> u8 {
        dispatch!(self, m => m.ram_byte_at(offset))
    }
    /// Delegate to the selected physical owner.
    pub fn screen_text(&self) -> Vec<String> {
        dispatch!(self, m => m.screen_text())
    }
    /// Delegate to the selected physical owner.
    pub fn screen_base(&self) -> u16 {
        dispatch!(self, m => m.screen_base())
    }
    /// Delegate to the selected physical owner.
    pub fn z80(&self) -> &Z80 {
        dispatch!(self, m => m.z80())
    }
    /// Delegate to the selected physical owner.
    pub fn z80_mut(&mut self) -> &mut Z80 {
        dispatch!(self, m => m.z80_mut())
    }
    /// Delegate to the selected physical owner.
    pub fn advance_tstates(&mut self, tstates: u32) {
        dispatch!(self, m => m.advance_tstates(tstates))
    }
    /// Delegate to the selected physical owner.
    pub fn gate_array(&self) -> &GateArray {
        dispatch!(self, m => m.gate_array())
    }
    /// Delegate to the selected physical owner.
    pub fn crtc(&self) -> &Crtc6845 {
        dispatch!(self, m => m.crtc())
    }
    /// Delegate to the selected physical owner.
    pub fn psg(&self) -> &Ay3_8912 {
        dispatch!(self, m => m.psg())
    }
    /// Delegate to the selected physical owner.
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self, m => m.cpu())
    }
    /// Delegate to the selected physical owner.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self, m => m.cpu_mut())
    }
    /// Delegate to the selected physical owner.
    pub fn ram(&self) -> &[u8] {
        dispatch!(self, m => m.ram())
    }
    /// Delegate to the selected physical owner.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self, m => m.poke(addr, value))
    }
    /// Delegate to the selected physical owner.
    pub fn start_io_trace(&mut self) {
        dispatch!(self, m => m.start_io_trace())
    }
    /// Delegate to the selected physical owner.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self, m => m.take_io_trace())
    }
    /// Delegate to the selected physical owner.
    pub fn out(&mut self, port: u16, value: u8) {
        dispatch!(self, m => m.out(port, value))
    }
    /// Delegate to the selected physical owner.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self, m => m.take_audio_buffer())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self, m => m.framebuffer())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_width())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_height())
    }
    /// Delegate to the selected physical owner.
    pub fn insert_tape(&mut self, spans: Vec<TapeSpan>) {
        dispatch!(self, m => m.insert_tape(spans))
    }
    /// Delegate to the selected physical owner.
    pub fn fit_upper_rom(&mut self, number: u8, image: &[u8]) -> Result<(), String> {
        dispatch!(self, m => m.fit_upper_rom(number, image))
    }
    /// Delegate to the selected physical owner.
    pub fn disk_motor_on(&self) -> bool {
        dispatch!(self, m => m.disk_motor_on())
    }
    /// Delegate to the selected physical owner.
    pub fn insert_disk(&mut self, drive: usize, image: DiskImage, writable: bool) {
        dispatch!(self, m => m.insert_disk(drive, image, writable))
    }
    /// Delegate to the selected physical owner.
    pub fn eject_disk(&mut self, drive: usize) {
        dispatch!(self, m => m.eject_disk(drive))
    }
    /// Delegate to the selected physical owner.
    pub fn disk(&self, drive: usize) -> Option<&DiskImage> {
        dispatch!(self, m => m.disk(drive))
    }
    /// Delegate to the selected physical owner.
    pub fn fdc(&self) -> &Upd765a {
        dispatch!(self, m => m.fdc())
    }
    /// Delegate to the selected physical owner.
    pub fn tape_motor_on(&self) -> bool {
        dispatch!(self, m => m.tape_motor_on())
    }
    /// Delegate to the selected physical owner.
    pub fn tape(&self) -> &TapePlayer {
        dispatch!(self, m => m.tape())
    }
    /// Delegate to the selected physical owner.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        dispatch!(self, m => m.press_key(row, bit))
    }
    /// Delegate to the selected physical owner.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        dispatch!(self, m => m.release_key(row, bit))
    }
    /// Delegate to the selected physical owner.
    pub fn press_char(&mut self, c: char) -> bool {
        dispatch!(self, m => m.press_char(c))
    }
    /// Delegate to the selected physical owner.
    pub fn release_char(&mut self, c: char) {
        dispatch!(self, m => m.release_char(c))
    }
    /// Delegate to the selected physical owner.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self, m => m.run_native_frame())
    }
    /// Delegate to the selected physical owner.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self, m => m.run_frame())
    }
}

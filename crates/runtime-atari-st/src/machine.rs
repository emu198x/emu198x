//! Closed family dispatch over four fixed ST owners.
use common_atari_st::{AtariSt as StState, *};
/// The concrete board held by the shared runtime.
pub enum AtariSt {
    St520Pal(Box<machine_atari_520st_pal::AtariSt>),
    St520Ntsc(Box<machine_atari_520st_ntsc::AtariSt>),
    St1040Pal(Box<machine_atari_1040st_pal::AtariSt>),
    St1040Ntsc(Box<machine_atari_1040st_ntsc::AtariSt>),
}
macro_rules! dispatch {
    ($s:expr, $m:ident => $body:expr) => {
        match $s {
            AtariSt::St520Pal($m) => $body,
            AtariSt::St520Ntsc($m) => $body,
            AtariSt::St1040Pal($m) => $body,
            AtariSt::St1040Ntsc($m) => $body,
        }
    };
}
impl serde::Serialize for AtariSt {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>serde::Serialize::serialize(m,s))
    }
}
impl AtariSt {
    pub(crate) fn new(
        model: Model,
        standard: VideoStandard,
        monitor: Monitor,
        tos: Vec<u8>,
    ) -> Result<Self, RomError> {
        match (model, standard) {
            (Model::St520, VideoStandard::Pal) => {
                machine_atari_520st_pal::AtariSt::new(monitor, tos)
                    .map(|m| Self::St520Pal(Box::new(m)))
            }
            (Model::St520, VideoStandard::Ntsc) => {
                machine_atari_520st_ntsc::AtariSt::new(monitor, tos)
                    .map(|m| Self::St520Ntsc(Box::new(m)))
            }
            (Model::St1040, VideoStandard::Pal) => {
                machine_atari_1040st_pal::AtariSt::new(monitor, tos)
                    .map(|m| Self::St1040Pal(Box::new(m)))
            }
            (Model::St1040, VideoStandard::Ntsc) => {
                machine_atari_1040st_ntsc::AtariSt::new(monitor, tos)
                    .map(|m| Self::St1040Ntsc(Box::new(m)))
            }
        }
    }
    pub(crate) fn from_state(
        state: StState,
        model: crate::Model,
        tos: Vec<u8>,
    ) -> Result<Self, String> {
        match model {
            crate::Model::St520Pal => machine_atari_520st_pal::AtariSt::from_state(state, tos)
                .map(|m| Self::St520Pal(Box::new(m))),
            crate::Model::St520Ntsc => machine_atari_520st_ntsc::AtariSt::from_state(state, tos)
                .map(|m| Self::St520Ntsc(Box::new(m))),
            crate::Model::St1040Pal => machine_atari_1040st_pal::AtariSt::from_state(state, tos)
                .map(|m| Self::St1040Pal(Box::new(m))),
            crate::Model::St1040Ntsc => machine_atari_1040st_ntsc::AtariSt::from_state(state, tos)
                .map(|m| Self::St1040Ntsc(Box::new(m))),
        }
    }
    /// Immutable physical board identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Observe the selected board’s hardware.
    pub fn cpu(&self) -> &Cpu68000 {
        dispatch!(self,m=>m.cpu())
    }
    /// Observe the selected board’s hardware.
    pub fn glue(&self) -> &GlueMmu {
        dispatch!(self,m=>m.glue())
    }
    /// Observe the selected board’s hardware.
    pub fn shifter(&self) -> &Shifter {
        dispatch!(self,m=>m.shifter())
    }
    /// Observe the selected board’s hardware.
    pub fn mfp(&self) -> &Mfp68901 {
        dispatch!(self,m=>m.mfp())
    }
    /// Observe the selected board’s hardware.
    pub fn acia_keyboard(&self) -> &Acia6850 {
        dispatch!(self,m=>m.acia_keyboard())
    }
    /// Observe the selected board’s hardware.
    pub fn acia_midi(&self) -> &Acia6850 {
        dispatch!(self,m=>m.acia_midi())
    }
    /// Observe the selected board’s hardware.
    pub fn psg(&self) -> &Ay3_8910 {
        dispatch!(self,m=>m.psg())
    }
    /// Observe the selected board’s hardware.
    pub fn fdc(&self) -> &Wd1770 {
        dispatch!(self,m=>m.fdc())
    }
    /// Observe the selected board’s hardware.
    pub fn dma(&self) -> &Dma {
        dispatch!(self,m=>m.dma())
    }
    /// Observe the selected board’s hardware.
    pub fn ikbd(&self) -> &Ikbd {
        dispatch!(self,m=>m.ikbd())
    }
    /// Explicit debug/register access to the selected owner.
    pub fn cpu_mut(&mut self) -> &mut Cpu68000 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Explicit debug/register access to the selected owner.
    pub fn write_glue(&mut self, offset: u16, value: u8) -> bool {
        dispatch!(self,m=>m.write_glue(offset, value))
    }
    /// Explicit debug/register access to the selected owner.
    pub fn set_memory_config(&mut self, value: u8) {
        dispatch!(self,m=>m.set_memory_config(value))
    }
    /// Explicit debug/register access to the selected owner.
    pub fn set_resolution(&mut self, value: u8) {
        dispatch!(self,m=>m.set_resolution(value))
    }
    /// Explicit debug/register access to the selected owner.
    pub fn write_palette(&mut self, index: usize, value: u16) {
        dispatch!(self,m=>m.write_palette(index, value))
    }
    /// Explicit debug/register access to the selected owner.
    pub fn select_psg_register(&mut self, value: u8) {
        dispatch!(self,m=>m.select_psg_register(value))
    }
    /// Explicit debug/register access to the selected owner.
    pub fn write_psg_data(&mut self, value: u8) {
        dispatch!(self,m=>m.write_psg_data(value))
    }
    /// Delegate to the selected owner.
    pub fn reset(&mut self) {
        dispatch!(self,m=>m.reset())
    }
    /// Delegate to the selected owner.
    pub fn model(&self) -> Model {
        dispatch!(self,m=>m.model())
    }
    /// Delegate to the selected owner.
    pub fn standard(&self) -> VideoStandard {
        dispatch!(self,m=>m.standard())
    }
    /// Delegate to the selected owner.
    pub fn monitor(&self) -> Monitor {
        dispatch!(self,m=>m.monitor())
    }
    /// Delegate to the selected owner.
    pub fn cpu_hz(&self) -> u64 {
        dispatch!(self,m=>m.cpu_hz())
    }
    /// Delegate to the selected owner.
    pub fn master_hz(&self) -> u64 {
        dispatch!(self,m=>m.master_hz())
    }
    /// Delegate to the selected owner.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        dispatch!(self,m=>m.insert_disk(drive, disk))
    }
    /// Delegate to the selected owner.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        dispatch!(self,m=>m.eject_disk(drive))
    }
    /// Delegate to the selected owner.
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        dispatch!(self,m=>m.disk(drive))
    }
    /// Delegate to the selected owner.
    pub fn cycles(&self) -> u64 {
        dispatch!(self,m=>m.cycles())
    }
    /// Delegate to the selected owner.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the selected owner.
    pub fn tick(&mut self) {
        dispatch!(self,m=>m.tick())
    }
    /// Delegate to the selected owner.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the selected owner.
    pub fn frame_width(&self) -> usize {
        dispatch!(self,m=>m.frame_width())
    }
    /// Delegate to the selected owner.
    pub fn frame_height(&self) -> usize {
        dispatch!(self,m=>m.frame_height())
    }
    /// Delegate to the selected owner.
    pub fn display_origin(&self) -> (usize, usize) {
        dispatch!(self,m=>m.display_origin())
    }
    /// Delegate to the selected owner.
    pub fn take_audio(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio())
    }
    /// Delegate to the selected owner.
    pub fn key(&mut self, scancode: u8, pressed: bool) {
        dispatch!(self,m=>m.key(scancode, pressed))
    }
    /// Delegate to the selected owner.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) {
        dispatch!(self,m=>m.mouse_move(dx, dy))
    }
    /// Delegate to the selected owner.
    pub fn mouse_buttons(&mut self, left: bool, right: bool) {
        dispatch!(self,m=>m.mouse_buttons(left, right))
    }
    /// Delegate to the selected owner.
    pub fn mouse_button(&mut self, left: bool, pressed: bool) {
        dispatch!(self,m=>m.mouse_button(left, pressed))
    }
    /// Delegate to the selected owner.
    pub fn joystick(&mut self, port: usize, stick: Stick) {
        dispatch!(self,m=>m.joystick(port, stick))
    }
    /// Delegate to the selected owner.
    pub fn peek(&self, address: u32) -> u8 {
        dispatch!(self,m=>m.peek(address))
    }
    /// Delegate to the selected owner.
    pub fn poke(&mut self, address: u32, value: u8) {
        dispatch!(self,m=>m.poke(address, value))
    }
    /// Delegate to the selected owner.
    pub fn ram(&self) -> &[u8] {
        dispatch!(self,m=>m.ram())
    }
    /// Delegate to the selected owner.
    pub fn rom(&self) -> &[u8] {
        dispatch!(self,m=>m.rom())
    }
}

impl AtariSt {
    /// Ask the selected TOS for its line-A base without changing live memory.
    pub fn line_a_base(&self) -> Option<u32> {
        dispatch!(self,m=>m.line_a_base())
    }
    /// Consume the next keyboard-controller protocol byte.
    pub fn take_ikbd_output(&mut self) -> Option<u8> {
        dispatch!(self,m=>m.take_ikbd_output())
    }
}

impl AtariSt {
    /// Read keyboard ACIA status or consume received data.
    pub fn read_keyboard_acia(&mut self, data: bool) -> u8 {
        dispatch!(self,m=>m.read_keyboard_acia(data))
    }
    /// Write keyboard ACIA control or transmit data.
    pub fn write_keyboard_acia(&mut self, data: bool, value: u8) {
        dispatch!(self,m=>m.write_keyboard_acia(data,value))
    }
}

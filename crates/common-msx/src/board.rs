//! Static named-board construction over the original MSX state.
use super::*;
/// Physical named boards; generic standard presets are retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsxHardware {
    SonyHb10p,
    ToshibaHx10,
    MitsubishiMlF80,
    YamahaCx5m,
    MitsubishiMlFx1,
    SonyHb10,
    PhilipsVg8235,
    SonyHbF1,
    PanasonicFsA1,
}
/// Construction policy owned by one manufacturer board crate.
pub trait MsxBoard {
    const HARDWARE: MsxHardware;
    const REGION: MsxRegion;
    const IS_MSX2: bool;
    fn layout() -> MsxLayout;
}
/// Fixed physical owner with unchanged legacy state serialization.
pub struct MsxCore<B: MsxBoard> {
    state: MsxState,
    board: std::marker::PhantomData<B>,
}
impl<B: MsxBoard> Serialize for MsxCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: MsxBoard> MsxCore<B> {
    /// Construct this fixed board with its required firmware kind.
    pub fn try_new(bios: Vec<u8>, sub: Option<Vec<u8>>) -> Result<Self, String> {
        if B::IS_MSX2 != sub.is_some() {
            return Err("SUB-ROM presence does not match this MSX board".into());
        }
        let state = if B::IS_MSX2 {
            MsxState::msx2(
                bios,
                sub.ok_or("MSX2 SUB-ROM missing")?,
                B::REGION,
                B::layout(),
            )
        } else {
            MsxState::with_layout(bios, B::REGION, B::layout())
        };
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Bind original state after validating physical board topology and video.
    pub fn from_state(state: MsxState) -> Result<Self, String> {
        if state.region() != B::REGION
            || state.is_msx2() != B::IS_MSX2
            || state.v9938().is_some() != B::IS_MSX2
            || state.layout() != &B::layout()
        {
            return Err("snapshot topology or video belongs to another MSX board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// This owner's immutable physical identity.
    pub const fn hardware(&self) -> MsxHardware {
        B::HARDWARE
    }
    /// Delegate to the original hardware state.
    pub fn install_msx_music(&mut self, slot: SlotAddress, rom: Vec<u8>) {
        self.state.install_msx_music(slot, rom)
    }
    /// Delegate to the original hardware state.
    pub fn msx_music(&self) -> Option<&Ym2413> {
        self.state.msx_music()
    }
    /// Delegate to the original hardware state.
    pub fn cartridge_scc(&self, slot: u8) -> Option<&Scc> {
        self.state.cartridge_scc(slot)
    }
    /// Delegate to the original hardware state.
    pub fn cartridge_ym2413(&self, slot: u8) -> Option<&Ym2413> {
        self.state.cartridge_ym2413(slot)
    }
    /// Delegate to the original hardware state.
    pub fn fm_pac_sram(&self, slot: u8) -> Option<&[u8]> {
        self.state.fm_pac_sram(slot)
    }
    /// Delegate to the original hardware state.
    pub fn fm_pac_sram_mut(&mut self, slot: u8) -> Option<&mut [u8]> {
        self.state.fm_pac_sram_mut(slot)
    }
    /// Delegate to the original hardware state.
    pub fn end_audio_frame(&mut self, out: &mut [f32]) -> usize {
        self.state.end_audio_frame(out)
    }
    /// Delegate to the original hardware state.
    pub fn is_msx2(&self) -> bool {
        self.state.is_msx2()
    }
    /// Delegate to the original hardware state.
    pub fn sub_rom(&self) -> Option<&[u8]> {
        self.state.sub_rom()
    }
    /// Delegate to the original hardware state.
    pub fn mapper_registers(&self) -> Option<[u8; 4]> {
        self.state.mapper_registers()
    }
    /// Delegate to the original hardware state.
    pub fn rtc(&self) -> Option<&Rp5c01> {
        self.state.rtc()
    }
    /// Delegate to the original hardware state.
    pub fn rtc_mut(&mut self) -> Option<&mut Rp5c01> {
        self.state.rtc_mut()
    }
    /// Delegate to the original hardware state.
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        self.state.set_joystick(port, up, down, left, right, fire)
    }
    /// Delegate to the original hardware state.
    pub fn joystick_byte(&self, port: u8) -> u8 {
        self.state.joystick_byte(port)
    }
    /// Delegate to the original hardware state.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.state.insert_cart1(rom, mapper)
    }
    /// Delegate to the original hardware state.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.state.insert_cart2(rom, mapper)
    }
    /// Delegate to the original hardware state.
    pub fn bios_rom(&self) -> &[u8] {
        self.state.bios_rom()
    }
    /// Delegate to the original hardware state.
    pub fn cartridge(&self, slot: u8) -> Option<(&[u8], MapperType)> {
        self.state.cartridge(slot)
    }
    /// Delegate to the original hardware state.
    pub fn cart1_mapper(&self) -> MapperType {
        self.state.cart1_mapper()
    }
    /// Delegate to the original hardware state.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to the original hardware state.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to the original hardware state.
    pub fn layout(&self) -> &MsxLayout {
        self.state.layout()
    }
    /// Delegate to the original hardware state.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to the original hardware state.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to the original hardware state.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to the original hardware state.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        self.state.press_key(row, bit)
    }
    /// Delegate to the original hardware state.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        self.state.release_key(row, bit)
    }
    /// Delegate to the original hardware state.
    pub fn keyboard_mut(&mut self) -> &mut [u8; 11] {
        self.state.keyboard_mut()
    }
    /// Delegate to the original hardware state.
    pub fn cpu(&self) -> &Z80 {
        self.state.cpu()
    }
    /// Delegate to the original hardware state.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.state.cpu_mut()
    }
    /// Delegate to the original hardware state.
    pub fn tms9918(&self) -> Option<&Tms9918> {
        self.state.tms9918()
    }
    /// Delegate to the original hardware state.
    pub fn v9938(&self) -> Option<&V9938> {
        self.state.v9938()
    }
    /// Delegate to the original hardware state.
    pub fn vdp_scanline(&self) -> u16 {
        self.state.vdp_scanline()
    }
    /// Delegate to the original hardware state.
    pub fn psg(&self) -> &Ay3_8912 {
        self.state.psg()
    }
    /// Delegate to the original hardware state.
    pub fn psg_mut(&mut self) -> &mut Ay3_8912 {
        self.state.psg_mut()
    }
    /// Delegate to the original hardware state.
    pub fn caps_led_on(&self) -> bool {
        self.state.caps_led_on()
    }
    /// Delegate to the original hardware state.
    pub fn cassette_motor_on(&self) -> bool {
        self.state.cassette_motor_on()
    }
    /// Delegate to the original hardware state.
    pub fn cassette_output(&self) -> bool {
        self.state.cassette_output()
    }
    /// Delegate to the original hardware state.
    pub fn key_click_output(&self) -> bool {
        self.state.key_click_output()
    }
    /// Delegate to the original hardware state.
    pub fn ppi(&self) -> &Ppi8255 {
        self.state.ppi()
    }
    /// Delegate to the original hardware state.
    pub fn region(&self) -> MsxRegion {
        self.state.region()
    }
    /// Delegate to the original hardware state.
    pub fn cpu_tstates(&self) -> u64 {
        self.state.cpu_tstates()
    }
    /// Delegate to the original hardware state.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to the original hardware state.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to the original hardware state.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to the original hardware state.
    pub fn start_io_trace(&mut self) {
        self.state.start_io_trace()
    }
    /// Delegate to the original hardware state.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.take_io_trace()
    }
    /// Delegate to the original hardware state.
    pub fn start_ay_write_watch(&mut self) -> u32 {
        self.state.start_ay_write_watch()
    }
    /// Delegate to the original hardware state.
    pub fn stop_ay_write_watch(&mut self) {
        self.state.stop_ay_write_watch()
    }
    /// Delegate to the original hardware state.
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.state.ay_write_watch_records()
    }
    /// Delegate to the original hardware state.
    pub fn clear_ay_write_watch_records(&mut self) {
        self.state.clear_ay_write_watch_records()
    }
}
impl<B: MsxBoard> emu198x_zilog_z80::Z80Stepper for MsxCore<B> {
    fn z80_instructions_retired(&self) -> u64 {
        emu198x_zilog_z80::Z80Stepper::z80_instructions_retired(&self.state)
    }
    fn step_tick(&mut self) {
        emu198x_zilog_z80::Z80Stepper::step_tick(&mut self.state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestBoard;
    impl MsxBoard for TestBoard {
        const HARDWARE: MsxHardware = MsxHardware::SonyHbF1;
        const REGION: MsxRegion = MsxRegion::Ntsc;
        const IS_MSX2: bool = true;
        fn layout() -> MsxLayout {
            MsxLayout {
                sub_rom: Some(SlotAddress::expanded(3, 0)),
                ram: vec![RamRegion {
                    slot: SlotAddress::expanded(3, 3),
                    start: 0,
                    end: 0xFFFF,
                }],
                expanded: [false, false, false, true],
                ..MsxLayout::default()
            }
        }
    }
    #[test]
    fn binding_checks_the_video_chip_as_well_as_msx2_parts() {
        let mut state = MsxState::msx2(
            vec![0; 32768],
            vec![0; 16384],
            MsxRegion::Ntsc,
            TestBoard::layout(),
        );
        state.video = Video::Tms9918(Box::new(Tms9918::new(VdpRegion::Ntsc)));
        assert!(MsxCore::<TestBoard>::from_state(state).is_err());
    }
}

//! Fixed configuration binding over the original family engine.
use super::*;
use std::marker::PhantomData;
/// Configuration fixed by a machine owner.
pub trait Board: Clone + 'static {
    const MODEL: C128Model;
    const MACHINE_ID: &'static str;
}
/// State bound to one board; the original engine is never replaceable by callers.
#[derive(Clone)]
pub struct C128Core<B: Board> {
    inner: C128,
    board: PhantomData<B>,
}
impl<B: Board> C128Core<B> {
    /// Construct with the fixed VIC-IIe and oscillator selection.
    pub fn new(roms: &C128Roms<'_>) -> Result<Self, RomError> {
        Ok(Self {
            inner: C128::with_model(roms, B::MODEL)?,
            board: PhantomData,
        })
    }
    /// Bind legacy bytes to this board, then rehydrate the Z80 walker.
    pub fn from_state(mut state: C128) -> Result<Self, String> {
        if state.model != B::MODEL
            || state.vic.model() != B::MODEL.vic_model()
            || state.memory.ram.len() != RAM_SIZE
            || state.memory.colour.len() != 0x800
            || state.cia1.model() != CiaModel::Mos6526A
            || state.cia2.model() != CiaModel::Mos6526A
            || state.sid.model != emu198x_mos_sid_6581::SidModel::Mos6581
            || state.vdc_framebuffer().len() != C128::VDC_WIDTH * C128::VDC_HEIGHT
            || state.vic.framebuffer().len() != Vic::new(B::MODEL.vic_model()).framebuffer().len()
        {
            return Err("C128 snapshot board or memory layout does not match fixed owner".into());
        }
        state.finish_restore();
        Ok(Self {
            inner: state,
            board: PhantomData,
        })
    }
    /// Original state without a new enum tag.
    pub fn snapshot_state(&self) -> C128 {
        self.inner.clone()
    }
    /// Forward the original `C128::model` operation.
    pub fn model(&self) -> C128Model {
        self.inner.model()
    }
    /// Forward the original `C128::timing` operation.
    pub fn timing(&self) -> C64Timing {
        self.inner.timing()
    }
    /// Forward the original `C128::cpu` operation.
    pub fn cpu(&self) -> &M6502 {
        self.inner.cpu()
    }
    /// Forward the original `C128::cpu_mut` operation.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        self.inner.cpu_mut()
    }
    /// Forward the original `C128::vic` operation.
    pub fn vic(&self) -> &Vic {
        self.inner.vic()
    }
    /// Forward the original `C128::cia1` operation.
    pub fn cia1(&self) -> &Cia6526 {
        self.inner.cia1()
    }
    /// Forward the original `C128::cia2` operation.
    pub fn cia2(&self) -> &Cia6526 {
        self.inner.cia2()
    }
    /// Forward the original `C128::vic_register` operation.
    pub fn vic_register(&self, index: u8) -> u8 {
        self.inner.vic_register(index)
    }
    /// Forward the original `C128::raster_line` operation.
    pub fn raster_line(&self) -> u16 {
        self.inner.raster_line()
    }
    /// Forward the original `C128::cycle_in_line` operation.
    pub fn cycle_in_line(&self) -> u8 {
        self.inner.cycle_in_line()
    }
    /// Forward the original `C128::audio_sample_rate` operation.
    pub fn audio_sample_rate(&self) -> u32 {
        self.inner.audio_sample_rate()
    }
    /// Forward the original `C128::step_instruction` operation.
    pub fn step_instruction(&mut self) -> u64 {
        self.inner.step_instruction()
    }
    /// Forward the original `C128::z80` operation.
    pub fn z80(&self) -> &Z80 {
        self.inner.z80()
    }
    /// Forward the original `C128::running_8502` operation.
    pub fn running_8502(&self) -> bool {
        self.inner.running_8502()
    }
    /// Forward the original `C128::keyboard_mut` operation.
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        self.inner.keyboard_mut()
    }
    /// Forward the original `C128::set_80_column_key` operation.
    pub fn set_80_column_key(&mut self, down: bool) {
        self.inner.set_80_column_key(down)
    }
    /// Forward the original `C128::framebuffer` operation.
    pub fn framebuffer(&self) -> &[u32] {
        self.inner.framebuffer()
    }
    /// Forward the original `C128::vdc_framebuffer` operation.
    pub fn vdc_framebuffer(&self) -> &[u32] {
        self.inner.vdc_framebuffer()
    }
    /// Forward the original `C128::vdc_ram` operation.
    pub fn vdc_ram(&self) -> &[u8] {
        self.inner.vdc_ram()
    }
    /// Forward the original `C128::take_audio_buffer` operation.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.inner.take_audio_buffer()
    }
    /// Forward the original `C128::phi2_cycles` operation.
    pub fn phi2_cycles(&self) -> u64 {
        self.inner.phi2_cycles()
    }
    /// Forward the original `C128::frame_count` operation.
    pub fn frame_count(&self) -> u64 {
        self.inner.frame_count()
    }
    /// Forward the original `C128::ram` operation.
    pub fn ram(&self, address: u32) -> u8 {
        self.inner.ram(address)
    }
    /// Forward the original `C128::peek` operation.
    pub fn peek(&self, addr: u16) -> u8 {
        self.inner.peek(addr)
    }
    /// Forward the original `C128::poke` operation.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.inner.poke(addr, value)
    }
    /// Forward the original `C128::load_prg` operation.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        self.inner.load_prg(data)
    }
    /// Forward the original `C128::screen_text` operation.
    pub fn screen_text(&self) -> Vec<String> {
        self.inner.screen_text()
    }
    /// Forward the original `C128::run_frame` operation.
    pub fn run_frame(&mut self) -> u64 {
        self.inner.run_frame()
    }
    /// Forward the original `C128::tick` operation.
    pub fn tick(&mut self) -> bool {
        self.inner.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Pal;
    impl Board for Pal {
        const MODEL: C128Model = C128Model::Pal;
        const MACHINE_ID: &'static str = "commodore-c128-pal";
    }
    #[test]
    fn matching_model_does_not_hide_foreign_chips_or_memory_geometry() {
        let raw = C128::new(&C128Roms {
            kernal: &[0; 0x4000],
            basic_lo: &[0; 0x4000],
            basic_hi: &[0; 0x4000],
            chargen: &[0; 0x2000],
        })
        .expect("ROMs");
        assert!(C128Core::<Pal>::from_state(raw.clone()).is_ok());
        for mutation in 0..7 {
            let mut forged = raw.clone();
            match mutation {
                0 => forged.model = C128Model::Ntsc,
                1 => forged.vic = Vic::new(VicModel::Ntsc8564),
                2 => forged.cia1.set_model(CiaModel::Mos6526),
                3 => forged.cia2.set_model(CiaModel::Mos6526),
                4 => forged.sid.model = emu198x_mos_sid_6581::SidModel::Mos8580,
                5 => {
                    forged.memory.ram.pop();
                }
                _ => {
                    forged.memory.colour.pop();
                }
            }
            assert!(
                C128Core::<Pal>::from_state(forged).is_err(),
                "mutation {mutation}"
            );
        }
    }
}

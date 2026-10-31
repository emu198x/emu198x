//! Checked fixed-board ownership over the family engine.
use crate::{Board, Pet, PetKey};
use std::{marker::PhantomData, ops::Deref};
/// Implemented by each concrete machine crate.
pub trait Configuration {
    const BOARD: Board;
    const MACHINE_ID: &'static str;
}
/// A machine whose physical configuration cannot be replaced by callers.
pub struct PetCore<C: Configuration> {
    inner: Pet,
    configuration: PhantomData<C>,
}
impl<C: Configuration> PetCore<C> {
    pub fn new(
        kernal: Vec<u8>,
        basic: Vec<u8>,
        editor: Vec<u8>,
        chars: Vec<u8>,
    ) -> Result<Self, String> {
        Self::from_state(Pet::new(kernal, basic, editor, chars, C::BOARD))
    }
    pub fn from_state(inner: Pet) -> Result<Self, String> {
        inner.validate(C::BOARD)?;
        Ok(Self {
            inner,
            configuration: PhantomData,
        })
    }
    pub fn cold_boot(&self) -> Self {
        Self {
            inner: self.inner.cold_boot(),
            configuration: PhantomData,
        }
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.inner.run_native_frame()
    }
    pub fn run_frame(&mut self) -> u64 {
        self.inner.run_frame()
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.inner.take_audio_buffer()
    }
    pub fn press_key(&mut self, key: PetKey) {
        self.inner.press_key(key)
    }
    pub fn release_key(&mut self, key: PetKey) {
        self.inner.release_key(key)
    }
    pub fn release_all_keys(&mut self) {
        self.inner.release_all_keys()
    }
    pub fn cpu_mut(&mut self) -> &mut emu198x_mos_6502::M6502 {
        self.inner.cpu_mut()
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.inner.poke(addr, value)
    }
    pub fn step_instruction(&mut self) -> u64 {
        self.inner.step_instruction()
    }
}
impl<C: Configuration> Deref for PetCore<C> {
    type Target = Pet;
    fn deref(&self) -> &Pet {
        &self.inner
    }
}

//! Fixed regional binding without mutable access to replaceable machine state.
use crate::{Apu, Buttons, Cartridge, Pad, Region, Snes};
use std::{marker::PhantomData, ops::Deref};
/// The immutable machine-owner wiring.
pub trait Configuration: Clone + std::fmt::Debug {
    const REGION: Region;
    const MACHINE_ID: &'static str;
}
/// A fixed regional machine. The original serializer remains on the raw engine.
#[derive(Clone, Debug)]
pub struct SnesCore<C: Configuration> {
    inner: Snes,
    configuration: PhantomData<C>,
}
impl<C: Configuration> SnesCore<C> {
    pub fn new(cart: Cartridge, ipl: Option<[u8; 64]>) -> Self {
        Self {
            inner: Snes::new(cart, C::REGION, ipl),
            configuration: PhantomData,
        }
    }
    pub fn from_state(inner: Snes) -> Result<Self, String> {
        inner.validate_board(C::REGION)?;
        Ok(Self {
            inner,
            configuration: PhantomData,
        })
    }
    /// Replace the inserted cartridge's RAM without replacing board state.
    pub fn load_cartridge_ram(&mut self, bytes: &[u8]) {
        self.inner.cart.load_ram(bytes);
    }
    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.inner.apu
    }
    pub fn pads_mut(&mut self) -> &mut [Pad; 2] {
        &mut self.inner.pads
    }
    pub fn tick(&mut self) {
        self.inner.tick()
    }
    pub fn reset(&mut self) {
        self.inner.reset()
    }
    pub fn run_frame(&mut self) -> u64 {
        self.inner.run_frame()
    }
    pub fn take_audio(&mut self) -> Vec<f32> {
        self.inner.take_audio()
    }
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        self.inner.set_buttons(port, buttons)
    }
    pub fn poke(&mut self, address: u32, value: u8) {
        self.inner.poke(address, value)
    }
    pub fn step_instruction(&mut self, limit: u64) -> u64 {
        self.inner.step_instruction(limit)
    }
}
impl<C: Configuration> Deref for SnesCore<C> {
    type Target = Snes;
    fn deref(&self) -> &Snes {
        &self.inner
    }
}

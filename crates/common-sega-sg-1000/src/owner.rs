//! Fixed board bindings over the unchanged serialized engine.
use crate::{ControllerState, IoEvent, Sg1000, Sg1000Region};
use emu198x_zilog_z80::{Z80, Z80Stepper};
use std::{marker::PhantomData, ops::Deref};
pub trait Configuration {
    const REGION: Sg1000Region;
    const MACHINE_ID: &'static str;
}
pub struct Sg1000Core<C: Configuration> {
    inner: Sg1000,
    configuration: PhantomData<C>,
}
impl<C: Configuration> Sg1000Core<C> {
    pub fn machine_id(&self) -> &'static str {
        C::MACHINE_ID
    }
    pub fn new(cart: Vec<u8>) -> Self {
        Self {
            inner: Sg1000::new(cart, C::REGION),
            configuration: PhantomData,
        }
    }
    pub fn from_state(mut inner: Sg1000) -> Result<Self, String> {
        inner.validate_board(C::REGION)?;
        inner.cpu_mut().rehydrate_walker_sequence();
        Ok(Self {
            inner,
            configuration: PhantomData,
        })
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
    pub fn controller1_mut(&mut self) -> &mut ControllerState {
        self.inner.controller1_mut()
    }
    pub fn controller2_mut(&mut self) -> &mut ControllerState {
        self.inner.controller2_mut()
    }
    pub fn set_pause_pressed(&mut self, pressed: bool) {
        self.inner.set_pause_pressed(pressed)
    }
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.inner.cpu_mut()
    }
    pub fn poke(&mut self, address: u16, value: u8) {
        self.inner.poke(address, value)
    }
    pub fn start_io_trace(&mut self) {
        self.inner.start_io_trace()
    }
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.inner.take_io_trace()
    }
}
impl<C: Configuration> Deref for Sg1000Core<C> {
    type Target = Sg1000;
    fn deref(&self) -> &Sg1000 {
        &self.inner
    }
}
impl<C: Configuration> Z80Stepper for Sg1000Core<C> {
    fn z80_instructions_retired(&self) -> u64 {
        self.inner.z80_instructions_retired()
    }
    fn step_tick(&mut self) {
        self.inner.step_tick()
    }
}

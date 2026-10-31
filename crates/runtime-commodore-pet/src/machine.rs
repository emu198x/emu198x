//! Closed dispatch for the three fixed PET board owners.
use crate::profiles::Model;
use common_commodore_pet::{Pet as State, PetKey};
/// Live machine with a checked board owner.
pub enum Pet {
    Pet2001n32k(machine_commodore_pet_2001n_32k::Pet),
    Pet4032(machine_commodore_pet_4032::Pet),
    Pet8032(machine_commodore_pet_8032::Pet),
}
impl Pet {
    pub(crate) fn new(
        model: Model,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        editor: Vec<u8>,
        chars: Vec<u8>,
    ) -> Result<Self, String> {
        Self::from_state(
            model,
            State::new(kernal, basic, editor, chars, model.board()),
        )
    }
    pub(crate) fn from_state(model: Model, state: State) -> Result<Self, String> {
        Ok(match model {
            Model::Pet2001n32k => {
                Self::Pet2001n32k(machine_commodore_pet_2001n_32k::Pet::from_state(state)?)
            }
            Model::Pet40Col => Self::Pet4032(machine_commodore_pet_4032::Pet::from_state(state)?),
            Model::Pet80Col => Self::Pet8032(machine_commodore_pet_8032::Pet::from_state(state)?),
        })
    }
    pub(crate) fn state(&self) -> &State {
        match self {
            Self::Pet2001n32k(m) => m,
            Self::Pet4032(m) => m,
            Self::Pet8032(m) => m,
        }
    }
    pub fn cold_boot(&self) -> Self {
        match self {
            Self::Pet2001n32k(m) => Self::Pet2001n32k(m.cold_boot()),
            Self::Pet4032(m) => Self::Pet4032(m.cold_boot()),
            Self::Pet8032(m) => Self::Pet8032(m.cold_boot()),
        }
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        match self {
            Self::Pet2001n32k(m) => m.run_native_frame(),
            Self::Pet4032(m) => m.run_native_frame(),
            Self::Pet8032(m) => m.run_native_frame(),
        }
    }
    pub fn run_frame(&mut self) -> u64 {
        match self {
            Self::Pet2001n32k(m) => m.run_frame(),
            Self::Pet4032(m) => m.run_frame(),
            Self::Pet8032(m) => m.run_frame(),
        }
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        match self {
            Self::Pet2001n32k(m) => m.take_audio_buffer(),
            Self::Pet4032(m) => m.take_audio_buffer(),
            Self::Pet8032(m) => m.take_audio_buffer(),
        }
    }
    pub fn press_key(&mut self, key: PetKey) {
        match self {
            Self::Pet2001n32k(m) => m.press_key(key),
            Self::Pet4032(m) => m.press_key(key),
            Self::Pet8032(m) => m.press_key(key),
        }
    }
    pub fn release_key(&mut self, key: PetKey) {
        match self {
            Self::Pet2001n32k(m) => m.release_key(key),
            Self::Pet4032(m) => m.release_key(key),
            Self::Pet8032(m) => m.release_key(key),
        }
    }
    pub fn release_all_keys(&mut self) {
        match self {
            Self::Pet2001n32k(m) => m.release_all_keys(),
            Self::Pet4032(m) => m.release_all_keys(),
            Self::Pet8032(m) => m.release_all_keys(),
        }
    }
    pub fn cpu_mut(&mut self) -> &mut common_commodore_pet::M6502 {
        match self {
            Self::Pet2001n32k(m) => m.cpu_mut(),
            Self::Pet4032(m) => m.cpu_mut(),
            Self::Pet8032(m) => m.cpu_mut(),
        }
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        match self {
            Self::Pet2001n32k(m) => m.poke(addr, value),
            Self::Pet4032(m) => m.poke(addr, value),
            Self::Pet8032(m) => m.poke(addr, value),
        }
    }
    pub fn step_instruction(&mut self) -> u64 {
        match self {
            Self::Pet2001n32k(m) => m.step_instruction(),
            Self::Pet4032(m) => m.step_instruction(),
            Self::Pet8032(m) => m.step_instruction(),
        }
    }
}
impl std::ops::Deref for Pet {
    type Target = State;
    fn deref(&self) -> &State {
        self.state()
    }
}

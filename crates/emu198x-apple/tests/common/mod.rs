//! A safe Rust client of the C interface, standing in for the Swift host.
//! Every call goes through the exported `extern "C"` functions.

use std::ffi::{CStr, CString};

use emu198x_apple::ffi::{self, Emu198xBuffer};
use emu198x_apple::{Emu198xMachine, status};

/// A failed call: its status and `emu198x_last_error`.
pub type Failure = (i32, String);

pub fn last_error() -> String {
    let message = ffi::emu198x_last_error();
    if message.is_null() {
        return String::new();
    }
    // SAFETY: non-null, so it is the library's NUL-terminated message, valid
    // until the next call on this thread.
    unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

pub fn check(code: i32) -> Result<(), Failure> {
    if code == status::OK {
        Ok(())
    } else {
        Err((code, last_error()))
    }
}

pub fn c_string(text: &str) -> CString {
    CString::new(text).expect("test strings have no NUL")
}

/// Copies a library buffer and frees it.
pub fn take(buffer: Emu198xBuffer) -> Vec<u8> {
    // SAFETY: the library returned this buffer with `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.data, buffer.len) }.to_vec();
    // SAFETY: freed exactly once, and not read afterwards.
    unsafe { ffi::emu198x_buffer_free(buffer) };
    bytes
}

pub fn empty_buffer() -> Emu198xBuffer {
    Emu198xBuffer {
        data: std::ptr::null_mut(),
        len: 0,
    }
}

/// Owns one handle and frees it on drop.
pub struct CMachine(*mut Emu198xMachine);

impl CMachine {
    pub fn create(family: &str, variant: &str) -> Result<Self, Failure> {
        let (family, variant) = (c_string(family), c_string(variant));
        let mut out = std::ptr::null_mut();
        // SAFETY: NUL-terminated strings and a writable out pointer.
        check(unsafe {
            ffi::emu198x_machine_create(family.as_ptr(), variant.as_ptr(), &raw mut out)
        })?;
        Ok(Self(out))
    }

    pub fn raw(&self) -> *mut Emu198xMachine {
        self.0
    }

    pub fn firmware(&mut self, id: &str, data: &[u8]) -> Result<(), Failure> {
        let id = c_string(id);
        // SAFETY: a live handle owned by `self`, a NUL-terminated id and a
        // slice valid for its length.
        check(unsafe {
            ffi::emu198x_machine_firmware(self.0, id.as_ptr(), data.as_ptr(), data.len())
        })
    }

    pub fn boot(&mut self) -> Result<(), Failure> {
        // SAFETY: a live handle owned by `self`.
        check(unsafe { ffi::emu198x_machine_boot(self.0) })
    }

    pub fn step(&mut self) -> Result<(), Failure> {
        // SAFETY: a live handle owned by `self`.
        check(unsafe { ffi::emu198x_machine_step(self.0) })
    }

    pub fn advance(&mut self, elapsed_ms: f64) -> Result<u32, Failure> {
        let mut frames = 0;
        // SAFETY: a live handle owned by `self` and a writable count.
        check(unsafe { ffi::emu198x_machine_advance(self.0, elapsed_ms, &raw mut frames) })?;
        Ok(frames)
    }

    pub fn frame_ms(&mut self) -> Result<f64, Failure> {
        let mut frame_ms = 0.0;
        // SAFETY: a live handle owned by `self` and a writable value.
        check(unsafe { ffi::emu198x_machine_frame_ms(self.0, &raw mut frame_ms) })?;
        Ok(frame_ms)
    }

    pub fn load(&mut self, slot: &str, data: &[u8]) -> Result<(), Failure> {
        let slot = c_string(slot);
        // SAFETY: a live handle owned by `self`, a NUL-terminated slot and a
        // slice valid for its length.
        check(unsafe {
            ffi::emu198x_machine_load(self.0, slot.as_ptr(), data.as_ptr(), data.len())
        })
    }

    pub fn key(&mut self, name: &str, pressed: bool) -> Result<(), Failure> {
        let name = c_string(name);
        // SAFETY: a live handle owned by `self` and a NUL-terminated name.
        check(unsafe { ffi::emu198x_machine_key(self.0, name.as_ptr(), pressed) })
    }

    pub fn button(&mut self, name: &str, pressed: bool) -> Result<(), Failure> {
        let name = c_string(name);
        // SAFETY: a live handle owned by `self` and a NUL-terminated name.
        check(unsafe { ffi::emu198x_machine_button(self.0, name.as_ptr(), pressed) })
    }

    pub fn configure_audio(&mut self, rate: u32) -> Result<(), Failure> {
        // SAFETY: a live handle owned by `self`.
        check(unsafe { ffi::emu198x_machine_configure_audio(self.0, rate) })
    }

    /// Reads at most `capacity` samples.
    pub fn audio(&mut self, capacity: usize) -> Result<Vec<f32>, Failure> {
        let mut samples = vec![0.0; capacity];
        let mut written = 0;
        // SAFETY: a live handle owned by `self`, a buffer of `capacity`
        // floats and a writable count.
        check(unsafe {
            ffi::emu198x_machine_audio(self.0, samples.as_mut_ptr(), capacity, &raw mut written)
        })?;
        samples.truncate(written);
        Ok(samples)
    }

    pub fn frame_size(&mut self) -> Result<(u32, u32), Failure> {
        let (mut width, mut height) = (0, 0);
        // SAFETY: a live handle owned by `self` and two writable values.
        check(unsafe { ffi::emu198x_machine_frame_size(self.0, &raw mut width, &raw mut height) })?;
        Ok((width, height))
    }

    pub fn pixels(&mut self) -> Result<Vec<u8>, Failure> {
        let (width, height) = self.frame_size()?;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let mut written = 0;
        // SAFETY: a live handle owned by `self`, a buffer of its length and
        // a writable count.
        check(unsafe {
            ffi::emu198x_machine_pixels(self.0, pixels.as_mut_ptr(), pixels.len(), &raw mut written)
        })?;
        pixels.truncate(written);
        Ok(pixels)
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, Failure> {
        let mut buffer = empty_buffer();
        // SAFETY: a live handle owned by `self` and a writable buffer.
        check(unsafe { ffi::emu198x_machine_save_state(self.0, &raw mut buffer) })?;
        Ok(take(buffer))
    }

    pub fn restore_state(&mut self, state: &[u8]) -> Result<(), Failure> {
        // SAFETY: a live handle owned by `self` and a slice valid for its
        // length.
        check(unsafe { ffi::emu198x_machine_restore_state(self.0, state.as_ptr(), state.len()) })
    }
}

impl Drop for CMachine {
    fn drop(&mut self) {
        // SAFETY: `self` owns the handle and nothing uses it afterwards.
        unsafe { ffi::emu198x_machine_free(self.0) };
    }
}

/// FNV-1a, enough to tell frames apart.
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

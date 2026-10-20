//! The exported C functions, and the only module allowed `unsafe`.
//!
//! The caller's obligations are in `include/emu198x.h`: pointers are null or
//! valid for the stated length, strings are NUL-terminated UTF-8, and one
//! machine is never used from two threads at once.
//!
//! No function lets a panic unwind into the caller. A panic during a call
//! poisons that machine, because its state may be half-updated; every later
//! call on it returns `POISONED` until it is freed.

use std::any::Any;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use emu198x_fleet_web::catalogue;

use crate::{Emu198xMachine, status};

/// Bytes owned by the library. Release with `emu198x_buffer_free`.
#[repr(C)]
pub struct Emu198xBuffer {
    pub data: *mut u8,
    pub len: usize,
}

impl Emu198xBuffer {
    fn from_vec(bytes: Vec<u8>) -> Self {
        let bytes = bytes.into_boxed_slice();
        let len = bytes.len();
        Self {
            data: Box::into_raw(bytes).cast::<u8>(),
            len,
        }
    }
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(message: &str) {
    let text = CString::new(message.replace('\0', "")).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(text));
}

fn clear_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

/// Why a call failed, before it becomes a status code.
enum Failure {
    Error(String),
    Invalid(&'static str),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Error(message)
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

fn report(failure: Failure) -> i32 {
    match failure {
        Failure::Error(message) => {
            set_error(&message);
            status::ERROR
        }
        Failure::Invalid(message) => {
            set_error(message);
            status::INVALID_ARGUMENT
        }
    }
}

/// Runs a call that has no machine to poison.
fn settle(body: impl FnOnce() -> Result<(), Failure>) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            clear_error();
            status::OK
        }
        Ok(Err(failure)) => report(failure),
        Err(payload) => {
            set_error(&format!("emulator panicked: {}", panic_message(&*payload)));
            status::PANICKED
        }
    }
}

/// Runs a call against a live machine, poisoning it if the call panics.
fn guard(
    machine: &mut Emu198xMachine,
    body: impl FnOnce(&mut Emu198xMachine) -> Result<(), Failure>,
) -> i32 {
    if machine.poisoned {
        set_error("the machine panicked earlier; free it and create another");
        return status::POISONED;
    }
    match catch_unwind(AssertUnwindSafe(|| body(&mut *machine))) {
        Ok(Ok(())) => {
            clear_error();
            status::OK
        }
        Ok(Err(failure)) => report(failure),
        Err(payload) => {
            machine.poisoned = true;
            set_error(&format!("emulator panicked: {}", panic_message(&*payload)));
            status::PANICKED
        }
    }
}

/// # Safety
/// `machine` is null or a live handle from `emu198x_machine_create` that no
/// other thread is using.
unsafe fn with_machine(
    machine: *mut Emu198xMachine,
    body: impl FnOnce(&mut Emu198xMachine) -> Result<(), Failure>,
) -> i32 {
    // SAFETY: the caller guarantees `machine` is null or a live, unshared
    // handle; `as_mut` rejects null.
    match unsafe { machine.as_mut() } {
        Some(machine) => guard(machine, body),
        None => report(Failure::Invalid("machine is null")),
    }
}

/// # Safety
/// `ptr` is null or a NUL-terminated string that outlives `'a`.
unsafe fn text<'a>(ptr: *const c_char, what: &'static str) -> Result<&'a str, Failure> {
    if ptr.is_null() {
        return Err(Failure::Invalid(what));
    }
    // SAFETY: non-null, and the caller guarantees termination and lifetime.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| Failure::Invalid(what))
}

/// # Safety
/// `ptr` is null or valid for reading `len` bytes for `'a`.
unsafe fn bytes<'a>(ptr: *const u8, len: usize, what: &'static str) -> Result<&'a [u8], Failure> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(Failure::Invalid(what));
    }
    // SAFETY: non-null, and the caller guarantees `len` readable bytes.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// # Safety
/// `ptr` is null or valid for writing `len` values of `T` for `'a`.
unsafe fn slots<'a, T>(
    ptr: *mut T,
    len: usize,
    what: &'static str,
) -> Result<&'a mut [T], Failure> {
    if len == 0 {
        return Ok(&mut []);
    }
    if ptr.is_null() {
        return Err(Failure::Invalid(what));
    }
    // SAFETY: non-null, and the caller guarantees `len` writable values.
    Ok(unsafe { std::slice::from_raw_parts_mut(ptr, len) })
}

/// # Safety
/// `ptr` is null or valid for writing one `T`.
unsafe fn put<T>(ptr: *mut T, value: T, what: &'static str) -> Result<(), Failure> {
    if ptr.is_null() {
        return Err(Failure::Invalid(what));
    }
    // SAFETY: non-null, and the caller guarantees it is writable.
    unsafe { ptr.write(value) };
    Ok(())
}

/// Creates an unbooted machine. See `include/emu198x.h`.
///
/// # Safety
/// The strings are NUL-terminated; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_create(
    family: *const c_char,
    variant: *const c_char,
    out: *mut *mut Emu198xMachine,
) -> i32 {
    settle(|| {
        // SAFETY: the caller guarantees `out` is null or writable.
        unsafe { put(out, std::ptr::null_mut(), "out is null") }?;
        // SAFETY: the caller guarantees NUL-terminated strings.
        let family = unsafe { text(family, "family is null or not UTF-8") }?;
        // SAFETY: as above.
        let variant = unsafe { text(variant, "variant is null or not UTF-8") }?;
        let machine = Box::new(Emu198xMachine::new(family, variant)?);
        // SAFETY: checked writable above.
        unsafe { put(out, Box::into_raw(machine), "out is null") }
    })
}

/// Frees a machine. Null is ignored.
///
/// # Safety
/// `machine` is null or a live handle, not used again afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_free(machine: *mut Emu198xMachine) {
    if machine.is_null() {
        return;
    }
    // SAFETY: the caller hands back ownership of a handle this library
    // created with `Box::into_raw`.
    let machine = unsafe { Box::from_raw(machine) };
    let _ = catch_unwind(AssertUnwindSafe(|| drop(machine)));
}

/// Supplies a firmware image before boot.
///
/// # Safety
/// See `with_machine`, `text` and `bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_firmware(
    machine: *mut Emu198xMachine,
    id: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller upholds `with_machine`, `text` and `bytes`.
    unsafe {
        with_machine(machine, |m| {
            let id = text(id, "firmware id is null or not UTF-8")?;
            let data = bytes(data, len, "firmware data is null")?;
            Ok(m.fleet.firmware(id, data)?)
        })
    }
}

/// Builds the runtime from the supplied firmware.
///
/// # Safety
/// See `with_machine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_boot(machine: *mut Emu198xMachine) -> i32 {
    // SAFETY: the caller upholds `with_machine`.
    unsafe { with_machine(machine, |m| Ok(m.fleet.boot()?)) }
}

/// Advances by wall-clock time and reports how many frames ran.
///
/// # Safety
/// See `with_machine` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_advance(
    machine: *mut Emu198xMachine,
    elapsed_ms: f64,
    frames_out: *mut u32,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `put`.
    unsafe {
        with_machine(machine, |m| {
            if !elapsed_ms.is_finite() || elapsed_ms < 0.0 {
                return Err(Failure::Invalid(
                    "elapsed time must be finite and not negative",
                ));
            }
            let frames = m.fleet.advance(elapsed_ms)?;
            put(frames_out, frames, "frames_out is null")
        })
    }
}

/// Runs exactly one frame.
///
/// # Safety
/// See `with_machine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_step(machine: *mut Emu198xMachine) -> i32 {
    // SAFETY: the caller upholds `with_machine`.
    unsafe { with_machine(machine, |m| Ok(m.fleet.step()?)) }
}

/// Loads media into a declared slot.
///
/// # Safety
/// See `with_machine`, `text` and `bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_load(
    machine: *mut Emu198xMachine,
    slot: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller upholds `with_machine`, `text` and `bytes`.
    unsafe {
        with_machine(machine, |m| {
            let slot = text(slot, "slot is null or not UTF-8")?;
            let data = bytes(data, len, "media data is null")?;
            Ok(m.fleet.load(slot, data)?)
        })
    }
}

/// Starts or stops a tape deck.
///
/// # Safety
/// See `with_machine` and `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_transport(
    machine: *mut Emu198xMachine,
    slot: *const c_char,
    playing: bool,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `text`.
    unsafe {
        with_machine(machine, |m| {
            let slot = text(slot, "slot is null or not UTF-8")?;
            Ok(m.fleet.transport(slot, playing)?)
        })
    }
}

/// Presses or releases a machine key.
///
/// # Safety
/// See `with_machine` and `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_key(
    machine: *mut Emu198xMachine,
    name: *const c_char,
    pressed: bool,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `text`.
    unsafe {
        with_machine(machine, |m| {
            let name = text(name, "key name is null or not UTF-8")?;
            Ok(m.fleet.key(name, pressed)?)
        })
    }
}

/// Presses or releases a controller-one button.
///
/// # Safety
/// See `with_machine` and `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_button(
    machine: *mut Emu198xMachine,
    name: *const c_char,
    pressed: bool,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `text`.
    unsafe {
        with_machine(machine, |m| {
            let name = text(name, "button name is null or not UTF-8")?;
            Ok(m.fleet.button(name, pressed)?)
        })
    }
}

/// Turns on stereo sound at `rate` samples per second.
///
/// # Safety
/// See `with_machine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_configure_audio(
    machine: *mut Emu198xMachine,
    rate: u32,
) -> i32 {
    // SAFETY: the caller upholds `with_machine`.
    unsafe { with_machine(machine, |m| Ok(m.configure_audio(rate)?)) }
}

/// Copies queued interleaved stereo samples into `out`.
///
/// # Safety
/// See `with_machine`, `slots` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_audio(
    machine: *mut Emu198xMachine,
    out: *mut f32,
    capacity: usize,
    written_out: *mut usize,
) -> i32 {
    // SAFETY: the caller upholds `with_machine`, `slots` and `put`.
    unsafe {
        with_machine(machine, |m| {
            let out = slots(out, capacity, "audio buffer is null")?;
            let written = m.read_audio(out)?;
            put(written_out, written, "written_out is null")
        })
    }
}

/// Reports the picture size in pixels.
///
/// # Safety
/// See `with_machine` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_frame_size(
    machine: *mut Emu198xMachine,
    width_out: *mut u32,
    height_out: *mut u32,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `put`.
    unsafe {
        with_machine(machine, |m| {
            let (width, height) = (m.fleet.width()?, m.fleet.height()?);
            put(width_out, width, "width_out is null")?;
            put(height_out, height, "height_out is null")
        })
    }
}

/// Copies the latest frame as RGBA bytes. A buffer smaller than the frame is
/// refused rather than filled with part of one.
///
/// # Safety
/// See `with_machine`, `slots` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_pixels(
    machine: *mut Emu198xMachine,
    out: *mut u8,
    capacity: usize,
    written_out: *mut usize,
) -> i32 {
    // SAFETY: the caller upholds `with_machine`, `slots` and `put`.
    unsafe {
        with_machine(machine, |m| {
            let pixels = m.fleet.pixels()?;
            if capacity < pixels.len() {
                return Err(Failure::Invalid("pixel buffer is smaller than the frame"));
            }
            let out = slots(out, capacity, "pixel buffer is null")?;
            out[..pixels.len()].copy_from_slice(&pixels);
            put(written_out, pixels.len(), "written_out is null")
        })
    }
}

/// Reports the machine's frame period in milliseconds.
///
/// # Safety
/// See `with_machine` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_frame_ms(
    machine: *mut Emu198xMachine,
    out: *mut f64,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `put`.
    unsafe {
        with_machine(machine, |m| {
            let frame_ms = m.fleet.frame_ms()?;
            put(out, frame_ms, "out is null")
        })
    }
}

/// Exports the machine state into a library-owned buffer.
///
/// # Safety
/// See `with_machine` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_save_state(
    machine: *mut Emu198xMachine,
    out: *mut Emu198xBuffer,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `put`.
    unsafe {
        with_machine(machine, |m| {
            if out.is_null() {
                return Err(Failure::Invalid("out is null"));
            }
            let state = m.fleet.save_state()?;
            put(out, Emu198xBuffer::from_vec(state), "out is null")
        })
    }
}

/// Restores a state produced by `emu198x_machine_save_state`.
///
/// # Safety
/// See `with_machine` and `bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_restore_state(
    machine: *mut Emu198xMachine,
    data: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `bytes`.
    unsafe {
        with_machine(machine, |m| {
            let data = bytes(data, len, "state data is null")?;
            Ok(m.fleet.restore_state(data)?)
        })
    }
}

/// Exports the browser key-code to machine-key map as UTF-8 JSON.
///
/// # Safety
/// See `with_machine` and `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_machine_keymap(
    machine: *mut Emu198xMachine,
    out: *mut Emu198xBuffer,
) -> i32 {
    // SAFETY: the caller upholds `with_machine` and `put`.
    unsafe {
        with_machine(machine, |m| {
            if out.is_null() {
                return Err(Failure::Invalid("out is null"));
            }
            let keymap = m.fleet.keymap()?;
            put(
                out,
                Emu198xBuffer::from_vec(keymap.into_bytes()),
                "out is null",
            )
        })
    }
}

/// Exports the compiled families and variants as UTF-8 JSON.
///
/// # Safety
/// See `put`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_catalogue(out: *mut Emu198xBuffer) -> i32 {
    settle(|| {
        if out.is_null() {
            return Err(Failure::Invalid("out is null"));
        }
        let entries: Vec<String> = catalogue().iter().map(ToString::to_string).collect();
        let json = format!("[{}]", entries.join(","));
        // SAFETY: the caller guarantees `out` is writable; null is rejected.
        unsafe {
            put(
                out,
                Emu198xBuffer::from_vec(json.into_bytes()),
                "out is null",
            )
        }
    })
}

/// Releases a buffer this library returned. A null buffer is ignored.
///
/// # Safety
/// `buffer` came from this library and has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu198x_buffer_free(buffer: Emu198xBuffer) {
    if buffer.data.is_null() {
        return;
    }
    // SAFETY: the caller hands back a buffer `Emu198xBuffer::from_vec` made
    // from a boxed slice of exactly `len` bytes.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
}

/// The calling thread's last error message, or null after a successful call.
/// Valid until the next call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn emu198x_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |text| text.as_ptr())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Emu198xMachine {
        let mut machine =
            Emu198xMachine::new("nintendo-game-boy", "dmg").expect("compiled variant");
        machine.fleet.boot().expect("needs no firmware");
        machine
    }

    #[test]
    fn a_panic_poisons_the_machine() {
        let mut machine = machine();
        assert_eq!(guard(&mut machine, |_| panic!("probe")), status::PANICKED);
        assert!(machine.poisoned);
        assert_eq!(guard(&mut machine, |_| Ok(())), status::POISONED);
    }

    #[test]
    fn a_panic_without_a_machine_is_contained() {
        assert_eq!(settle(|| panic!("probe")), status::PANICKED);
        assert_eq!(settle(|| Ok(())), status::OK);
    }
}

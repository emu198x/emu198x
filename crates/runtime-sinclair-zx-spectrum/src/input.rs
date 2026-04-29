//! Spectrum keyboard input mapping.
//!
//! Splits the per-event keyboard handling out of `runtime.rs`. The
//! Spectrum's keyboard matrix is a runtime-owned cache that survives
//! across `run_until` calls — the matrix is updated incrementally as
//! events arrive, and `run_until` pushes the cached rows into the
//! machine once before stepping the frame. The runtime is therefore
//! the natural argument for this free function: it owns both the
//! cache and the machine, and Spectrum input is uniform across every
//! variant in the family (each variant's `set_keyboard_rows` is on
//! the `SpectrumMachine` trait).
//!
//! The `<M: SpectrumMachine>` bound threads through so the function
//! works for every `SpectrumRuntime<M>` instantiation.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use emu198x_shell::InputEvent;

use crate::runtime::{SpectrumMachine, SpectrumRuntime};

/// Apply one host input event to the runtime's keyboard matrix.
/// Recognised key names update the matching cell; other event kinds
/// (joystick, mouse, etc.) are ignored. The cached rows are pushed
/// to the machine separately by `run_until` to preserve the original
/// "decode N events, push once" semantics.
pub(crate) fn apply_input_event<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    event: &InputEvent,
) {
    if let InputEvent::Key { name, pressed } = event
        && let Some(key) = SpectrumKey::from_name(name.as_ref())
    {
        runtime.keyboard_mut().set_key(key, *pressed);
    }
}

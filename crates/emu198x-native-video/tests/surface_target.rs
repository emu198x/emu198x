//! The presenter must attach to any wgpu surface target, not only a winit
//! window, because the browser hands it an `HtmlCanvasElement`.
//!
//! Compile-time only: constructing a real presenter needs a GPU adapter and a
//! window, neither of which exists on a CI runner. What is worth pinning is
//! the *shape* of the constructor — that it is generic over the surface target
//! and that the native convenience wrapper still takes a window.

use std::sync::Arc;

use emu198x_native_video::WgpuVideoPresenter;
use winit::window::Window;

/// `new_async` accepts anything wgpu can build a surface from. On the web that
/// is an `HtmlCanvasElement`; here we can only name the native side, but the
/// bound is what the browser build relies on.
#[test]
fn new_async_is_generic_over_the_surface_target() {
    fn accepts<T: Into<wgpu::SurfaceTarget<'static>>>() {}
    accepts::<Arc<Window>>();

    // The signature itself is the assertion: this coerces only if `new_async`
    // is generic over the target and takes an explicit surface size, rather
    // than reading one off a window.
    let _: fn(Arc<Window>, (u32, u32), u32, u32) -> _ =
        WgpuVideoPresenter::new_async::<Arc<Window>>;
}

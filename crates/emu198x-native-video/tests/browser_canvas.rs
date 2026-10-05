//! The presenter must put pixels on a browser canvas, not merely attach to one.
//!
//! ```text
//! wasm-pack test --headless --chrome --release crates/emu198x-native-video
//! ```
//!
//! #1416 verified this crate compile-only for wasm and shipped a presenter
//! that attached to a canvas, returned `Ok` from every call, and left the
//! canvas black (#1436): the browser's WebGPU rejected the shader module, and
//! that error never reached the caller. So this test reads the canvas back. It
//! presents a frame of four known colours and checks each quadrant, which is
//! the only evidence that the picture reached the page.
//!
//! It runs twice: once on whatever backend the browser offers, and once with
//! WebGPU offering no adapter, which must fall back to WebGL2 rather than fail.
//!
//! The readback copies the canvas into a 2-D canvas in the same task as the
//! present. A WebGL canvas without `preserveDrawingBuffer` clears once the
//! browser composites, so a readback after an `await` would see a blank canvas
//! whether the presenter drew or not.

#![cfg(target_arch = "wasm32")]

use emu198x_native_video::{PresentationProfile, ScalingMode, WgpuVideoPresenter, wgpu};
use emu198x_shell::{CapturedFrame, MachineTime, PixelFormat};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

wasm_bindgen_test_configure!(run_in_browser);

/// Canvas size: 2× a 2×2 frame, so each frame pixel covers a 2×2 block.
const CANVAS: u32 = 4;

/// Top-left, top-right, bottom-left, bottom-right, as RGBA.
const QUADRANTS: [[u8; 4]; 4] = [
    [0xFF, 0x00, 0x00, 0xFF],
    [0x00, 0xFF, 0x00, 0xFF],
    [0x00, 0x00, 0xFF, 0xFF],
    [0xFF, 0xFF, 0xFF, 0xFF],
];

fn canvas(width: u32, height: u32) -> HtmlCanvasElement {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("a browser test has a document");
    let canvas = document
        .create_element("canvas")
        .expect("a document can create a canvas")
        .dyn_into::<HtmlCanvasElement>()
        .expect("a canvas element is an HtmlCanvasElement");
    canvas.set_width(width);
    canvas.set_height(height);
    document
        .body()
        .expect("a browser test has a body")
        .append_child(&canvas)
        .expect("the canvas attaches to the page");
    canvas
}

fn frame() -> CapturedFrame {
    CapturedFrame {
        timestamp: MachineTime::new(0),
        format: PixelFormat::Rgba8888,
        width: 2,
        height: 2,
        palette: None,
        pixels: QUADRANTS.concat(),
    }
}

/// Copies `source` into a 2-D canvas and returns its RGBA pixels.
fn read_back(source: &HtmlCanvasElement) -> Vec<u8> {
    let target = canvas(source.width(), source.height());
    let context = target
        .get_context("2d")
        .expect("2-D context request")
        .expect("a fresh canvas has a 2-D context")
        .dyn_into::<CanvasRenderingContext2d>()
        .expect("the context is 2-D");
    context
        .draw_image_with_html_canvas_element(source, 0.0, 0.0)
        .expect("a canvas can be drawn into another");
    context
        .get_image_data(
            0.0,
            0.0,
            f64::from(source.width()),
            f64::from(source.height()),
        )
        .expect("pixels can be read from a same-origin canvas")
        .data()
        .to_vec()
}

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * CANVAS + x) * 4) as usize;
    [
        rgba[offset],
        rgba[offset + 1],
        rgba[offset + 2],
        rgba[offset + 3],
    ]
}

/// Presents the test frame on a fresh canvas and returns the four quadrants.
async fn present_and_read() -> [[u8; 4]; 4] {
    // A fresh canvas each time: one that has handed out a WebGPU context
    // cannot hand out a WebGL2 one.
    let canvas = canvas(CANVAS, CANVAS);
    let mut presenter = WgpuVideoPresenter::new_async(
        wgpu::SurfaceTarget::Canvas(canvas.clone()),
        (CANVAS, CANVAS),
        2,
        2,
    )
    .await
    .expect("the browser offers WebGPU or WebGL2");

    let profile = PresentationProfile {
        scaling: ScalingMode::Stretch,
        ..PresentationProfile::raw()
    };
    presenter
        .present(&frame(), &profile)
        .expect("the frame presents");

    // No await between the present and the readback; see the module doc.
    let rgba = read_back(&canvas);

    // One pixel inside each 2×2 block, at the canvas corners.
    [
        pixel(&rgba, 0, 0),
        pixel(&rgba, 3, 0),
        pixel(&rgba, 0, 3),
        pixel(&rgba, 3, 3),
    ]
}

#[wasm_bindgen_test]
async fn presenting_a_frame_draws_it_on_the_canvas() {
    assert_eq!(
        present_and_read().await,
        QUADRANTS,
        "the canvas shows the presented frame on the browser's own backend"
    );

    // What a browser that exposes `navigator.gpu` but has no adapter does.
    // Patched last, so the first pass above still gets WebGPU where it exists.
    js_sys::Function::new_no_args(
        "if (globalThis.GPU) { GPU.prototype.requestAdapter = () => Promise.resolve(null); }",
    )
    .call0(&wasm_bindgen::JsValue::UNDEFINED)
    .expect("the adapter request can be stubbed");

    assert_eq!(
        present_and_read().await,
        QUADRANTS,
        "with no WebGPU adapter the presenter falls back to WebGL2 and still draws"
    );
}

//! Shared Commodore 64-family timing data.
//!
//! This crate only owns stable hardware facts that the future machine/runtime
//! layers can build on. It does not claim a runnable C64 implementation.

pub mod timing;

pub use timing::{C64Timing, FRAMEBUFFER_WIDTH, TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};

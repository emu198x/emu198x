//! Condition code evaluation for Bcc/DBcc/Scc instructions.
//!
//! This is a thin wrapper around `Status::condition()` for clarity.

pub use super::flags::Status;

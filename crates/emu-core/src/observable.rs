//! Observability trait for inspecting component state.
//!
//! Every emulator component exposes its internal state for education and
//! debugging. Queries never affect emulation state.

use std::collections::HashMap;
use std::fmt;

/// A dynamically-typed value for state queries.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Boolean value.
    Bool(bool),
    /// 8-bit unsigned integer.
    U8(u8),
    /// 16-bit unsigned integer.
    U16(u16),
    /// 32-bit unsigned integer.
    U32(u32),
    /// 64-bit unsigned integer.
    U64(u64),
    /// 8-bit signed integer.
    I8(i8),
    /// String value.
    String(String),
    /// Array of values.
    Array(Vec<Value>),
    /// Map of string keys to values.
    Map(HashMap<String, Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(v) => write!(f, "{v}"),
            Value::U8(v) => write!(f, "{v:#04X}"),
            Value::U16(v) => write!(f, "{v:#06X}"),
            Value::U32(v) => write!(f, "{v:#010X}"),
            Value::U64(v) => write!(f, "{v}"),
            Value::I8(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "{v}"),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<u8> for Value {
    fn from(v: u8) -> Self {
        Value::U8(v)
    }
}

impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Value::U16(v)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::U32(v)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::U64(v)
    }
}

impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Value::I8(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

/// A component whose state can be inspected.
///
/// Observability is a core design goal. At any tick, you can inspect any
/// component. Queries never affect emulation state.
pub trait Observable {
    /// Query a specific property by path.
    ///
    /// Paths are hierarchical, separated by dots:
    /// - `pc` - Program counter
    /// - `a` - Accumulator
    /// - `flags.z` - Zero flag
    ///
    /// Returns `None` if the path is not recognised.
    fn query(&self, path: &str) -> Option<Value>;

    /// List all available query paths.
    ///
    /// Returns paths that can be passed to `query()`.
    fn query_paths(&self) -> &'static [&'static str];
}

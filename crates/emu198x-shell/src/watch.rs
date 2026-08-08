//! Machine-agnostic write-watch access for the shared MCP/script watch tools.
//!
//! A *watch* arms the running machine to record events as the CPU runs —
//! distinct from the synchronous [`crate::debug::DebugTarget`] verbs, which
//! read current state on demand. The captured log is then polled
//! (non-destructively) or dropped.
//!
//! Two watchable surfaces exist today, both behind one trait so the shared
//! tools in [`crate::mcp_tools`] (`watch_memory_*`, `watch_ay_*`) and the
//! `--script` runner execute the identical body on any machine:
//!
//! - **Memory writes** — every observed write inside an address range, as
//!   `(pc, addr, value)`. The Amiga additionally stamps `cck`, write width,
//!   and whether the CPU, blitter, or disk DMA issued the write.
//! - **AY register writes** — every `OUT` to the AY data port, as
//!   `(pc, register, value)`; AY/PSG machines only.
//!
//! A machine surfaces this through
//! [`MachineCore::watch_target`](crate::MachineCore::watch_target). Each
//! surface defaults to unsupported, so a machine opts into whichever it has
//! (the Amiga has memory-write capture but no AY; the Spectrum has both).
//! Addresses are `u32` so the surface spans the 8/16-bit machines (low 16
//! bits) and the 68000 family (24-bit bus) uniformly.

/// Hardware agent responsible for a captured memory write.
///
/// This field is optional on [`WatchMemoryRecord`] so machine families whose
/// watch is intrinsically CPU-only preserve their existing JSON shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchMemorySource {
    /// The active processor issued the write.
    Cpu,
    /// The Amiga blitter's D channel issued the write.
    Blitter,
    /// Agnus transferred a Paula disk read-DMA word into chip RAM.
    DiskDma,
}

/// One captured memory write, widened to the family-wide shape.
///
/// `cck` and `size_bytes` carry the richer 68000 detail the Amiga records;
/// byte-only 8/16-bit machines leave `cck` `None` and `size_bytes` `1`.
#[derive(Debug, Clone, Copy)]
pub struct WatchMemoryRecord {
    /// CPU program counter at the moment of observation. For DMA writes this
    /// is concurrent CPU context rather than the writer's instruction PC.
    pub pc: u32,
    /// Target address of the write.
    pub addr: u32,
    /// Value written. A byte occupies the low 8 bits; a word the low 16.
    pub value: u32,
    /// Colour-clock timestamp of the write, when the machine stamps one
    /// (the Amiga does; the 8/16-bit cores do not).
    pub cck: Option<u64>,
    /// Width of the write in bytes (`1` for a byte store, `2` for a word).
    pub size_bytes: u8,
    /// Hardware agent that issued the write, when the machine distinguishes
    /// writers. CPU-only families leave this absent.
    pub source: Option<WatchMemorySource>,
}

/// One captured AY-3-8910/8912 register write.
#[derive(Debug, Clone, Copy)]
pub struct WatchAyRecord {
    /// CPU program counter at the moment of the write.
    pub pc: u32,
    /// AY register index (0-15) selected at the write.
    pub register: u8,
    /// Byte written to the selected register.
    pub value: u8,
}

/// Why a watch could not be armed.
#[derive(Debug, Clone)]
pub enum WatchError {
    /// The active machine (or variant) has no such watchable surface —
    /// e.g. an AY watch on a 48K Spectrum, or any watch on a core that
    /// has not wired write-capture.
    Unsupported,
    /// The request was malformed for this machine — e.g. an address
    /// outside the CPU's space, or a range too long for it.
    Invalid(String),
}

impl core::fmt::Display for WatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("not supported on this machine"),
            Self::Invalid(msg) => f.write_str(msg),
        }
    }
}

/// Write-watch access to a running machine, behind a trait so the shared
/// watch tools stay machine-agnostic.
///
/// Implementors are the per-system runtimes (the Spectrum family enum and the
/// Amiga family enum today), each delegating to its own capture buffer. Every
/// method defaults to "unsupported / empty", so a machine implements only the
/// surfaces it has and the shell's generic verbs fall back cleanly elsewhere.
pub trait WatchTarget {
    /// Whether this machine can watch memory writes.
    fn supports_memory_watch(&self) -> bool {
        false
    }

    /// Arm a memory-write watch over `[addr, addr + len)`, replacing any
    /// prior range and clearing the captured log. Returns the log capacity
    /// (max records before it stops growing).
    fn start_memory_watch(&mut self, _addr: u32, _len: u32) -> Result<u32, WatchError> {
        Err(WatchError::Unsupported)
    }

    /// Disarm the memory-write watch and drop its log. Returns
    /// `(had_watch, captured)` — whether a range was configured, and how
    /// many records were dropped.
    fn clear_memory_watch(&mut self) -> (bool, u32) {
        (false, 0)
    }

    /// The active memory-watch range `(addr, len)`, or `None` if disarmed.
    fn memory_watch_range(&self) -> Option<(u32, u32)> {
        None
    }

    /// Snapshot of captured memory writes (oldest first), or `None` if the
    /// watch is disarmed.
    fn memory_watch_records(&self) -> Option<Vec<WatchMemoryRecord>> {
        None
    }

    /// Whether this machine can watch AY register writes.
    fn supports_ay_watch(&self) -> bool {
        false
    }

    /// Arm an AY register-write watch, clearing any prior log. Returns the
    /// log capacity.
    fn start_ay_watch(&mut self) -> Result<u32, WatchError> {
        Err(WatchError::Unsupported)
    }

    /// Disarm the AY watch and drop its log. Returns `(had_watch, captured)`.
    fn clear_ay_watch(&mut self) -> (bool, u32) {
        (false, 0)
    }

    /// Snapshot of captured AY writes (oldest first), or `None` if disarmed.
    fn ay_watch_records(&self) -> Option<Vec<WatchAyRecord>> {
        None
    }
}

//! Motorola 68010 CPU — skeleton crate.
//!
//! The 68010 is the first incremental refresh of the 68000 family. It
//! shares the M68000's 4-clock bus cycle, 16-bit ALU, and two-word
//! prefetch pipeline. The user-visible additions are mostly about
//! *exception handling* and *operating-system support* — the changes
//! that let the 68010 host UNIX (Sun-2) and a virtual-memory Amiga
//! (Sun-2-style trap-and-restart bus errors).
//!
//! # Today
//!
//! No machine in this workspace runs a 68010-class part. This crate
//! is an architectural seam: the type alias [`Cpu68010`] resolves to
//! [`motorola_68000::Cpu68000`] today, but the M68000 core no longer
//! contains 68010-specific code paths — those were stripped on
//! 2026-04-29 alongside the all-variants split. When a 68010 machine
//! arrives, this crate gains its own state machine and the alias
//! collapses into a real type.
//!
//! # What a real 68010 implementation needs
//!
//! Adding the 68010 to the workspace means re-introducing every
//! capability listed below to a dedicated `Cpu68010` core. References
//! are to the M68000PRM (Programmer's Reference Manual) and the
//! 68010 User's Manual where they diverge.
//!
//! ## New control registers
//!
//! - **VBR** (Vector Base Register, 32-bit) — relocates the exception
//!   vector table away from $00000000. M68000PRM § 1.2.4. Used by
//!   AmigaOS for ROM-relative vectors and by every modern OS that
//!   wants vectors in writable memory.
//! - **SFC** / **DFC** (Source / Destination Function Code, 3 bits
//!   each) — supply the FC[2:0] pins for `MOVES` accesses, letting
//!   supervisor code reach across address spaces.
//!
//! ## New instructions
//!
//! - **MOVEC** (`$4E7A` / `$4E7B`) — privileged read / write of
//!   control registers (VBR, SFC, DFC). M68000PRM § 6.2.21.
//! - **MOVES** (`$0Exx`) — privileged data move using SFC / DFC.
//!   Lets the kernel poke user space through MMU translation.
//!   M68000PRM § 6.2.23.
//! - **RTD** (`$4E74`) — return and deallocate; pops PC then adds a
//!   sign-extended d16 to SP. M68000PRM § 6.2.32.
//! - **MOVE from CCR** (`$42C0`) — non-privileged read of CCR.
//!   The 68000 has only the privileged MOVE-from-SR; the 68010 splits
//!   them so user code can sample condition codes without a trap.
//! - **BKPT** (`$4848`-`$484F`) — breakpoint acknowledge bus cycle
//!   (or illegal-instruction trap when no debugger is attached).
//!
//! ## Changed instruction behaviour
//!
//! - **MOVE from SR** is now privileged on the 68010 (it was open on
//!   the 68000). User-mode `MOVE SR,Rn` traps via vector 8.
//! - **Loop mode** — `DBcc` taken-branch fast path. When a one-word
//!   instruction sits in IR and the prefetched word in IRC is the
//!   `DBcc` itself, the 68010 stays in a tight micro-coded loop
//!   without re-fetching either word. Visible only as a timing
//!   improvement; semantically transparent. M68010UM § 7.2.
//!
//! ## Exception frames
//!
//! - **6-word stack frame** (vs the 68000's 4-word frame) — adds a
//!   format / vector word at the top of every exception frame.
//!   `RTE` reads this format word and pops the right number of words
//!   based on the format code:
//!
//!   - Format `$0`: 4-word frame (group-1/2 short).
//!   - Format `$1`: throwaway frame (interrupt return without
//!     instruction-restart).
//!   - Format `$8`: 29-word *bus-error* frame (group-0). The 68010's
//!     instruction-continuation model means the bus-error exception
//!     handler can patch the fault and `RTE` resumes mid-instruction.
//!     This is the feature that made the Sun-2 / SunOS feasible.
//!
//! # Today's wrapper
//!
//! [`Cpu68010`] is a thin wrapper around [`motorola_68000::Cpu68000`]
//! that installs a decode hook in the shared core's
//! `variant_decode_hook` slot. The hook handles the 68010 ISA delta:
//! `MOVEC` (read/write VBR / SFC / DFC / USP) and `MOVE from CCR`
//! with a register destination. `Deref` / `DerefMut` forward
//! everything else to the inner 68000. Adding 68010-specific control
//! registers does not need new wrapper fields — `VBR`, `SFC`,
//! `DFC`, and the 68020-plus control regs all live on
//! [`motorola_68k_common::registers::Registers`], which is shared
//! across every member of the family.
//!
//! Deferred to a later phase: `RTD`, `MOVES`, loop-mode `DBcc`, and
//! the 68010 6-word exception frames — all of which need the
//! multi-step continuation pipeline (currently 68000-only).

pub mod cpu;

pub use cpu::{Cpu68010, decode_68010_opcode};
pub use motorola_68k_common::{CpuCapabilities, CpuModel, TimingClass};

/// Marker zero-sized type identifying the 68010 variant.
///
/// Reserved for the future per-variant generic shape: when
/// `Cpu68k<M: M68kVariant>` lands, this is the type that carries
/// `M68010`-specific associated types and methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M68010Variant;

impl M68010Variant {
    /// The CPU model this variant marker stands for.
    #[must_use]
    pub const fn model() -> CpuModel {
        CpuModel::M68010
    }
}

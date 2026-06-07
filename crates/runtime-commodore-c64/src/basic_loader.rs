//! C64 direct-to-RAM BASIC loader.
//!
//! Tokenises plain-text BASIC, imports the PRG image into RAM at `$0801`,
//! and fixes the BASIC zero-page pointers so the interpreter sees the
//! program as if it had been `LOAD`ed. Optionally types `RUN` + `RETURN`
//! so the program starts executing. This is the C64 counterpart of the
//! Spectrum `runtime-sinclair-zx-spectrum::basic_loader` — same shape and
//! `LoadBasicResult`/`LoadBasicError` surface — adapted to the C64 memory
//! map. It is far shorter than the Spectrum loader because the heavy
//! lifting (poke at `$0801`, relink line pointers, set `VARTAB`) already
//! lives in `format-commodore-c64-prg::load_prg`, reached here through
//! [`C64Runtime::load_prg_bytes`].

use emu198x_shell::{HeadlessSession, MachineTime, SessionError, SessionQueryProvider};
use format_commodore_c64_bas::{BasicProgram, tokenise};
use thiserror::Error;

use crate::C64Runtime;
use crate::typing::{DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES, type_string};

/// Default frame budget for waiting on the KERNAL `READY.` prompt before
/// installing the program. Mirrors the C64 autoload-tape default.
pub const DEFAULT_BASIC_LOADER_BOOT_FRAMES: u32 = 200;

/// Frames spent letting BASIC execute `RUN` after the keyword is typed —
/// long enough for the editor to leave the input line and the first
/// statement to draw.
const RUN_SETTLE_FRAMES: u32 = 30;

/// BASIC program area start on a stock C64 (`TXTTAB` after a cold boot).
const BASIC_START: u16 = 0x0801;

/// Start-of-variables pointer (`VARTAB`), set by the PRG import.
const VARTAB_LO: u16 = 0x2D;
const VARTAB_HI: u16 = 0x2E;
/// Start-of-arrays pointer (`ARYTAB`).
const ARYTAB_LO: u16 = 0x2F;
const ARYTAB_HI: u16 = 0x30;
/// End-of-strings pointer (`STREND`).
const STREND_LO: u16 = 0x31;
const STREND_HI: u16 = 0x32;

/// Largest program body (excluding the 2-byte PRG load-address header)
/// that fits between `$0801` and the BASIC ROM at `$A000`.
const MAX_PROGRAM_BODY: usize = 0xA000 - BASIC_START as usize;

/// Result of installing one BASIC program in RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadBasicResult {
    /// Imported program body length in bytes (PRG bytes minus the 2-byte
    /// load-address header).
    pub program_bytes: u16,
    /// Address the program was loaded at (`$0801`).
    pub load_addr: u16,
    /// `VARTAB` value after the import — the byte past the program.
    pub vartab: u16,
    /// Whether the loader drove the editor to `RUN` the program.
    pub ran: bool,
    /// Machine time reached after the loader completed.
    pub reached: MachineTime,
}

/// Failure surfaced by the C64 BASIC loader.
#[derive(Debug, Error)]
pub enum LoadBasicError {
    /// One headless-session operation failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// The source text could not be tokenised.
    #[error("could not tokenise BASIC source: {0}")]
    Tokenise(String),

    /// The PRG import into RAM failed.
    #[error("could not import tokenised program: {0}")]
    Import(String),

    /// The tokenised program is empty (header only, no lines).
    #[error("BASIC program is empty")]
    EmptyProgram,

    /// The tokenised program would not fit in the BASIC RAM area.
    #[error("BASIC program is {actual} bytes; maximum supported is {limit}")]
    ProgramTooLarge {
        /// Program body size in bytes.
        actual: usize,
        /// Maximum supported body size.
        limit: usize,
    },
}

/// Tokenises `source` and installs it, optionally running it.
///
/// Convenience wrapper over [`load_basic_program`] for callers that hold
/// the raw `.bas` text rather than a pre-tokenised [`BasicProgram`].
///
/// # Errors
///
/// Returns [`LoadBasicError::Tokenise`] when the source is not valid
/// BASIC, or any error from [`load_basic_program`].
pub fn load_basic_source<Q: SessionQueryProvider<C64Runtime>>(
    session: &mut HeadlessSession<C64Runtime, Q>,
    source: &str,
    run: bool,
    max_boot_frames: u32,
) -> Result<LoadBasicResult, LoadBasicError> {
    let program = tokenise(source).map_err(LoadBasicError::Tokenise)?;
    load_basic_program(session, &program, run, max_boot_frames)
}

/// Installs a tokenised BASIC program in RAM and (optionally) RUNs it.
///
/// Waits for the KERNAL `READY.` prompt, imports the PRG image at `$0801`
/// (which also relinks line pointers and sets `VARTAB`), mirrors `VARTAB`
/// into `ARYTAB`/`STREND` so a loaded-but-not-run program has a consistent
/// variable area, and — when `run` is set — types `RUN` + `RETURN` and
/// runs a short settle window.
///
/// # Errors
///
/// Returns [`LoadBasicError::EmptyProgram`] for a header-only program,
/// [`LoadBasicError::ProgramTooLarge`] when it overflows the BASIC area,
/// [`LoadBasicError::Import`] when the PRG import fails, or wraps a
/// [`SessionError`] for boot-wait / input failures.
pub fn load_basic_program<Q: SessionQueryProvider<C64Runtime>>(
    session: &mut HeadlessSession<C64Runtime, Q>,
    program: &BasicProgram,
    run: bool,
    max_boot_frames: u32,
) -> Result<LoadBasicResult, LoadBasicError> {
    let body_len = program.bytes.len().saturating_sub(2);
    if body_len == 0 {
        return Err(LoadBasicError::EmptyProgram);
    }
    if body_len > MAX_PROGRAM_BODY {
        return Err(LoadBasicError::ProgramTooLarge {
            actual: body_len,
            limit: MAX_PROGRAM_BODY,
        });
    }

    let _ = session.wait_for_boot(max_boot_frames)?;

    let load_addr = session
        .machine_mut()
        .load_prg_bytes(&program.bytes)
        .map_err(LoadBasicError::Import)?;

    // `load_prg` set VARTAB to the byte past the program. Read it back and
    // mirror it into ARYTAB/STREND so the variable, array, and string areas
    // start cleanly above the program even before an implicit CLR (RUN does
    // its own CLR; this keeps a non-run load consistent for LIST / inspect).
    let vartab = {
        let machine = session.machine_mut().machine_mut();
        let vartab =
            u16::from(machine.cpu_read(VARTAB_LO)) | (u16::from(machine.cpu_read(VARTAB_HI)) << 8);
        machine.cpu_write(ARYTAB_LO, (vartab & 0xFF) as u8);
        machine.cpu_write(ARYTAB_HI, (vartab >> 8) as u8);
        machine.cpu_write(STREND_LO, (vartab & 0xFF) as u8);
        machine.cpu_write(STREND_HI, (vartab >> 8) as u8);
        vartab
    };

    let ran = if run {
        type_string(
            session,
            "RUN\n",
            DEFAULT_KEY_HOLD_FRAMES,
            DEFAULT_TYPE_SETTLE_FRAMES,
        )?;
        session.run_frames(RUN_SETTLE_FRAMES)?;
        true
    } else {
        false
    };

    Ok(LoadBasicResult {
        program_bytes: body_len as u16,
        load_addr,
        vartab,
        ran,
        reached: session.time(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{C64SessionQueryProvider, Model};
    use emu198x_shell::HeadlessSession;

    fn stub_session() -> HeadlessSession<C64Runtime, C64SessionQueryProvider> {
        let runtime = C64Runtime::blank(Model::C64PalBreadbin);
        HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider)
    }

    #[test]
    fn rejects_header_only_program() {
        let mut session = stub_session();
        // PRG header with no lines and no end marker → empty body.
        let program = BasicProgram {
            bytes: vec![0x01, 0x08],
        };
        let err = load_basic_program(&mut session, &program, false, 1)
            .expect_err("header-only program should be rejected");
        assert!(matches!(err, LoadBasicError::EmptyProgram));
    }

    #[test]
    fn rejects_oversized_program() {
        let mut session = stub_session();
        let program = BasicProgram {
            bytes: vec![0u8; MAX_PROGRAM_BODY + 3],
        };
        let err = load_basic_program(&mut session, &program, false, 1)
            .expect_err("oversized program should be rejected");
        assert!(matches!(
            err,
            LoadBasicError::ProgramTooLarge { limit, .. } if limit == MAX_PROGRAM_BODY
        ));
    }

    #[test]
    fn tokenise_failure_surfaces_as_error() {
        let mut session = stub_session();
        // Missing line number is a tokeniser error.
        let err = load_basic_source(&mut session, "PRINT \"NO LINE NUMBER\"", false, 1)
            .expect_err("source without a line number should fail to tokenise");
        assert!(matches!(err, LoadBasicError::Tokenise(_)));
    }
}

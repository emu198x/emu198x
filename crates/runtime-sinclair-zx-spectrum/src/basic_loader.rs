//! Spectrum-family direct-to-RAM BASIC loader.
//!
//! Pokes a tokenised BASIC program at `PROG` and patches the system
//! variables that track program / variables / workspace boundaries so
//! the interpreter sees the new program as if it had been LOADed from
//! tape. Optionally types `R` `ENTER` to drive the editor's RUN
//! shortcut so the program starts executing.
//!
//! The 48K BASIC program area starts at `0x5CCB` after a clean boot;
//! 16K, 48K, and Spectrum+ share that layout. 128K-class variants
//! also keep the BASIC area at the same address (the BASIC ROM and
//! its system variables live in lower RAM regardless of paging), so
//! the helper is signature-bound to the 48K runtime today and broadens
//! when each later variant lands a runtime.

use emu198x_shell::{
    HeadlessSession, MachineCore, MachineTime, SessionError, SessionQueryProvider,
};
use format_sinclair_zx_spectrum_bas::BasicProgram;
use thiserror::Error;

use crate::SpectrumLiveAccess;
use crate::autoload::{decoded_prompt_line, tap_key, wait_for_prompt_line};

/// Default frame budget used to wait for the 48K ROM boot banner before
/// installing the program. Mirrors the autoload-tape default so script
/// authors can use the same value across both helpers.
pub const DEFAULT_BASIC_LOADER_BOOT_FRAMES: u32 = 250;

/// Frames spent letting the interpreter execute `RUN` after the keyword
/// has been typed. Long enough for the editor to leave K mode and the
/// first interpreted statement to draw to the screen.
const RUN_SETTLE_FRAMES: u32 = 30;

/// Standard 48K BASIC program area start, derived by the boot ROM and
/// pointed at by the `PROG` system variable on a clean boot.
const PROG_ADDR: u16 = 0x5CCB;

/// Bytes written immediately after the tokenised program: `$80` ends
/// the variables area, `$0D` terminates the empty edit line, and the
/// final `$80` marks the workspace boundary.
const TRAILING_BYTES: [u8; 3] = [0x80, 0x0D, 0x80];

const VARS_SYSVAR: u16 = 0x5C4B;
const E_LINE_SYSVAR: u16 = 0x5C59;
const K_CUR_SYSVAR: u16 = 0x5C5B;
const WORKSP_SYSVAR: u16 = 0x5C61;
const STKBOT_SYSVAR: u16 = 0x5C63;
const STKEND_SYSVAR: u16 = 0x5C65;

/// Maximum tokenised program size that fits below the BASIC stacks on
/// a vanilla 48K. Leaves a generous safety margin for the calculator
/// and GOSUB stacks which grow downward from `RAMTOP`.
const MAX_PROGRAM_BYTES: usize = 0x9000;

/// Result of installing one BASIC program in RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadBasicResult {
    /// Tokenised program length in bytes.
    pub program_bytes: u16,
    /// Final value written into the `VARS` system variable.
    pub vars_addr: u16,
    /// Final value written into the `E_LINE` system variable.
    pub e_line_addr: u16,
    /// Whether the helper drove the editor to `RUN` the program.
    pub ran: bool,
    /// Machine time reached after the helper completed.
    pub reached: MachineTime,
}

/// Failure surfaced by the BASIC loader.
#[derive(Debug, Error)]
pub enum LoadBasicError {
    /// One headless-session operation failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// The supplied tokenised program is empty.
    #[error("BASIC program is empty")]
    EmptyProgram,

    /// The tokenised program would not fit in the 48K BASIC area.
    #[error("BASIC program is {actual} bytes; maximum supported is {limit}")]
    ProgramTooLarge {
        /// Actual program size in bytes.
        actual: usize,
        /// Maximum supported size for the helper.
        limit: usize,
    },

    /// The 48K BASIC editor was not at the K prompt when the helper
    /// tried to install (and optionally `RUN`) the program.
    #[error("48K BASIC prompt was not ready; row 23 was {line:?}")]
    PromptNotReady {
        /// Decoded row-23 text observed when the check failed.
        line: String,
    },
}

/// Installs a tokenised BASIC program in RAM and (optionally) RUNs it.
///
/// Waits for the 48K ROM to reach the K prompt, then pokes the program
/// at `PROG` (`0x5CCB`), writes the `$80` `$0D` `$80` trailers, and
/// updates the `VARS` / `E_LINE` / `WORKSP` / `STKBOT` / `STKEND`
/// system variables so the BASIC interpreter sees a freshly-loaded
/// program. When `run` is `true`, types `R` (RUN keyword on a K
/// prompt) followed by `ENTER` and runs a short settle window so the
/// interpreter exits the editor and starts executing.
///
/// # Errors
///
/// Returns [`LoadBasicError::EmptyProgram`] for empty input,
/// [`LoadBasicError::ProgramTooLarge`] when the program overflows the
/// 48K BASIC area, [`LoadBasicError::PromptNotReady`] when the editor
/// is not at the K prompt, or wraps a [`SessionError`] for boot-wait
/// or input failures.
pub fn load_basic_program<R, Q>(
    session: &mut HeadlessSession<R, Q>,
    program: &BasicProgram,
    run: bool,
    max_boot_frames: u32,
) -> Result<LoadBasicResult, LoadBasicError>
where
    R: MachineCore + SpectrumLiveAccess,
    Q: SessionQueryProvider<R>,
{
    if program.bytes.is_empty() {
        return Err(LoadBasicError::EmptyProgram);
    }
    if program.bytes.len() > MAX_PROGRAM_BYTES {
        return Err(LoadBasicError::ProgramTooLarge {
            actual: program.bytes.len(),
            limit: MAX_PROGRAM_BYTES,
        });
    }
    let program_len = program.bytes.len() as u16;

    let _ = session.wait_for_boot(max_boot_frames)?;

    // Boot detection fires while the copyright banner is still visible
    // on row 23. One ENTER tap clears the banner and exposes the K
    // prompt — same shape as autoload_basic_tape.
    if decoded_prompt_line(session)?.trim_end() != "K" {
        tap_key(session, "enter")?;
    }
    // Then wait for the cursor rather than reading once. The tap only runs
    // the two frames of its own key edges, and the ROM has cleared row 23
    // and not yet repainted it, so an immediate read gets 32 spaces (#1413).
    let prompt = wait_for_prompt_line(session)?;
    if prompt.trim_end() != "K" {
        return Err(LoadBasicError::PromptNotReady { line: prompt });
    }

    poke_program(session, program, program_len);

    let trail_addr = PROG_ADDR.saturating_add(program_len);
    let vars = trail_addr;
    let e_line = vars.saturating_add(1);
    let worksp = e_line.saturating_add(2);

    update_system_variables(session, vars, e_line, worksp);

    let ran = if run {
        tap_key(session, "r")?;
        tap_key(session, "enter")?;
        session.run_frames(RUN_SETTLE_FRAMES)?;
        true
    } else {
        false
    };

    Ok(LoadBasicResult {
        program_bytes: program_len,
        vars_addr: vars,
        e_line_addr: e_line,
        ran,
        reached: session.time(),
    })
}

fn poke_program<R, Q>(session: &mut HeadlessSession<R, Q>, program: &BasicProgram, program_len: u16)
where
    R: MachineCore + SpectrumLiveAccess,
    Q: SessionQueryProvider<R>,
{
    let machine = session.machine_mut();
    for (i, byte) in program.bytes.iter().enumerate() {
        machine.write_byte(PROG_ADDR.saturating_add(i as u16), *byte);
    }
    let trail_addr = PROG_ADDR.saturating_add(program_len);
    for (i, byte) in TRAILING_BYTES.iter().enumerate() {
        machine.write_byte(trail_addr.saturating_add(i as u16), *byte);
    }
}

fn update_system_variables<R, Q>(
    session: &mut HeadlessSession<R, Q>,
    vars: u16,
    e_line: u16,
    worksp: u16,
) where
    R: MachineCore + SpectrumLiveAccess,
    Q: SessionQueryProvider<R>,
{
    let machine = session.machine_mut();
    write_word_le(machine, VARS_SYSVAR, vars);
    write_word_le(machine, E_LINE_SYSVAR, e_line);
    // K_CUR is the editor's cursor inside the current edit-line buffer.
    // After boot it points at E_LINE; if we move E_LINE without updating
    // K_CUR, the next keypress lands at the OLD K_CUR position — which
    // is now inside the program area — and the inserted byte corrupts
    // the program's line header, with the LIST display then interpreting
    // bytes 0..1 of the program as a wrong line number. Park it at the
    // start of the now-empty edit area so the editor accepts new input
    // there.
    write_word_le(machine, K_CUR_SYSVAR, e_line);
    write_word_le(machine, WORKSP_SYSVAR, worksp);
    write_word_le(machine, STKBOT_SYSVAR, worksp);
    write_word_le(machine, STKEND_SYSVAR, worksp);
}

fn write_word_le<A: SpectrumLiveAccess>(machine: &mut A, addr: u16, word: u16) {
    machine.write_byte(addr, (word & 0xFF) as u8);
    machine.write_byte(addr.saturating_add(1), (word >> 8) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spectrum48kRuntime;
    use crate::runtime::SpectrumMachine;
    use emu198x_shell::{FirmwareImage, FirmwareSet, QueryResult};
    use serde_json::json;

    fn loaded_runtime() -> Spectrum48kRuntime {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &[0; 16 * 1024],
        ));
        Spectrum48kRuntime::from_firmware(&firmware).expect("dummy firmware should boot")
    }

    /// Provider that reports a detected boot status and a row-23 K
    /// prompt — drives the loader's success path without needing a
    /// real ROM.
    struct ReadyPromptProvider;

    impl SessionQueryProvider<Spectrum48kRuntime> for ReadyPromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec![
                "boot.detected".to_owned(),
                "boot.reason".to_owned(),
                "boot.row".to_owned(),
                "screen.text.lines".to_owned(),
            ]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            let value = match path {
                "boot.detected" => json!(true),
                "boot.reason" => json!("found copyright banner on row 23"),
                "boot.row" => json!(23),
                "screen.text.lines" => {
                    let mut lines = vec![" ".repeat(32); 24];
                    lines[23] = "K                               ".to_owned();
                    json!(lines)
                }
                _ => return Ok(None),
            };
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }))
        }
    }

    #[test]
    fn empty_program_is_rejected() {
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, ReadyPromptProvider);
        let program = BasicProgram { bytes: Vec::new() };
        let err = load_basic_program(
            &mut session,
            &program,
            false,
            DEFAULT_BASIC_LOADER_BOOT_FRAMES,
        )
        .expect_err("empty program should be rejected");
        assert!(matches!(err, LoadBasicError::EmptyProgram));
    }

    #[test]
    fn oversized_program_is_rejected() {
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, ReadyPromptProvider);
        let program = BasicProgram {
            bytes: vec![0u8; MAX_PROGRAM_BYTES + 1],
        };
        let err = load_basic_program(
            &mut session,
            &program,
            false,
            DEFAULT_BASIC_LOADER_BOOT_FRAMES,
        )
        .expect_err("oversized program should be rejected");
        assert!(matches!(err, LoadBasicError::ProgramTooLarge { .. }));
    }

    #[test]
    fn poke_writes_program_bytes_and_updates_system_variables() {
        // Use a fake-ROM runtime; we only need the memory bus to be
        // wired up, not the full BASIC ROM, because we're verifying
        // the address-level behaviour rather than actual interpretation.
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, ReadyPromptProvider);

        let program = BasicProgram {
            bytes: vec![0xAA, 0xBB, 0xCC, 0xDD],
        };
        let result = load_basic_program(
            &mut session,
            &program,
            false,
            DEFAULT_BASIC_LOADER_BOOT_FRAMES,
        )
        .expect("loader should succeed");

        assert_eq!(result.program_bytes, 4);
        assert_eq!(result.vars_addr, PROG_ADDR + 4);
        assert_eq!(result.e_line_addr, PROG_ADDR + 5);
        assert!(!result.ran);

        // Read back the four poked bytes.
        let machine = session.machine().machine();
        for (i, expected) in program.bytes.iter().enumerate() {
            assert_eq!(machine.read_byte(PROG_ADDR + i as u16), *expected);
        }
        // Trailing markers.
        assert_eq!(machine.read_byte(PROG_ADDR + 4), 0x80);
        assert_eq!(machine.read_byte(PROG_ADDR + 5), 0x0D);
        assert_eq!(machine.read_byte(PROG_ADDR + 6), 0x80);

        // System variables written little-endian.
        let read_word = |addr: u16| -> u16 {
            u16::from(machine.read_byte(addr)) | (u16::from(machine.read_byte(addr + 1)) << 8)
        };
        assert_eq!(read_word(VARS_SYSVAR), PROG_ADDR + 4);
        assert_eq!(read_word(E_LINE_SYSVAR), PROG_ADDR + 5);
        assert_eq!(read_word(WORKSP_SYSVAR), PROG_ADDR + 7);
        assert_eq!(read_word(STKBOT_SYSVAR), PROG_ADDR + 7);
        assert_eq!(read_word(STKEND_SYSVAR), PROG_ADDR + 7);
    }

    /// Provider returning a non-K prompt — drives the PromptNotReady arm.
    struct StuckPromptProvider;

    impl SessionQueryProvider<Spectrum48kRuntime> for StuckPromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec!["boot.detected".to_owned(), "screen.text.lines".to_owned()]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            let value = match path {
                "boot.detected" => json!(true),
                "screen.text.lines" => {
                    let lines = vec!["X".repeat(32); 24];
                    json!(lines)
                }
                _ => return Ok(None),
            };
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }))
        }
    }

    #[test]
    fn loader_refuses_to_poke_when_prompt_is_not_ready() {
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, StuckPromptProvider);
        let program = BasicProgram {
            bytes: vec![0x00, 0x0A, 0x02, 0x00, 0xFB, 0x0D],
        };
        let err = load_basic_program(
            &mut session,
            &program,
            false,
            DEFAULT_BASIC_LOADER_BOOT_FRAMES,
        )
        .expect_err("loader must refuse to type into a non-K prompt");
        match err {
            LoadBasicError::PromptNotReady { line } => assert!(line.starts_with('X')),
            other => panic!("expected PromptNotReady, got {other:?}"),
        }
    }

    /// Provider that repaints row 23 the way the real ROM does: the
    /// copyright banner, then a gap of blank rows while the ROM clears
    /// and redraws the edit line, then the `K` cursor.
    ///
    /// `ReadyPromptProvider` reports `K` from the very first read, which
    /// is why the existing tests passed while the loader was broken on
    /// every real 48K boot (#1413).
    struct RepaintingPromptProvider {
        reads: std::cell::Cell<usize>,
    }

    impl RepaintingPromptProvider {
        /// Row-23 reads that come back before the cursor is drawn.
        const BLANK_READS: usize = 4;

        fn new() -> Self {
            Self {
                reads: std::cell::Cell::new(0),
            }
        }
    }

    impl SessionQueryProvider<Spectrum48kRuntime> for RepaintingPromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec!["boot.detected".to_owned(), "screen.text.lines".to_owned()]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            let value = match path {
                "boot.detected" => json!(true),
                "screen.text.lines" => {
                    let seen = self.reads.get();
                    self.reads.set(seen + 1);
                    let row23 = if seen == 0 {
                        // wait_for_boot returns on the banner.
                        format!("{:<32}", "\u{a9} 1982 Sinclair Research Ltd")
                    } else if seen <= Self::BLANK_READS {
                        // Cleared, not yet repainted. This is the state
                        // the loader used to sample exactly once.
                        " ".repeat(32)
                    } else {
                        format!("{:<32}", "K")
                    };
                    let mut lines = vec![" ".repeat(32); 24];
                    lines[23] = row23;
                    json!(lines)
                }
                _ => return Ok(None),
            };
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }))
        }
    }

    #[test]
    fn the_prompt_is_waited_for_rather_than_sampled_once() {
        let runtime = loaded_runtime();
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, RepaintingPromptProvider::new());
        let program = BasicProgram {
            bytes: vec![0x00, 0x0A, 0x02, 0x00, 0xFB, 0x0D],
        };

        let result = load_basic_program(
            &mut session,
            &program,
            false,
            DEFAULT_BASIC_LOADER_BOOT_FRAMES,
        )
        .expect("loader must wait for the cursor instead of failing on the repaint gap");

        assert_eq!(result.program_bytes, 6);
    }
}

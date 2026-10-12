use std::path::PathBuf;

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{HeadlessSession, MachineCore};
use runtime_sinclair_zx_spectrum::{
    Spectrum48kRuntime, SpectrumLiveAccess, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
};

#[test]
#[ignore = "FIXTURE: requires genuine 48K ROM; explicitly fails if unavailable"]
fn shared_basic_native_loader_preserves_rom_editor_bytes_and_runs() {
    let path = std::env::var_os("EMU198X_SPECTRUM_48K_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME or ROM path"))
                .join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
        });
    let rom = std::fs::read(path).expect("provision the genuine 48K ROM");
    let runtime = Spectrum48kRuntime::from_rom_bytes(&rom).expect("48K ROM");
    let mut session = HeadlessSession::new_with_query_provider(
        SpectrumRuntimeKind::Spectrum48K(runtime),
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );
    // Independent ROM captures, with provenance beside the shared fixture.
    let source = include_str!("../../emu198x-spectrum-web/tests/fixtures/rom-editor/cases.bas");
    let expected: Vec<u8> =
        include_str!("../../emu198x-spectrum-web/tests/fixtures/rom-editor/cases.hex")
            .lines()
            .flat_map(|hex| {
                (0..hex.len())
                    .step_by(2)
                    .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("captured hex"))
            })
            .collect();
    assert!(!expected.is_empty());
    let result = SpectrumRuntimeKind::load_basic_program(&mut session, source, false)
        .expect("native source loader");
    assert!(!result.ran);
    assert_eq!(usize::from(result.program_bytes), expected.len());
    let bytes: Vec<_> = (0..result.program_bytes)
        .map(|offset| session.machine().read_byte(0x5CCB + offset))
        .collect();
    assert_eq!(
        bytes, expected,
        "installed program must match ROM editor captures"
    );

    // The native loader waits for the cold-boot banner on each call. Exercise
    // RUN from a fresh machine, as the loader's host entry point expects.
    let runtime = Spectrum48kRuntime::from_rom_bytes(&rom).expect("48K ROM");
    let mut session = HeadlessSession::new_with_query_provider(
        SpectrumRuntimeKind::Spectrum48K(runtime),
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );
    let result = SpectrumRuntimeKind::load_basic_program(&mut session, "10 POKE 32768,42", true)
        .expect("run a listing after cold boot");
    assert!(result.ran);
    assert_eq!(session.machine().read_byte(32768), 42);
}

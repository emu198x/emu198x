use std::path::PathBuf;

use common_commodore_c64::TIMING_PAL_BREADBIN;
use emu198x_shell::HeadlessSession;
use runtime_commodore_c64::file_loader::load_host_file;
use runtime_commodore_c64::load_basic_source;
use runtime_commodore_c64::type_string;
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};

#[test]
#[ignore = "FIXTURE: requires genuine C64 ROMs; explicitly fails if unavailable"]
fn shared_basic_loaders_match_the_live_rom_cruncher_and_run() {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME or ROM root")).join(".emu198x/roms")
        })
        .join("commodore-c64");
    let runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        std::fs::read(root.join("kernal.rom")).expect("KERNAL"),
        std::fs::read(root.join("basic.rom")).expect("BASIC"),
        std::fs::read(root.join("chargen.rom")).expect("CHARGEN"),
        None,
    )
    .expect("C64 firmware");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    session.wait_for_boot(200).expect("READY prompt");
    let source = "0 ?42:POKE 49152,73\n";
    assert_eq!(
        type_string(&mut session, source, 3, 10).expect("type into ROM"),
        source.len() as u32
    );
    let ram = session.machine().machine().memory();
    let end = u16::from_le_bytes([ram.ram_read(0x2D), ram.ram_read(0x2E)]);
    assert!(end > 0x0808, "the ROM must have stored the line");
    let expected: Vec<_> = (0x0801..end).map(|addr| ram.ram_read(addr)).collect();
    assert_eq!(&expected[2..5], &[0, 0, 0x99], "line zero and PRINT token");

    let mut file_runtime = C64Runtime::blank(Model::C64PalBreadbin);
    load_host_file(&mut file_runtime, "oracle.bas", source.as_bytes()).expect("file import");
    let file_bytes: Vec<_> = (0x0801..end)
        .map(|addr| file_runtime.machine().memory().ram_read(addr))
        .collect();
    assert_eq!(file_bytes, expected, "file import agrees with the live ROM");

    load_basic_source(&mut session, "10 END", false, 200).expect("replace captured program");
    let result = load_basic_source(&mut session, source, false, 200).expect("source import");
    assert_eq!(result.vartab, end);
    let loaded: Vec<_> = (0x0801..end)
        .map(|addr| session.machine().machine().memory().ram_read(addr))
        .collect();
    assert_eq!(loaded, expected, "direct loader agrees with the live ROM");
    let result = load_basic_source(&mut session, source, true, 200).expect("RUN");
    assert!(result.ran);
    assert_eq!(session.machine().machine().memory().ram_read(49152), 73);
}

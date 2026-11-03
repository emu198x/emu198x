//! Identified original 48 BASIC ROM on every implemented physical board/RAM pair.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, MachineCore, SessionQueryProvider,
};
use runtime_sinclair_zx_spectrum::{
    Model, SpectrumLiveAccess, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

// Sinclair's original ROM, also held by Fuse 1.7.0. MAME spectrum.cpp BIOS 0
// independently identifies SHA-1 5ea7c2b824672e914525d1d5c419d71b84a426a2.
const ROM_SHA256: &str = "d55daa439b673b0e3f5897f99ac37ecb45f974d1862b4dadb85dec34af99cb42";

fn screen(machine: &SpectrumRuntimeKind) -> Vec<String> {
    SpectrumSessionQueryProvider
        .query(machine, "screen.text.lines")
        .expect("screen query")
        .expect("screen text exists")
        .value
        .as_array()
        .expect("screen rows")
        .iter()
        .map(|row| row.as_str().expect("text row").to_owned())
        .collect()
}

#[test]
#[ignore = "FIXTURE: requires identified original 48K ROM; fails if absent or changed"]
fn original_rom_boots_and_executes_basic_on_all_physical_boards_and_ram_profiles() {
    let path = PathBuf::from(
        std::env::var_os("EMU198X_SPECTRUM_48K_ROM")
            .expect("set EMU198X_SPECTRUM_48K_ROM to the original ROM"),
    );
    let rom = std::fs::read(path).expect("read original firmware");
    assert_eq!(rom.len(), 16384, "complete original ROM size");
    let hash: String = Sha256::digest(&rom)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, ROM_SHA256, "complete original ROM identity");
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    let mut checked = 0;
    let mut observations = Vec::new();
    for model in Model::ALL
        .into_iter()
        .filter(|m| m.physical_board().is_some())
    {
        let runtime = SpectrumRuntimeKind::from_firmware(model, &firmware).expect("fixed owner");
        let ticks = runtime.native_frame_ticks();
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, ticks, SpectrumSessionQueryProvider);
        session.run_frames(160).expect("bounded native boot");
        let boot = screen(session.machine());
        assert!(
            boot.iter()
                .any(|row| row.contains("1982 Sinclair Research Ltd")),
            "{}: original boot banner: {boot:?}",
            model.profile_id()
        );
        let loaded = SpectrumRuntimeKind::load_basic_program(
            &mut session,
            "10 POKE 30000,42\n20 POKE 40000,85\n30 PRINT PEEK 30000\n40 PRINT PEEK 40000",
            true,
        )
        .expect("ROM interpreter executes a listing through native RUN input");
        assert!(loaded.ran);
        session.run_frames(2).expect("completed output fields");
        let lines = screen(session.machine());
        let upper = if model.ram_kib() == Some(16) { 255 } else { 85 };
        assert_eq!(
            session.machine().read_byte(30000),
            42,
            "{}: CPU write reaches logical lower RAM",
            model.profile_id()
        );
        assert_eq!(
            session.machine().read_byte(40000),
            upper,
            "{}: CPU write follows fitted upper RAM",
            model.profile_id()
        );
        assert_eq!(
            lines[0].trim(),
            "42",
            "{}: lower PEEK result",
            model.profile_id()
        );
        assert_eq!(
            lines[1].trim(),
            upper.to_string(),
            "{}: upper PEEK result",
            model.profile_id()
        );
        let frame = session.latest_frame().expect("native output");
        assert_eq!((frame.width, frame.height), (352, 296));
        if let Some(root) = std::env::var_os("EMU198X_SPECTRUM_BOARD_ARTIFACTS") {
            use std::io::Write;
            let root = PathBuf::from(root);
            std::fs::create_dir_all(&root).expect("artifact root");
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(format!("{}.png", model.profile_id())))
                .expect("new capture only")
                .write_all(&frame.png_bytes().expect("native PNG"))
                .expect("capture bytes");
        }
        observations.push(serde_json::json!({"profile":model.profile_id(),
            "machine":model.model_id(),"ram_kib":model.ram_kib(),"rom_sha256":ROM_SHA256,
            "boot_banner":true,"lower_peek":42,"upper_peek":upper,
            "native_dimensions":[frame.width,frame.height]}));
        checked += 1;
    }
    assert_eq!(checked, 18);
    if let Some(root) = std::env::var_os("EMU198X_SPECTRUM_BOARD_ARTIFACTS") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(PathBuf::from(root).join("observations.json"))
            .expect("new report only")
            .write_all(&serde_json::to_vec_pretty(&observations).expect("report JSON"))
            .expect("report bytes");
    }
}

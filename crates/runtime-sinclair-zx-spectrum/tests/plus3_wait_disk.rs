//! Original Cybernoid loader, compared with an independent MAME bus observation.
//! No replacement screenshot oracle: preserve every existing catalogue hash.
use std::path::PathBuf;

use common_sinclair_zx_spectrum::timing::TIMING_PLUS2A;
use emu198x_shell::{
    HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet, read_media_asset,
};
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_sinclair_zx_spectrum::{Model, SpectrumPlus3Runtime, SpectrumSessionQueryProvider};
use sha2::{Digest, Sha256};

fn assert_identity(bytes: &[u8], expected: &str, input: &str) {
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(actual, expected, "complete {input} identity");
}

#[test]
#[ignore = "FIXTURE: original +3 ROM pages and Cybernoid DSK; explicit execution verifies full input identities"]
fn original_loader_reads_every_byte_seen_on_the_independent_reference_bus() {
    let rom_root = std::env::var_os("EMU198X_PLUS3_WAIT_ROM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME or explicit ROM directory"))
                .join(".emu198x/roms/amstrad-zx-spectrum-plus3")
        });
    let disk_path = std::env::var_os("EMU198X_PLUS3_WAIT_DISK").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Volumes/Data/Library/ROMs/TOSEC/Sinclair/ZX Spectrum/Games/[DSK]/Cybernoid - The Fighting Machine (1988)(Hewson Consultants)(+3).zip"));
    let identities = [
        "841d54545c743a7202fb9b068534ed906a08cc657a19955ae20a8de5af8ab185",
        "6d6ccab28f255b01c8cd89ceab9e7b60247f4610471118092f290f6c7768b7e3",
        "c5590212a269117657ce611acbac582e41e9db87dc78db480da2615e413b2da6",
        "8ec5cb9fd7ec24a041800314bb5a2c99be02f7e50745194eb649ed6d9085bd1f",
    ];
    let roms: Vec<_> = identities
        .iter()
        .enumerate()
        .map(|(index, sha)| {
            let bytes = std::fs::read(rom_root.join(format!("plus3-{index}.rom")))
                .expect("original ROM page");
            assert_identity(&bytes, sha, "ROM page");
            bytes
        })
        .collect();
    let disk = read_media_asset(&disk_path, MediaKind::Disk).expect("original Cybernoid disk");
    assert_identity(
        &disk.bytes,
        "840b39c2e2cfa40320f471de9c5ebbe10915c0d4cabd52b68616fe03191fbdf8",
        "decoded disk",
    );
    let mut machine = SpectrumPlus3::new();
    machine
        .memory
        .load_roms(&roms[0], &roms[1], &roms[2], &roms[3]);
    let mut session = HeadlessSession::new_with_query_provider(
        SpectrumPlus3Runtime::new(Model::SpectrumPlus3, machine),
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, &disk.bytes));
    session.prepare(&media, &[]).expect("mount original disk");
    session.wait_for_boot(250).expect("original firmware menu");
    session.run_frames(50).expect("menu settles");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session.run_frames(5).expect("ordinary ENTER");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });
    let mut status = 0;
    let mut execution = Vec::new();
    for _ in 0..1500 {
        session.machine_mut().machine_mut().start_io_trace();
        session.run_frames(1).expect("bounded native loader frame");
        for event in session.machine_mut().machine_mut().take_io_trace() {
            if event.write {
                continue;
            }
            match event.port & 0xf002 {
                0x2000 => status = event.value,
                0x3000 if status & 0xe0 == 0xe0 && event.pc == 0xff58 => {
                    execution.push(event.value);
                    assert!(execution.len() <= 49152, "unexpected extra loader data");
                }
                _ => {}
            }
        }
        if execution.len() == 49152 {
            break;
        }
    }
    assert_eq!(
        execution.len(),
        49152,
        "all twelve original 4 KiB transfers must complete"
    );
    // MAME 0.289, original specpls3 English BIOS and the identical DSK, ordinary
    // ENTER at 4.9 s. Transparent I/O taps leave all three screenshot files
    // byte-identical to the untapped run. Count only NDMA/RQM/DIO execution
    // reads at the ROM-copied loop's PC FF58, excluding result-phase bytes.
    // The private native trace/provenance is retained with #2265 evidence.
    assert_identity(
        &execution,
        "f55ada06cff6f6f87f501f9acd3fea7525f7a87dec0bd1021619ec0e0cc188ff",
        "independent fast-loader bus stream",
    );
}

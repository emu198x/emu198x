//! Original AES software control for the ROM-set layout correction (#2133).
//! Exact fixture identities and the independent MAME capture are retained in
//! docs/plans/2026-10-28-small-issue-fixes-3/.
use std::{fs::File, io::Read, path::PathBuf};

use emu198x_shell::HeadlessSession;
use runtime_snk_neo_geo::{Model, NeoGeoRuntime};

#[test]
#[ignore = "FIXTURE: aes.zip and mslug.zip in EMU198X_NEOGEO_MAME_ROMS; exact identities in the #2133 receipt"]
fn metal_slug_matches_the_reference_attract_frame() {
    let root =
        PathBuf::from(std::env::var_os("EMU198X_NEOGEO_MAME_ROMS").expect("fixture directory"));
    let mut aes = zip::ZipArchive::new(File::open(root.join("aes.zip")).expect("AES archive"))
        .expect("AES zip");
    let mut read = |name: &str| {
        let mut bytes = Vec::new();
        aes.by_name(name)
            .expect("firmware entry")
            .read_to_end(&mut bytes)
            .expect("firmware bytes");
        bytes
    };
    let mut runtime = NeoGeoRuntime::new(Model::Aes, read("neo-epo.bin"), Some(read("000-lo.lo")))
        .expect("Asia AES");
    runtime
        .insert_cartridge(&std::fs::read(root.join("mslug.zip")).expect("original cartridge"))
        .expect("ROM-set loader");
    let machine = runtime.machine().expect("machine");
    let header: Vec<u8> = (0x100..0x108).map(|at| machine.peek(at)).collect();
    assert_eq!(header, b"NEO-GEO\0");
    let mut session = HeadlessSession::new(runtime, Model::Aes.frame_ticks());
    session
        .run_frames(900)
        .expect("bounded no-input attract run");
    let frame = session.machine().machine().expect("machine").framebuffer();
    assert_eq!(frame.len(), 320 * 224);
    // FNV-1a of the independent MAME AES/Asia frame's RGB bytes. The receipt
    // also retains its SHA-256 and exact zero-difference pixel comparison.
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    for &pixel in frame {
        for byte in [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8] {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
    assert_eq!(
        digest, 0xe58b_8b80_650c_0928,
        "Metal Slug must reach the independently captured PUSH START screen"
    );
}

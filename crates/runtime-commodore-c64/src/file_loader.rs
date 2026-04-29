//! Host-side file import helpers for the fresh-workspace C64 runtime.
//!
//! These paths are explicit convenience imports, not emulated media.

use format_commodore_c64_bas::tokenise;
use format_commodore_c64_d64::extract_first_prg;
use format_commodore_c64_t64::extract_first_program;

use crate::C64Runtime;

/// Loads one supported host-side file into the runtime.
///
/// Currently supported:
/// - `.prg`: imported directly into RAM
/// - `.bas`: tokenised to PRG bytes, then imported into RAM
/// - `.t64`: first loadable archive entry extracted and imported into RAM
/// - `.d64`: first PRG directory entry extracted and imported into RAM
///
/// # Errors
///
/// Returns an error if the extension is unsupported or parsing fails.
pub fn load_host_file(machine: &mut C64Runtime, name: &str, data: &[u8]) -> Result<String, String> {
    let lower = name.to_ascii_lowercase();

    if lower.ends_with(".prg") {
        let load_addr = machine.load_prg_bytes(data)?;
        return Ok(format!(
            "Imported PRG: {name} ({} bytes, load address ${load_addr:04X})",
            data.len()
        ));
    }

    if lower.ends_with(".bas") {
        let source = std::str::from_utf8(data)
            .map_err(|err| format!("BASIC source is not valid UTF-8: {err}"))?;
        let program = tokenise(source)?;
        let load_addr = machine.load_prg_bytes(&program.bytes)?;
        return Ok(format!(
            "Imported BASIC source: {name} ({} bytes source, tokenised load address ${load_addr:04X})",
            data.len()
        ));
    }

    if lower.ends_with(".t64") {
        let program = extract_first_program(data)
            .map_err(|err| format!("failed to parse T64 container: {err}"))?;
        let load_addr = machine.load_prg_bytes(&program.prg_bytes())?;
        return Ok(format!(
            "Imported T64 entry: {} from {name} ({} bytes, load address ${load_addr:04X})",
            program.name,
            program.data.len()
        ));
    }

    if lower.ends_with(".d64") {
        let program =
            extract_first_prg(data).map_err(|err| format!("failed to parse D64 image: {err}"))?;
        let load_addr = machine.load_prg_bytes(&program.data)?;
        return Ok(format!(
            "Imported D64 entry: {} from {name} ({} bytes, load address ${load_addr:04X})",
            program.name,
            program.data.len()
        ));
    }

    Err(format!(
        "unrecognised file extension: {name}. Supported: .prg, .bas, .t64, .d64"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;

    #[test]
    fn rejects_unrecognised_extension() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let err = load_host_file(&mut runtime, "demo.zip", &[0])
            .expect_err("zip is not in the supported list");
        assert!(err.contains("unrecognised file extension"));
    }

    #[test]
    fn bas_loader_reports_non_utf8_source() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let err = load_host_file(&mut runtime, "broken.bas", &[0xFF, 0xFE, 0xFD])
            .expect_err("non-UTF-8 BASIC source should be rejected");
        assert!(err.contains("BASIC source is not valid UTF-8"));
    }

    #[test]
    fn t64_loader_reports_invalid_archive() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let err = load_host_file(&mut runtime, "broken.t64", &[0; 4])
            .expect_err("4-byte file is not a T64 archive");
        assert!(err.contains("failed to parse T64 container"));
    }

    #[test]
    fn d64_loader_reports_invalid_image() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let err = load_host_file(&mut runtime, "broken.d64", &[0; 4])
            .expect_err("4-byte file is not a D64 image");
        assert!(err.contains("failed to parse D64 image"));
    }

    #[test]
    fn imports_prg_file() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let msg = load_host_file(&mut runtime, "demo.prg", &[0x00, 0xC0, 0x11, 0x22])
            .expect("PRG should import");

        assert!(msg.contains("Imported PRG"));
        assert_eq!(runtime.machine().memory().ram_read(0xC000), 0x11);
        assert_eq!(runtime.machine().memory().ram_read(0xC001), 0x22);
    }

    #[test]
    fn imports_bas_file_via_tokeniser() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let msg = load_host_file(&mut runtime, "demo.bas", b"10 END\n").expect("BAS should import");

        assert!(msg.contains("Imported BASIC source"));
        assert_eq!(runtime.machine().memory().ram_read(0x0801), 0x07);
        assert_eq!(runtime.machine().memory().ram_read(0x0802), 0x08);
        assert_eq!(runtime.machine().memory().ram_read(0x0805), 0x80);
    }

    #[test]
    fn imports_first_t64_entry() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let mut image = vec![0; 0x400];
        image[..19].copy_from_slice(b"C64 tape image file");
        image[0x22..0x24].copy_from_slice(&(1u16).to_le_bytes());
        image[0x24..0x26].copy_from_slice(&(1u16).to_le_bytes());
        image[0x40] = 1;
        image[0x41] = 0x82;
        image[0x42..0x44].copy_from_slice(&(0xC000u16).to_le_bytes());
        image[0x44..0x46].copy_from_slice(&(0xC003u16).to_le_bytes());
        image[0x48..0x4C].copy_from_slice(&(0x400u32).to_le_bytes());
        image[0x50..0x57].copy_from_slice(b"DEMO   ");
        image.extend_from_slice(&[0x11, 0x22, 0x33]);

        let msg = load_host_file(&mut runtime, "demo.t64", &image).expect("T64 should import");

        assert!(msg.contains("Imported T64 entry"));
        assert_eq!(runtime.machine().memory().ram_read(0xC000), 0x11);
        assert_eq!(runtime.machine().memory().ram_read(0xC001), 0x22);
        assert_eq!(runtime.machine().memory().ram_read(0xC002), 0x33);
    }

    #[test]
    fn imports_first_d64_entry() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let mut image = vec![0; 174_848];

        let mut bam = [0u8; 256];
        bam[0] = 18;
        bam[1] = 1;
        let bam_offset = 357 * 256;
        image[bam_offset..bam_offset + 256].copy_from_slice(&bam);

        let mut directory = [0u8; 256];
        directory[2] = 0x82;
        directory[3] = 1;
        directory[4] = 0;
        directory[5..9].copy_from_slice(b"DEMO");
        directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
        let dir_offset = 358 * 256;
        image[dir_offset..dir_offset + 256].copy_from_slice(&directory);

        let mut file_sector = [0u8; 256];
        file_sector[0] = 0;
        file_sector[1] = 5;
        file_sector[2..6].copy_from_slice(&[0x00, 0xC0, 0x11, 0x22]);
        image[..256].copy_from_slice(&file_sector);

        let msg = load_host_file(&mut runtime, "demo.d64", &image).expect("D64 should import");

        assert!(msg.contains("Imported D64 entry"));
        assert_eq!(runtime.machine().memory().ram_read(0xC000), 0x11);
        assert_eq!(runtime.machine().memory().ram_read(0xC001), 0x22);
    }
}

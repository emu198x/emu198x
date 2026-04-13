//! Host-side file import helpers for the fresh-workspace C64 runtime.
//!
//! These paths are explicit convenience imports, not emulated media.

use format_commodore_c64_bas::tokenise;

use crate::C64Runtime;

/// Loads one supported host-side file into the runtime.
///
/// Currently supported:
/// - `.prg`: imported directly into RAM
/// - `.bas`: tokenised to PRG bytes, then imported into RAM
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

    Err(format!(
        "unrecognised file extension: {name}. Supported: .prg, .bas"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;

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
}

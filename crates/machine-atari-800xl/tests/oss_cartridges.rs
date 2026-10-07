//! Real OSS cartridges boot to their prompt under the official XL OS.
//!
//! OSS sold its languages on bank-switched boards, and TOSEC ships them as
//! headerless 16 KB (or 8 KB) images that size alone cannot tell from a
//! flat cartridge. These load each dump with no header and no override, so
//! they pass only if the CRC32 table names the right board *and* the board
//! banks the way the software expects. A mismapped OSS cart still runs: it
//! executes the wrong 4 KB and paints garbage or nothing, so each check
//! reads the text the program drew rather than trusting that frames ran.
//!
//! Fixtures, both required:
//!
//! - `EMU198X_ATARI_800XL_OS` — the official Atari XL/XE OS revision 2
//!   (`ATARIXL.ROM`, CRC32 `1F9CD270`, Altirra Hardware Reference Manual
//!   F.4). Any other image is refused: a patched OS can mask or cause a
//!   cartridge-start difference.
//! - `EMU198X_ATARI_800XL_OSS_DIR` — a directory holding the TOSEC dumps
//!   from `Atari/8bit/Applications/[BIN]`, extracted, plus the Writer's
//!   Tool program disk from `[ATR]`. Files are found by CRC32, so their
//!   names do not matter, and every dump below must be present.

use std::collections::HashMap;
use std::path::PathBuf;

use format_atari_8bit_atr::AtrImage;
use machine_atari_800xl::{Atari800xl, Atari800xlRegion, Cartridge, CartridgeKind};

const OFFICIAL_XL_OS_CRC32: u32 = 0x1F9C_D270;

/// The Writer's Tool program disk (MAME `writerd`, `writer's tool.atr`).
const WRITERS_TOOL_DISK_CRC32: u32 = 0x5A6E_2133;

/// What a cartridge must put in its text window, and how to get it there.
struct Expect {
    crc: u32,
    title: &'static str,
    kind: CartridgeKind,
    /// Hold START on this frame for ten frames (BASIC XE 7.2's boot menu).
    press_start_at: Option<u32>,
    frames: u32,
    text: &'static [&'static str],
}

/// The TOSEC dumps, by the CRC32 MAME lists for each.
const CARTS: &[Expect] = &[
    Expect {
        crc: 0x5E03_3719,
        title: "MAC/65 1.00 (043M)",
        kind: CartridgeKind::OssTwoChip,
        press_start_at: None,
        frames: 300,
        text: &["MAC/65               Version 1.00", "Edit"],
    },
    Expect {
        crc: 0x4102_CA4F,
        title: "MAC/65 1.01 (M091)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: None,
        frames: 300,
        text: &["MAC/65               Version 1.01", "Edit"],
    },
    Expect {
        crc: 0x5193_1A08,
        title: "MAC/65 1.02 (034M)",
        kind: CartridgeKind::OssTwoChipLegacy,
        press_start_at: None,
        frames: 300,
        text: &["MAC/65               Version 1.02", "Edit"],
    },
    Expect {
        crc: 0x5752_D29F,
        title: "BASIC XL 1.02 (043M)",
        kind: CartridgeKind::OssTwoChip,
        press_start_at: None,
        frames: 300,
        text: &["BASIC XL  Version 1.02", "Ready"],
    },
    Expect {
        crc: 0xE8B3_FC3C,
        title: "BASIC XL 1.02 (034M)",
        kind: CartridgeKind::OssTwoChipLegacy,
        press_start_at: None,
        frames: 300,
        text: &["BASIC XL  Version 1.02", "Ready"],
    },
    Expect {
        crc: 0x94A0_5568,
        title: "BASIC XL 1.03 (M091)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: None,
        frames: 300,
        text: &["BASIC XL version 1.03", "Ready"],
    },
    Expect {
        crc: 0x003D_3A36,
        title: "BASIC XE 4.1 (M091)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: None,
        frames: 600,
        text: &["BASIC XE version 4.1", "Ready"],
    },
    Expect {
        crc: 0x3F06_B111,
        title: "BASIC XE 7.2 (M091)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: Some(300),
        frames: 450,
        text: &["BASIC XE version 7.2", "Ready"],
    },
    Expect {
        crc: 0xAE29_8A33,
        title: "Action! 3.5 (043M)",
        kind: CartridgeKind::OssTwoChip,
        press_start_at: None,
        frames: 300,
        text: &["ACTION! (c)1983 ACS"],
    },
    Expect {
        crc: 0xA1F9_0DFD,
        title: "Action! 3.6 one-chip (M091)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: None,
        frames: 300,
        text: &["ACTION! (c)1983 ACS"],
    },
    Expect {
        crc: 0xEB90_5CB4,
        title: "Action! 3.6 two-chip (034M)",
        kind: CartridgeKind::OssTwoChipLegacy,
        press_start_at: None,
        frames: 300,
        text: &["ACTION! (c)1983 ACS"],
    },
    // TOSEC's `[a]` MAC/65 1.01 carries a CART header (type 15). The header
    // path must keep working exactly as before the CRC32 table existed.
    Expect {
        crc: 0x7E66_B2F7,
        title: "MAC/65 1.01 [a] (.car, type 15)",
        kind: CartridgeKind::OssOneChip,
        press_start_at: None,
        frames: 300,
        text: &["MAC/65               Version 1.01", "Edit"],
    },
];

/// The text window as characters. Atari stores *internal* codes rather than
/// ATASCII: `$00-$3F` are ATASCII `$20-$5F` and `$60-$7F` are themselves.
fn screen_text(system: &Atari800xl) -> String {
    let savmsc = u16::from(system.peek(0x58)) | (u16::from(system.peek(0x59)) << 8);
    let mut out = String::with_capacity(24 * 41);
    for row in 0..24u16 {
        for col in 0..40u16 {
            let code = system.peek(savmsc.wrapping_add(row * 40 + col)) & 0x7F;
            out.push(match code {
                0x00..=0x3F => (code + 0x20) as char,
                0x60..=0x7F => code as char,
                _ => ' ',
            });
        }
        out.push('\n');
    }
    out
}

/// `text` as the internal screen codes the display would hold.
fn internal_codes(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|byte| match byte {
            0x20..=0x5F => byte - 0x20,
            other => other,
        })
        .collect()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

struct Fixtures {
    os: Vec<u8>,
    files: HashMap<u32, Vec<u8>>,
}

impl Fixtures {
    fn load() -> Option<Self> {
        let os_path = PathBuf::from(std::env::var_os("EMU198X_ATARI_800XL_OS")?);
        let dir = PathBuf::from(std::env::var_os("EMU198X_ATARI_800XL_OSS_DIR")?);
        let os = std::fs::read(&os_path).ok()?;
        assert_eq!(
            crc32(&os),
            OFFICIAL_XL_OS_CRC32,
            "{} is not the official XL OS rev. 2",
            os_path.display()
        );
        let files = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read(entry.path()).ok())
            .map(|bytes| (crc32(&bytes), bytes))
            .collect();
        Some(Self { os, files })
    }

    fn file(&self, crc: u32, title: &str) -> &[u8] {
        self.files.get(&crc).unwrap_or_else(|| {
            panic!("{title} (CRC32 {crc:08X}) is missing from EMU198X_ATARI_800XL_OSS_DIR")
        })
    }

    /// Cold boot `image` with no override under the XL OS, BASIC off.
    fn boot(&self, image: &[u8]) -> Atari800xl {
        let cart = Cartridge::from_rom(image).expect("cartridge parses");
        Atari800xl::with_cartridge(
            Some(self.os.clone()),
            None,
            Some(cart),
            Atari800xlRegion::Ntsc,
            false,
        )
    }
}

fn run(system: &mut Atari800xl, frames: u32, press_start_at: Option<u32>) {
    for frame in 0..frames {
        if let Some(at) = press_start_at {
            if frame == at {
                system.set_console_keys(true, false, false);
            } else if frame == at + 10 {
                system.set_console_keys(false, false, false);
            }
        }
        system.run_frame();
        assert!(
            !system.cpu().halted,
            "CPU jammed at ${:04X}",
            system.cpu().regs.pc
        );
    }
}

#[test]
#[ignore = "FIXTURE: needs EMU198X_ATARI_800XL_OS (official XL OS) and EMU198X_ATARI_800XL_OSS_DIR (TOSEC OSS dumps)"]
fn headerless_oss_cartridges_boot_to_their_prompt() {
    let Some(fixtures) = Fixtures::load() else {
        emu198x_test_skip::skip!(
            "OSS fixtures not staged (EMU198X_ATARI_800XL_OS, EMU198X_ATARI_800XL_OSS_DIR)"
        );
    };
    let mut failures = Vec::new();
    for expect in CARTS {
        let mut system = fixtures.boot(fixtures.file(expect.crc, expect.title));
        let kind = system.cartridge().map(Cartridge::kind);
        if kind != Some(expect.kind) {
            failures.push(format!(
                "{}: identified as {kind:?}, expected {:?}",
                expect.title, expect.kind
            ));
            continue;
        }
        run(&mut system, expect.frames, expect.press_start_at);
        let text = screen_text(&system);
        if let Some(missing) = expect.text.iter().find(|line| !text.contains(*line)) {
            failures.push(format!(
                "{}: `{missing}` not on screen:\n{text}",
                expect.title
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The Writer's Tool is the one OSS 8 KB board. The cartridge is a dongle
/// whose trailer asks the OS to boot the program disk, and the program then
/// runs its editor from the cartridge's banked window. Without the disk the
/// OS falls through to Self Test, which is correct and proves nothing.
#[test]
#[ignore = "FIXTURE: needs EMU198X_ATARI_800XL_OS (official XL OS) and EMU198X_ATARI_800XL_OSS_DIR (TOSEC OSS dumps)"]
fn writers_tool_runs_its_editor_from_the_oss_8k_board() {
    let Some(fixtures) = Fixtures::load() else {
        emu198x_test_skip::skip!(
            "OSS fixtures not staged (EMU198X_ATARI_800XL_OS, EMU198X_ATARI_800XL_OSS_DIR)"
        );
    };
    let cart = fixtures.file(0x13BC_F201, "The Writer's Tool cartridge");
    let disk = fixtures.file(WRITERS_TOOL_DISK_CRC32, "The Writer's Tool program disk");
    let mut system = fixtures.boot(cart);
    assert_eq!(
        system.cartridge().map(Cartridge::kind),
        Some(CartridgeKind::OssEightK)
    );
    system
        .sio_mut()
        .insert_disk(1, AtrImage::parse(disk).expect("ATR parses"));

    run(&mut system, 500, None);
    let text = screen_text(&system);
    assert!(
        text.contains("the writer's tool") && text.contains("VERSION  2.25"),
        "title screen not shown:\n{text}"
    );

    // The editor draws its status line, `OPTION --> MAIN MENU`, from the
    // ATASCII in the cartridge's fixed bank. Neither the disk nor the
    // cartridge holds it in screen codes, so finding them in RAM means the
    // editor ran and drew it.
    run(&mut system, 1000, None);
    let needle = internal_codes("MAIN MENU");
    assert!(
        system
            .ram()
            .windows(needle.len())
            .any(|window| window == needle),
        "the editor never drew its status line (PC ${:04X})",
        system.cpu().regs.pc
    );
}

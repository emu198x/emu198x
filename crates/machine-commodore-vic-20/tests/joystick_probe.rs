//! VIC-20 joystick register validation: drive each direction + fire and read
//! it back where the CPU sees it — VIA #1 port A ($9111) for up/down/left/fire
//! and VIA #2 port B ($9120) for right, all active-low. Layout per the standard
//! VIC-20 joystick (up=PA2, down=PA3, left=PA4, fire=PA5; right=PB7).
//!
//! Gated `#[ignore]`: needs the KERNAL/BASIC/char ROMs. Run with
//! EMU198X_VIC20_KERNAL / _BASIC / _CHAR set (or the default
//! `~/.emu198x/roms/commodore-vic-20/` paths).

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_commodore_vic_20::{Vic20, Vic20Model, Vic20RamExpansion};

fn rom(var: &str, default_name: &str, len: usize) -> Vec<u8> {
    let path = env::var(var).map(PathBuf::from).unwrap_or_else(|_| {
        let home = env::var("HOME").expect("HOME");
        PathBuf::from(home)
            .join(".emu198x/roms/commodore-vic-20")
            .join(default_name)
    });
    let data = fs::read(&path).unwrap_or_else(|_| panic!("read {} ({})", var, path.display()));
    assert_eq!(data.len(), len, "{var} wrong size");
    data
}

/// Read where the joystick lives, after letting the latches settle.
fn settle_read(sys: &mut Vic20) -> (u8, u8) {
    for _ in 0..2 {
        sys.run_frame();
    }
    (sys.peek(0x9111), sys.peek(0x9120))
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 ROMs — run with --ignored"]
fn joystick_lines_match_the_standard_layout() {
    let kernal = rom("EMU198X_VIC20_KERNAL", "kernal.rom", 8192);
    let basic = rom("EMU198X_VIC20_BASIC", "basic.rom", 8192);
    let char_rom = rom("EMU198X_VIC20_CHAR", "char.rom", 4096);

    let mut sys = Vic20::new(
        kernal,
        basic,
        char_rom,
        Vic20Model::Ntsc,
        Vic20RamExpansion::NONE,
    );
    // Boot to READY so the KERNAL has configured the VIA DDRs.
    for _ in 0..180 {
        sys.run_frame();
    }

    // Idle: every joystick line high (active-low, nothing pressed).
    sys.set_joystick(false, false, false, false, false);
    let (pa, pb) = settle_read(&mut sys);
    assert_eq!(pa & 0x3C, 0x3C, "idle: VIA1 PA2-5 all high, got {pa:02X}");
    assert_eq!(pb & 0x80, 0x80, "idle: VIA2 PB7 high, got {pb:02X}");

    // Each direction pulls its own line low and nothing else on that nibble.
    for (name, up, down, left, right, fire, mask_pa, mask_pb) in [
        ("up", true, false, false, false, false, 0x04u8, 0x00u8),
        ("down", false, true, false, false, false, 0x08, 0x00),
        ("left", false, false, true, false, false, 0x10, 0x00),
        ("fire", false, false, false, false, true, 0x20, 0x00),
        ("right", false, false, false, true, false, 0x00, 0x80),
    ] {
        sys.set_joystick(up, down, left, right, fire);
        let (pa, _) = settle_read(&mut sys);
        if mask_pa != 0 {
            assert_eq!(
                pa & mask_pa,
                0,
                "{name}: VIA1 PA bit {mask_pa:02X} should be low"
            );
        }
        if mask_pb != 0 {
            // VIA2 PB7 is a keyboard-column *output* at READY, so make it an
            // input first — then port B reflects the joystick-right line. Read
            // without an intervening frame so the IRQ keyboard scan can't
            // reconfigure DDRB before the read.
            sys.poke(0x9122, sys.peek(0x9122) & !mask_pb); // DDRB: bit → input
            let pb = sys.peek(0x9120);
            assert_eq!(
                pb & mask_pb,
                0,
                "{name}: VIA2 PB bit {mask_pb:02X} should be low"
            );
            println!("{name:>5}: VIA2 PB=${pb:02X} (DDRB bit cleared)");
        } else {
            println!("{name:>5}: VIA1 PA=${pa:02X}");
        }
    }
}

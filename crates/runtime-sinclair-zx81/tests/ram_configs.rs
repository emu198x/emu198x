//! Each board's RAM reaches the machine, and the ROM agrees about how much.

use std::{env, fs};

use runtime_sinclair_zx81::{Model, Zx81Runtime};

/// `RAMTOP` is the ROM's own answer to "how much memory is there".
///
/// It sits at `$4004` and holds the address one past the last byte, so a
/// board with `n` bytes fitted from `$4000` reports `$4000 + n`. Booting each
/// profile and reading it back is the end-to-end check that a profile's RAM
/// is not merely declared but actually fitted.
///
/// It checks the plumbing, not the figures. The runtime derives RAM from the
/// same constant this asserts against, so changing a board's declared size
/// moves both sides together and this still passes. The sizes themselves are
/// pinned literally in `profiles::tests` — 1,024 for the ZX81 and 2,048 for
/// the TS1000, from the hardware reference — and that is the test that fails
/// if someone changes one.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
fn the_rom_finds_the_ram_each_board_declares() {
    const RAMTOP: u16 = 0x4004;

    let Ok(path) = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
    else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let Ok(rom) = fs::read(&path) else {
        emu198x_test_skip::skip!("ZX81 ROM not staged at {path}");
    };

    for model in Model::ALL {
        let mut runtime = Zx81Runtime::new(model, rom.clone()).expect("runtime");
        let machine = runtime.machine_mut().expect("machine");
        for _ in 0..250 {
            machine.run_frame();
        }
        let ramtop = u16::from(machine.peek_memory(RAMTOP))
            | (u16::from(machine.peek_memory(RAMTOP + 1)) << 8);

        assert_eq!(
            usize::from(ramtop),
            0x4000 + model.ram_bytes(),
            "{}: the ROM should find {} bytes",
            model.display_name(),
            model.ram_bytes(),
        );
    }
}

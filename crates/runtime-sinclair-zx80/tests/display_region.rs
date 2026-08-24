//! The reported raster must follow the board's strap.

use emu198x_shell::display::Display;
use emu198x_shell::display::{NTSC_ACTIVE_LINES, PAL_ACTIVE_LINES};
use emu198x_shell::{MachineCore, Region};
use runtime_sinclair_zx80::{Model, Zx80Runtime};

fn television(model: Model) -> (Region, f64) {
    let runtime = Zx80Runtime::blank(model);
    match runtime.display().expect("ZX80 drives a television") {
        Display::Television {
            region,
            lines_per_tv_height,
            ..
        } => (region, lines_per_tv_height),
        other => panic!("expected a television, got {other:?}"),
    }
}

/// A USA board scans an NTSC set: 240 active lines, not 288.
///
/// Before #1133 there was no USA board to select. `region()` returned
/// `Region::Pal` for everything, so nothing contradicted anything and an
/// extent audit read the machine as 100% — an absent capability rather than a
/// wrong answer, which is why it outlived the ZX81's #1119.
#[test]
fn the_active_line_count_follows_the_strap() {
    assert_eq!(television(Model::Zx80), (Region::Pal, PAL_ACTIVE_LINES));
    assert_eq!(
        television(Model::Zx80RamPack),
        (Region::Pal, PAL_ACTIVE_LINES)
    );
    assert_eq!(
        television(Model::Zx80Usa),
        (Region::Ntsc, NTSC_ACTIVE_LINES)
    );
}

/// And the strap the machine is built with follows the profile.
#[test]
fn the_strap_follows_the_profile() {
    use machine_sinclair_zx80::TelevisionStandard;
    assert_eq!(
        Model::Zx80.television_standard(),
        TelevisionStandard::FiftyHz
    );
    assert_eq!(
        Model::Zx80Usa.television_standard(),
        TelevisionStandard::SixtyHz
    );
}

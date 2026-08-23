//! The reported raster must follow the board's region.

use emu198x_shell::display::Display;
use emu198x_shell::display::{NTSC_ACTIVE_LINES, PAL_ACTIVE_LINES};
use emu198x_shell::{MachineCore, Region};
use runtime_sinclair_zx81::{Model, Zx81Runtime};

fn television(model: Model) -> (Region, f64) {
    let runtime = Zx81Runtime::blank(model);
    match runtime.display().expect("ZX81 drives a television") {
        Display::Television {
            region,
            lines_per_tv_height,
            ..
        } => (region, lines_per_tv_height),
        other => panic!("expected a television, got {other:?}"),
    }
}

/// A 60 Hz board scans an NTSC set: 240 active lines, not PAL's 288.
///
/// This was hardcoded to PAL, so the 60 Hz variant reported an NTSC region
/// with a PAL raster. Nothing fails when that is wrong — the picture is just
/// the wrong shape — which is why it is asserted rather than eyeballed.
#[test]
fn the_active_line_count_follows_the_region() {
    assert_eq!(television(Model::Zx81), (Region::Pal, PAL_ACTIVE_LINES));
    assert_eq!(
        television(Model::Zx81Ntsc),
        (Region::Ntsc, NTSC_ACTIVE_LINES)
    );
}

//! The variant enum must forward every `MachineCore` method it is asked for.
//!
//! `AmigaRuntimeKind` dispatches to one of three chip stacks by hand, method
//! by method. Most of the trait has no default, so a forgotten method fails to
//! compile — but the ones with defaults do not. `display` was missed for
//! exactly that reason: the enum compiled, answered `None`, and the Amiga
//! reported no display at all while its inner runtimes each stated one. The
//! harness then fell back to square pixels on a machine whose pixels are not.
//!
//! Found by the #1054 audit, which read `session.display.kind` across the
//! fleet and got null for one machine out of thirty.

use emu198x_shell::MachineCore;
use emu198x_shell::display::Display;
use runtime_commodore_amiga::{AmigaRuntimeKind, Model};

/// One model per chip stack — the three arms the enum dispatches over.
const ONE_PER_STACK: [Model; 3] = [Model::A500OcsPal, Model::A600EcsPal, Model::A1200AgaPal];

#[test]
fn every_chip_stack_states_its_display_through_the_variant_enum() {
    for model in ONE_PER_STACK {
        let runtime = AmigaRuntimeKind::blank(model);

        let display = runtime
            .display()
            .unwrap_or_else(|| panic!("{model:?} states no display through the variant enum"));

        // A television specifically, and with usable numbers: the failure this
        // guards against answers `None`, but a half-forwarded one could answer
        // a default-constructed television that divides by zero downstream.
        let Display::Television {
            pixel_clock_hz,
            lines_per_tv_height,
            ..
        } = display
        else {
            panic!("{model:?} drives a television, not {display:?}");
        };
        assert!(
            pixel_clock_hz > 0.0 && lines_per_tv_height > 0.0,
            "{model:?} states an unusable raster: {pixel_clock_hz} Hz over {lines_per_tv_height} lines"
        );
    }
}

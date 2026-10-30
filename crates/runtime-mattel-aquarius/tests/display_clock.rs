//! Presentation must describe the clock the selected engine actually runs.
use emu198x_shell::{MachineCore, display::Display};
use runtime_mattel_aquarius::{AquariusRuntime, Model};
#[test]
fn television_dot_rate_matches_native_frame_timing_and_profile_clock() {
    for model in Model::ALL {
        let runtime = AquariusRuntime::blank(model);
        let rate = runtime.profile().clock.rate;
        let cpu_hz = rate.numerator_hz as f64 / rate.denominator_hz as f64;
        let native_cpu_hz = model.frame_ticks() as f64 * model.machine_region().frame_hz();
        assert!(
            (cpu_hz - native_cpu_hz).abs() < 1e-6,
            "{model:?}: profile and engine clocks disagree"
        );
        let Some(Display::Television {
            region,
            pixel_clock_hz,
            ..
        }) = runtime.display()
        else {
            panic!("television")
        };
        assert_eq!(region, runtime.profile().region);
        assert!(
            (pixel_clock_hz - 2.0 * native_cpu_hz).abs() < 1e-6,
            "{model:?}: display dot rate {pixel_clock_hz} disagrees with native timing {}",
            2.0 * native_cpu_hz
        );
    }
}

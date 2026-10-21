//! Intentionally exits unsuccessfully while programmed SRT has no effect.
//! Standalone diagnostic, outside the workspace test suite; not a passing gate.
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use nec_upd765a::Upd765a;

fn command(fdc: &mut Upd765a, bytes: &[u8]) {
    for &byte in bytes {
        assert_eq!(fdc.read_status() & 0xc0, 0x80, "command write readiness");
        fdc.write_data(byte);
    }
}

fn measure(srt: u8, target: u8) -> u32 {
    let mut fdc = Upd765a::new();
    fdc.enabled = true;
    fdc.set_ready(0, true); // Supplied digital READY; no media/firmware required.
    // Allow multiple full idle READY polling rounds, then drain every event.
    for tick in 0..100_000 {
        fdc.tick(tick);
    }
    for _ in 0..5 {
        command(&mut fdc, &[0x08]);
        let st0 = fdc.read_data();
        if st0 == 0x80 {
            break;
        }
        assert_eq!(st0, 0xc0, "only drive-0 READY became asserted");
        assert_eq!(fdc.read_data(), 0, "initial PCN");
    }
    assert_eq!(fdc.read_status(), 0x80);
    assert!(!fdc.interrupt, "READY attention was acknowledged");
    command(&mut fdc, &[0x03, srt << 4, 0x03]);
    assert_eq!(fdc.read_status(), 0x80, "SPECIFY has no result");
    assert!(!fdc.interrupt, "SPECIFY has no completion IRQ");
    command(&mut fdc, &[0x0f, 0, target]);
    assert!(!fdc.interrupt, "seek must not inherit READY IRQ");
    let elapsed = (1..=2_000_000)
        .find(|tick| {
            fdc.tick(*tick);
            fdc.interrupt
        })
        .expect("bounded seek completion");
    // Busy stays asserted until SIS consumes ST0. It is not the timing oracle.
    assert_eq!(fdc.read_status(), 0x81);
    command(&mut fdc, &[0x08]);
    let result = [fdc.read_data(), fdc.read_data()];
    assert_eq!(
        result,
        [0x20, target],
        "normal seek completion and exact PCN"
    );
    assert_eq!(fdc.read_status(), 0x80, "SIS clears drive busy");
    assert!(!fdc.interrupt);
    println!(
        "{{\"srt\":{srt},\"target\":{target},\"completion_ticks\":{elapsed},\"st0\":32,\"pcn\":{target}}}"
    );
    elapsed
}

fn main() {
    // Two-step journeys also observe an adjacent-step interval, independently
    // of uncertainty about the first pulse/one-step completion offset.
    let slow_one = measure(1, 1);
    let fast_one = measure(15, 1);
    let slow_two = measure(1, 2);
    let fast_two = measure(15, 2);
    assert!(
        slow_one > fast_one,
        "SPECIFY SRT=1 must take longer than SRT=F; got {slow_one} and {fast_one}"
    );
    assert!(
        slow_two > fast_two,
        "programmed adjacent-step spacing must differ"
    );
    assert!(
        slow_two > slow_one && fast_two > fast_one,
        "additional steps must consume programmed step time"
    );
}

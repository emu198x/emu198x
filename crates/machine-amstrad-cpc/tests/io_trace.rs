//! The I/O trace records whole addresses, because the CPC's device select
//! lives in the high byte.
//!
//! The CPC decodes I/O on A15-A10. `$7F00` is the Gate Array, `$BC00` /
//! `$BD00` the CRTC, `$F400`-`$F700` the PPI — every one with a low byte of
//! zero. A trace that kept only the low byte reported all of them as port 0,
//! which is why this machine had no I/O trace at all until #926.
//!
//! Driven by executing real `OUT (C),A` instructions rather than by calling
//! [`AmstradCpc::out`]. The trace records the **CPU's** bus activity, so a
//! test that poked the machine's own helper would prove nothing about the
//! path a debugger actually observes.
//!
//! No firmware needed: the code runs from RAM at `$8000`.

use machine_amstrad_cpc::AmstradCpc;

const CODE: u16 = 0x8000;

fn stub() -> Vec<u8> {
    vec![0; 0x8000]
}

/// `LD A,v` / `LD BC,port` / `OUT (C),A` for each pair, then `HALT`.
fn program(accesses: &[(u16, u8)]) -> Vec<u8> {
    let mut code = Vec::new();
    for &(port, value) in accesses {
        code.extend_from_slice(&[0x3E, value]); // LD A,value
        code.extend_from_slice(&[0x01, (port & 0xFF) as u8, (port >> 8) as u8]); // LD BC,port
        code.extend_from_slice(&[0xED, 0x79]); // OUT (C),A
    }
    code.push(0x76); // HALT
    code
}

/// Run the program from `CODE`, with the trace on, and return what it caught.
fn trace_of(accesses: &[(u16, u8)]) -> Vec<(u16, u8, bool)> {
    let mut cpc = AmstradCpc::new(&stub()).expect("32 KB stub");
    for (i, b) in program(accesses).iter().enumerate() {
        cpc.poke(CODE + u16::try_from(i).expect("fits"), *b);
    }
    // Enter on an instruction boundary; writing PC mid-instruction lets the
    // in-flight instruction finish against the new PC (#943).
    let mut guard = 0;
    while !cpc.z80().instruction_complete() {
        cpc.advance_tstates(1);
        guard += 1;
        assert!(guard < 256, "no instruction boundary within 256 t-states");
    }
    cpc.z80_mut().regs.pc = CODE;
    cpc.start_io_trace();

    let want = accesses.len() as u64 * 3 + 1;
    let start = cpc.z80().instructions_retired();
    let mut spins = 0;
    while cpc.z80().instructions_retired() - start < want {
        cpc.advance_tstates(1);
        spins += 1;
        assert!(spins < 100_000, "the program never reached its HALT");
    }
    cpc.take_io_trace()
        .into_iter()
        .map(|e| (e.port, e.value, e.write))
        .collect()
}

/// Two devices, two low bytes of zero, two distinguishable events.
#[test]
fn the_gate_array_and_the_crtc_are_told_apart() {
    let got = trace_of(&[(0x7F00, 0x8C), (0xBC00, 0x01), (0xBD00, 0x28)]);
    assert_eq!(
        got,
        vec![
            (0x7F00, 0x8C, true),
            (0xBC00, 0x01, true),
            (0xBD00, 0x28, true)
        ],
        "the trace should name the device, not report every one as port 0 — \
         which is what an 8-bit port did before #926"
    );
}

/// Truncating to the low byte collapses them onto one port. Stated as a test
/// so the regression is visible rather than implied.
#[test]
fn the_low_byte_alone_would_lose_them() {
    let got = trace_of(&[(0x7F00, 0x8C), (0xBC00, 0x01), (0xF400, 0x00)]);
    let low: Vec<u8> = got.iter().map(|&(p, _, _)| (p & 0xFF) as u8).collect();
    assert_eq!(
        low,
        vec![0x00, 0x00, 0x00],
        "the Gate Array, the CRTC and the PPI share a low byte of zero"
    );
    let high: Vec<u8> = got.iter().map(|&(p, _, _)| (p >> 8) as u8).collect();
    assert_eq!(
        high,
        vec![0x7F, 0xBC, 0xF4],
        "and are told apart only by the high byte"
    );
}

/// The trace is off until asked for, and stops when taken.
#[test]
fn tracing_is_opt_in_and_ends_when_taken() {
    let mut cpc = AmstradCpc::new(&stub()).expect("32 KB stub");
    for (i, b) in program(&[(0x7F00, 0x8C)]).iter().enumerate() {
        cpc.poke(CODE + u16::try_from(i).expect("fits"), *b);
    }
    while !cpc.z80().instruction_complete() {
        cpc.advance_tstates(1);
    }
    cpc.z80_mut().regs.pc = CODE;

    // Not started: the same program is run and nothing is captured.
    for _ in 0..200 {
        cpc.advance_tstates(1);
    }
    assert!(
        cpc.take_io_trace().is_empty(),
        "nothing should be captured before `start_io_trace`"
    );
}

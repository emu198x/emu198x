//! The ZX81 makes a noise, and it is the display making it.
//!
//! There is no sound chip. Thomasson, p13: *"the same pin on the ULA provides
//! both the video signal and the output to the tape recorder, hence the odd
//! patterns on the screen when tape is in use."* So the sound a host hears is
//! that pin — line sync while a picture is being generated, and the `SAVE`
//! waveform when the ROM is driving the software sync instead.

use machine_sinclair_zx81::{Zx81, Zx81Key};
use std::{env, fs};

/// Transitions in a run of samples: what makes a tone a tone.
fn transitions(samples: &[f32]) -> usize {
    samples.windows(2).filter(|w| w[0] != w[1]).count()
}

fn rom() -> Option<Vec<u8>> {
    let path = env::var("EMU198X_ZX81_ROM")
        .or_else(|_| env::var("HOME").map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom")))
        .ok()?;
    fs::read(path).ok()
}

/// A frame's worth of samples arrives, at the rate the sink is told.
///
/// 48 kHz against a frame a little over 64,000 T-states is about 960 samples.
/// Asserted as a range because the ZX81's frame length is the ROM's business
/// rather than a constant.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — set EMU198X_ZX81_ROM"]
fn a_frame_produces_a_frames_worth_of_samples() {
    let Some(rom) = rom() else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..200 {
        machine.run_frame();
    }
    let _ = machine.take_audio_buffer();
    machine.run_frame();
    let samples = machine.take_audio_buffer();
    assert!(
        (900..1000).contains(&samples.len()),
        "a 50 Hz frame at 48 kHz is about 960 samples, not {}",
        samples.len()
    );
    assert!(
        machine.take_audio_buffer().is_empty(),
        "taking the buffer should empty it"
    );
}

/// The buzz: at rest the pin carries line sync, so the sink gets a tone
/// rather than silence.
///
/// This is the whole point of #303. The drivability survey called the ZX81 a
/// near-silent machine with non-empty captures, and the captures were
/// non-empty because the sink was being handed an empty slice.
///
/// # What the rate should be, and why it is not the line rate
///
/// The obvious guess is 15.7 kHz — a 207 T-state line — and it is wrong. Sync
/// is only 15 of those 207 T-states, and a 48 kHz sink samples every 68, so
/// most sync pulses fall *between* samples and are never seen. What survives
/// is the duty cycle: about 15/207 of samples land low, each one isolated, and
/// each isolated low sample is two transitions.
///
///   48,000 x 15/207 x 2 = about 6,960 transitions a second
///
/// which is what this asserts, and what it measures. The band is derived from
/// that arithmetic rather than fitted to the result.
///
/// That aliasing is inherent to sampling a narrow pulse train at 48 kHz, not a
/// defect in the model — but it does mean the sink receives the *structure* of
/// the signal rather than its spectrum. Band-limiting before the sink would be
/// the honest fix and is not attempted here.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — set EMU198X_ZX81_ROM"]
fn the_display_buzzes_at_the_sync_duty_cycle() {
    let Some(rom) = rom() else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..200 {
        machine.run_frame();
    }
    let _ = machine.take_audio_buffer();
    machine.run_frame();
    let samples = machine.take_audio_buffer();

    assert!(
        samples.iter().any(|s| *s > 0.0) && samples.iter().any(|s| *s < 0.0),
        "the sink should get a waveform, not a constant"
    );

    let low_at_rest = samples.iter().filter(|s| **s < 0.0).count() as f64 / samples.len() as f64;
    assert!(
        (0.02..0.20).contains(&low_at_rest),
        "at rest the pin is low only for sync -- 15 T-states of every 207, so \
         about 7% of samples; it was {low_at_rest:.3}"
    );

    // 15 T-states of sync in a 207 T-state line, two transitions per isolated
    // low sample.
    let predicted = 48_000 * 15 * 2 / 207;
    let measured = transitions(&samples) * 50;
    assert!(
        measured * 4 > predicted * 3 && measured * 3 < predicted * 4,
        "the tone should sit near the sync duty cycle: predicted about \
         {predicted} transitions a second, measured about {measured}"
    );
}

/// And during `SAVE` it is the tape waveform on the same pin.
///
/// The discriminator is the *duty cycle*, not the rate. At rest the pin is low
/// only for sync — about 15 T-states of every 207, so 7% of samples. While the
/// ROM is saving it drives that same line directly and holds it low most of
/// the time, which measures around 90%.
///
/// Asserting the rate instead would not test anything: a model that ignored
/// the software sync entirely still gives a plausible-looking 7,360
/// transitions a second during a save, because it is just reporting the line
/// sync underneath. Tried, and it passed.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — set EMU198X_ZX81_ROM"]
fn saving_puts_the_tape_signal_on_the_same_pin() {
    let Some(rom) = rom() else {
        emu198x_test_skip::skip!("no ZX81 ROM");
    };
    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..400 {
        machine.run_frame();
    }
    let tap = |m: &mut Zx81, k: Zx81Key| {
        m.press_key(k);
        for _ in 0..25 {
            m.run_frame();
        }
        m.release_key(k);
        for _ in 0..120 {
            m.run_frame();
        }
    };
    let shifted = |m: &mut Zx81, k: Zx81Key| {
        m.press_key(Zx81Key::Shift);
        m.press_key(k);
        for _ in 0..25 {
            m.run_frame();
        }
        m.release_key(k);
        m.release_key(Zx81Key::Shift);
        for _ in 0..120 {
            m.run_frame();
        }
    };

    tap(&mut machine, Zx81Key::N1);
    tap(&mut machine, Zx81Key::E);
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);
    for _ in 0..120 {
        machine.run_frame();
    }
    tap(&mut machine, Zx81Key::S);
    shifted(&mut machine, Zx81Key::P);
    tap(&mut machine, Zx81Key::A);
    shifted(&mut machine, Zx81Key::P);

    let _ = machine.take_audio_buffer();
    machine.press_key(Zx81Key::Newline);
    for _ in 0..25 {
        machine.run_frame();
    }
    machine.release_key(Zx81Key::Newline);
    for _ in 0..300 {
        machine.run_frame();
    }
    let saving = machine.take_audio_buffer();

    assert!(
        saving.iter().any(|s| *s > 0.0) && saving.iter().any(|s| *s < 0.0),
        "SAVE should put a signal on the pin"
    );
    let low_while_saving = saving.iter().filter(|s| **s < 0.0).count() as f64 / saving.len() as f64;
    assert!(
        low_while_saving > 0.5,
        "while saving, the ROM drives this pin and holds it low most of the \
         time; it was low for {low_while_saving:.3} of the samples, which is \
         the idle sync duty cycle rather than a save"
    );
}

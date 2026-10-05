//! The window's sound output: the host audio device, or nothing.
//!
//! Sound is optional for the window. A machine with no usable output device
//! (a remote desktop, a VM, a minimal Linux install with no ALSA default)
//! still runs the emulator, silently, after one warning. Frame pacing never
//! depends on the device: the window paces itself against the wall clock
//! (`App::next_slice_at`), so a silent run keeps the machine's own speed.
//! Audio capture (`--audio-capture`, MCP recording) runs in the headless
//! session, which never opens the device either.

use emu198x_shell::{
    AudioPacket, AudioSink, MachineError, NativeAudioError, NativeAudioOutput, NullAudioSink,
};

/// Where the window sends the machine's audio.
pub(crate) enum HostAudio {
    /// Playing through the host's default output device.
    Device(NativeAudioOutput),
    /// No output: `--no-audio` was given, or the device could not be opened.
    Silent,
}

impl HostAudio {
    /// Open the output device through `open` when `enabled`, or run silent.
    ///
    /// A failure to open the device is a warning, not an error: the window
    /// runs on without sound. `open` is never called when `enabled` is
    /// false, so `--no-audio` touches no audio API at all.
    pub(crate) fn open(
        enabled: bool,
        open: impl FnOnce() -> Result<NativeAudioOutput, NativeAudioError>,
    ) -> Self {
        if !enabled {
            return Self::Silent;
        }
        match open() {
            Ok(output) => Self::Device(output),
            Err(err) => {
                eprintln!("warning: {}", device_warning(&err));
                Self::Silent
            }
        }
    }

    /// Drop queued host audio (after a reset, load, or turbo burst).
    pub(crate) fn clear(&mut self) {
        if let Self::Device(output) = self {
            output.clear();
        }
    }
}

impl AudioSink for HostAudio {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        match self {
            Self::Device(output) => output.push_audio(packet),
            Self::Silent => NullAudioSink.push_audio(packet),
        }
    }
}

/// The one-line warning printed when the device cannot be opened.
fn device_warning(err: &NativeAudioError) -> String {
    format!("running without sound: {err} (pass --no-audio to run silently on purpose)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(samples: &[f32]) -> AudioPacket<'_> {
        AudioPacket {
            timestamp: Default::default(),
            sample_rate: 44_100,
            channels: 1,
            samples,
        }
    }

    /// #1576: a host with no usable output device runs silent instead of
    /// ending the session before the window opens.
    #[test]
    fn a_device_that_fails_to_open_leaves_the_window_silent() {
        let mut audio = HostAudio::open(true, || Err(NativeAudioError::NoDefaultOutputDevice));

        assert!(matches!(audio, HostAudio::Silent));
        audio
            .push_audio(packet(&[0.25, -0.25]))
            .expect("a silent sink accepts audio");
        audio.clear();
    }

    #[test]
    fn no_audio_never_opens_the_device() {
        let mut opened = false;
        let audio = HostAudio::open(false, || {
            opened = true;
            Err(NativeAudioError::NoDefaultOutputDevice)
        });

        assert!(!opened, "--no-audio must not call into the audio API");
        assert!(matches!(audio, HostAudio::Silent));
    }

    #[test]
    fn the_warning_names_the_cause_and_the_flag() {
        let warning = device_warning(&NativeAudioError::NoDefaultOutputDevice);

        assert!(warning.starts_with("running without sound: "));
        assert!(warning.contains("no default output device is available"));
        assert!(warning.contains("--no-audio"));
    }
}

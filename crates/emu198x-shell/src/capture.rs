//! Shared headless capture helpers for screenshots and audio export.

use std::io::Cursor;

use thiserror::Error;

use crate::error::MachineError;
use crate::host::{AudioPacket, AudioSink, FramePacket, FrameSink, PixelFormat};
use crate::time::MachineTime;

/// Capture-time encoding or extraction failure.
#[derive(Debug, Error)]
pub enum CaptureError {
    /// No frame has been captured yet.
    #[error("no frame has been captured")]
    MissingFrame,

    /// No audio has been captured yet.
    #[error("no audio has been captured")]
    MissingAudio,

    /// Indexed capture requires a palette.
    #[error("indexed frame is missing a palette")]
    MissingPalette,

    /// The frame payload length does not match the declared geometry.
    #[error(
        "frame data length {actual} does not match expected {expected} for {format:?} {width}x{height}"
    )]
    InvalidFrameData {
        /// Declared pixel format.
        format: PixelFormat,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
        /// Expected raw byte count.
        expected: usize,
        /// Actual raw byte count.
        actual: usize,
    },

    /// One indexed pixel references a palette entry that does not exist.
    #[error("palette index {index} is out of range for palette length {palette_len}")]
    InvalidPaletteIndex {
        /// Offending palette index.
        index: u8,
        /// Available palette entries.
        palette_len: usize,
    },

    /// The PNG encoder rejected the generated RGBA frame.
    #[error("png encoding failed: {0}")]
    PngEncoding(#[from] png::EncodingError),

    /// A streaming WAV write or header fix-up hit an I/O error.
    #[error("wav stream io failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Owned copy of one emitted frame.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedFrame {
    /// Machine timestamp associated with the frame.
    pub timestamp: MachineTime,
    /// Pixel layout.
    pub format: PixelFormat,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Palette entries for indexed formats.
    pub palette: Option<Vec<u32>>,
    /// Raw frame bytes in the declared pixel format.
    pub pixels: Vec<u8>,
}

impl CapturedFrame {
    /// Creates an owned frame copy from one emitted packet.
    #[must_use]
    pub fn from_packet(packet: FramePacket<'_>) -> Self {
        Self {
            timestamp: packet.timestamp,
            format: packet.format,
            width: packet.width,
            height: packet.height,
            palette: packet.palette.map(ToOwned::to_owned),
            pixels: packet.pixels.to_vec(),
        }
    }

    /// Converts the frame into raw RGBA8888 pixels.
    ///
    /// # Errors
    ///
    /// Returns an error if the raw frame payload is malformed or if an indexed
    /// frame references palette data that is missing or out of range.
    pub fn rgba_pixels(&self) -> Result<Vec<u8>, CaptureError> {
        let pixel_count = usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .map(|height| width * height)
            })
            .ok_or(CaptureError::InvalidFrameData {
                format: self.format,
                width: self.width,
                height: self.height,
                expected: usize::MAX,
                actual: self.pixels.len(),
            })?;

        match self.format {
            PixelFormat::Indexed8 => {
                if self.pixels.len() != pixel_count {
                    return Err(CaptureError::InvalidFrameData {
                        format: self.format,
                        width: self.width,
                        height: self.height,
                        expected: pixel_count,
                        actual: self.pixels.len(),
                    });
                }

                let palette = self.palette.as_ref().ok_or(CaptureError::MissingPalette)?;
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for &index in &self.pixels {
                    let entry =
                        palette
                            .get(index as usize)
                            .ok_or(CaptureError::InvalidPaletteIndex {
                                index,
                                palette_len: palette.len(),
                            })?;
                    rgba.extend_from_slice(&rgba_u32_to_bytes(*entry));
                }
                Ok(rgba)
            }
            PixelFormat::Rgba8888 => {
                let expected = pixel_count * 4;
                if self.pixels.len() != expected {
                    return Err(CaptureError::InvalidFrameData {
                        format: self.format,
                        width: self.width,
                        height: self.height,
                        expected,
                        actual: self.pixels.len(),
                    });
                }
                Ok(self.pixels.clone())
            }
        }
    }

    /// Encodes the frame as PNG.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame cannot be converted into RGBA pixels or
    /// if the PNG encoder rejects the output.
    pub fn png_bytes(&self) -> Result<Vec<u8>, CaptureError> {
        let rgba = self.rgba_pixels()?;
        let mut cursor = Cursor::new(Vec::new());
        let mut encoder = png::Encoder::new(&mut cursor, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        drop(writer);
        Ok(cursor.into_inner())
    }
}

/// Captures the most recent emitted frame.
#[derive(Debug, Default)]
pub struct LatestFrameCapture {
    frame: Option<CapturedFrame>,
}

impl LatestFrameCapture {
    /// Returns the most recently captured frame.
    #[must_use]
    pub fn frame(&self) -> Option<&CapturedFrame> {
        self.frame.as_ref()
    }

    /// Encodes the most recently captured frame as PNG.
    ///
    /// # Errors
    ///
    /// Returns an error if no frame was captured or if PNG encoding fails.
    pub fn png_bytes(&self) -> Result<Vec<u8>, CaptureError> {
        self.frame
            .as_ref()
            .ok_or(CaptureError::MissingFrame)?
            .png_bytes()
    }
}

impl FrameSink for LatestFrameCapture {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.frame = Some(CapturedFrame::from_packet(frame));
        Ok(())
    }
}

/// Owned copy of one captured audio stream.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedAudio {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of interleaved channels.
    pub channels: u8,
    /// Interleaved audio samples.
    pub samples: Vec<f32>,
}

impl CapturedAudio {
    /// Encodes the stream as 16-bit PCM WAV.
    #[must_use]
    pub fn wav_bytes(&self) -> Vec<u8> {
        let bytes_per_sample = 2u16;
        let block_align = u16::from(self.channels) * bytes_per_sample;
        let byte_rate = self.sample_rate * u32::from(block_align);
        let data_len =
            u32::try_from(self.samples.len() * usize::from(bytes_per_sample)).unwrap_or(u32::MAX);
        let riff_len = 36u32.saturating_add(data_len);

        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&riff_len.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&u16::from(self.channels).to_le_bytes());
        wav.extend_from_slice(&self.sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());

        for &sample in &self.samples {
            wav.extend_from_slice(&f32_to_pcm16(sample).to_le_bytes());
        }

        wav
    }
}

/// Captures all emitted audio packets into one stream.
#[derive(Debug, Default)]
pub struct AudioCapture {
    audio: Option<CapturedAudio>,
}

impl AudioCapture {
    /// Returns the captured stream when any packets have been seen.
    #[must_use]
    pub fn audio(&self) -> Option<&CapturedAudio> {
        self.audio.as_ref()
    }

    /// Encodes the captured stream as 16-bit PCM WAV.
    ///
    /// # Errors
    ///
    /// Returns an error if no audio has been captured yet.
    pub fn wav_bytes(&self) -> Result<Vec<u8>, CaptureError> {
        Ok(self
            .audio
            .as_ref()
            .ok_or(CaptureError::MissingAudio)?
            .wav_bytes())
    }

    /// Drops the first `n` interleaved samples from the front of the
    /// buffer. Used by the session to bound RAM during a streaming audio
    /// recording: once a prefix has been flushed to disk (and no other
    /// consumer still needs it), it is drained here. Caller is responsible
    /// for shifting any retained sample offsets by `n`.
    pub fn drain_prefix(&mut self, n: usize) {
        if let Some(audio) = &mut self.audio {
            let n = n.min(audio.samples.len());
            audio.samples.drain(0..n);
        }
    }
}

impl AudioSink for AudioCapture {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        match &mut self.audio {
            Some(audio) => {
                if audio.sample_rate != packet.sample_rate || audio.channels != packet.channels {
                    return Err(MachineError::Host {
                        reason: format!(
                            "audio capture stream changed format from {} Hz/{} ch to {} Hz/{} ch",
                            audio.sample_rate, audio.channels, packet.sample_rate, packet.channels
                        ),
                    });
                }

                audio.samples.extend_from_slice(packet.samples);
            }
            None => {
                self.audio = Some(CapturedAudio {
                    sample_rate: packet.sample_rate,
                    channels: packet.channels,
                    samples: packet.samples.to_vec(),
                });
            }
        }

        Ok(())
    }
}

/// Streams a 16-bit PCM WAV to disk incrementally, so a long audio
/// recording doesn't have to hold every sample in RAM until the end.
///
/// Writes a 44-byte header with placeholder (zero) lengths up front,
/// appends PCM frames as they arrive via [`Self::append`], and patches
/// the RIFF / `data` chunk sizes in the header on [`Self::finish`] (which
/// seeks back to the two length fields). The on-disk result is a normal
/// canonical WAV — identical bytes to [`CapturedAudio::wav_bytes`] for the
/// same samples — produced without buffering the whole stream.
pub struct WavStreamWriter {
    file: std::io::BufWriter<std::fs::File>,
    sample_rate: u32,
    channels: u8,
    /// Interleaved samples appended so far.
    samples_written: u64,
}

impl WavStreamWriter {
    /// Creates `path` and writes the placeholder header (lengths zeroed;
    /// patched by [`Self::finish`]).
    ///
    /// # Errors
    /// Returns [`CaptureError::Io`] if the file can't be created or the
    /// header can't be written.
    pub fn create(
        path: &std::path::Path,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Self, CaptureError> {
        let file = std::fs::File::create(path)?;
        let mut writer = Self {
            file: std::io::BufWriter::new(file),
            sample_rate,
            channels,
            samples_written: 0,
        };
        writer.write_header(0)?;
        Ok(writer)
    }

    fn write_header(&mut self, data_len: u32) -> Result<(), CaptureError> {
        use std::io::Write;
        let block_align = u16::from(self.channels) * 2;
        let byte_rate = self.sample_rate * u32::from(block_align);
        let riff_len = 36u32.saturating_add(data_len);
        let f = &mut self.file;
        f.write_all(b"RIFF")?;
        f.write_all(&riff_len.to_le_bytes())?;
        f.write_all(b"WAVE")?;
        f.write_all(b"fmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?;
        f.write_all(&u16::from(self.channels).to_le_bytes())?;
        f.write_all(&self.sample_rate.to_le_bytes())?;
        f.write_all(&byte_rate.to_le_bytes())?;
        f.write_all(&block_align.to_le_bytes())?;
        f.write_all(&16u16.to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&data_len.to_le_bytes())?;
        Ok(())
    }

    /// Appends interleaved samples, each encoded little-endian PCM16.
    ///
    /// # Errors
    /// Returns [`CaptureError::Io`] on a write failure.
    pub fn append(&mut self, samples: &[f32]) -> Result<(), CaptureError> {
        use std::io::Write;
        for &sample in samples {
            self.file.write_all(&f32_to_pcm16(sample).to_le_bytes())?;
        }
        self.samples_written = self.samples_written.saturating_add(samples.len() as u64);
        Ok(())
    }

    /// Interleaved samples appended so far.
    #[must_use]
    pub fn samples_written(&self) -> u64 {
        self.samples_written
    }

    /// Sample rate the stream was opened with, in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count the stream was opened with.
    #[must_use]
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Flushes, seeks back to patch the RIFF / `data` length fields, and
    /// returns the interleaved sample count written.
    ///
    /// # Errors
    /// Returns [`CaptureError::Io`] on a flush, seek, or write failure.
    pub fn finish(mut self) -> Result<u64, CaptureError> {
        use std::io::{Seek, SeekFrom, Write};
        let data_len = u32::try_from(self.samples_written.saturating_mul(2)).unwrap_or(u32::MAX);
        let riff_len = 36u32.saturating_add(data_len);
        self.file.flush()?;
        // RIFF chunk size lives at byte offset 4.
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&riff_len.to_le_bytes())?;
        // `data` chunk size lives at byte offset 40.
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&data_len.to_le_bytes())?;
        self.file.flush()?;
        Ok(self.samples_written)
    }
}

fn rgba_u32_to_bytes(value: u32) -> [u8; 4] {
    [
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]
}

fn f32_to_pcm16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped <= -1.0 {
        i16::MIN
    } else {
        (clamped * f32::from(i16::MAX)).round() as i16
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn latest_frame_capture_encodes_indexed_png() {
        let mut capture = LatestFrameCapture::default();
        capture
            .push_frame(FramePacket {
                timestamp: MachineTime::new(1),
                format: PixelFormat::Indexed8,
                width: 2,
                height: 1,
                palette: Some(&[0xFF0000FF, 0x00FF00FF]),
                pixels: &[0, 1],
            })
            .expect("frame capture should accept indexed frame");

        let png = capture.png_bytes().expect("png should encode");
        let decoder = png::Decoder::new(Cursor::new(png));
        let mut reader = decoder.read_info().expect("png should decode");
        // png 0.18 changed `output_buffer_size()` to return Option<usize>
        // (None when the frame size would overflow a usize). Our test
        // image is 2×1, so unwrapping is safe — the .expect surfaces a
        // clear panic if a future bump changes the semantics again.
        let mut buf = vec![
            0;
            reader
                .output_buffer_size()
                .expect("buffer size fits in usize")
        ];
        let info = reader
            .next_frame(&mut buf)
            .expect("png frame should decode");

        assert_eq!(info.width, 2);
        assert_eq!(info.height, 1);
        assert_eq!(&buf[..8], &[0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn latest_frame_capture_rejects_missing_palette() {
        let mut capture = LatestFrameCapture::default();
        capture
            .push_frame(FramePacket {
                timestamp: MachineTime::new(1),
                format: PixelFormat::Indexed8,
                width: 1,
                height: 1,
                palette: None,
                pixels: &[0],
            })
            .expect("frame capture should store the packet");

        let result = capture.png_bytes();
        assert!(matches!(result, Err(CaptureError::MissingPalette)));
    }

    #[test]
    fn audio_capture_encodes_wav() {
        let mut capture = AudioCapture::default();
        capture
            .push_audio(AudioPacket {
                timestamp: MachineTime::new(1),
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0, 1.0, -1.0],
            })
            .expect("audio capture should accept first packet");

        let wav = capture.wav_bytes().expect("wav should encode");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn audio_capture_rejects_format_changes() {
        let mut capture = AudioCapture::default();
        capture
            .push_audio(AudioPacket {
                timestamp: MachineTime::new(1),
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0],
            })
            .expect("audio capture should accept first packet");

        let result = capture.push_audio(AudioPacket {
            timestamp: MachineTime::new(2),
            sample_rate: 48_000,
            channels: 1,
            samples: &[0.0],
        });

        assert!(matches!(result, Err(MachineError::Host { .. })));
    }

    /// The incrementally-streamed WAV is byte-for-byte identical to the
    /// one the in-memory encoder produces for the same samples — so the
    /// bounded-RAM path is a drop-in for the buffered one.
    #[test]
    fn wav_stream_writer_matches_buffered_encoder() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) / 500.0 - 1.0).collect();
        let sample_rate = 44_100;
        let channels = 2u8;

        let path = std::env::temp_dir().join(format!(
            "emu198x_wavstream_{}_{}.wav",
            std::process::id(),
            samples.len()
        ));

        let mut writer =
            WavStreamWriter::create(&path, sample_rate, channels).expect("create stream writer");
        // Append in several chunks — the streaming path must not depend on
        // seeing the whole stream at once.
        for chunk in samples.chunks(64) {
            writer.append(chunk).expect("append chunk");
        }
        let written = writer.finish().expect("finish patches the header");
        assert_eq!(written, samples.len() as u64);

        let streamed = std::fs::read(&path).expect("read streamed wav");
        let _ = std::fs::remove_file(&path);

        let buffered = CapturedAudio {
            sample_rate,
            channels,
            samples,
        }
        .wav_bytes();

        assert_eq!(
            streamed, buffered,
            "streamed WAV must equal the buffered encoder output"
        );
    }

    /// An empty recording still produces a valid (zero-data) WAV header.
    #[test]
    fn wav_stream_writer_finishes_empty() {
        let path = std::env::temp_dir().join(format!(
            "emu198x_wavstream_empty_{}.wav",
            std::process::id()
        ));
        let writer = WavStreamWriter::create(&path, 48_000, 1).expect("create");
        let written = writer.finish().expect("finish");
        let bytes = std::fs::read(&path).expect("read");
        let _ = std::fs::remove_file(&path);
        assert_eq!(written, 0);
        assert_eq!(bytes.len(), 44, "header-only WAV is 44 bytes");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(&bytes[40..44], &0u32.to_le_bytes(), "zero data length");
    }
}

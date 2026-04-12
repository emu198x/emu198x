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
        let mut buf = vec![0; reader.output_buffer_size()];
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
}

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::AudioFrame;
#[cfg(feature = "video")]
use crate::video::{VideoInfo, VideoRecorder};

pub(crate) fn save_screenshot_argb32(
    fb: &[u32],
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|e| format!("failed to create screenshot {}: {e}", path.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|e| format!("failed to write PNG header {}: {e}", path.display()))?;

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for &pixel in fb {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(0xFF);
    }

    png_writer
        .write_image_data(&rgba)
        .map_err(|e| format!("failed to write PNG data {}: {e}", path.display()))
}

pub(crate) struct AudioCapture {
    path: PathBuf,
    writer: Option<hound::WavWriter<BufWriter<File>>>,
}

impl AudioCapture {
    pub(crate) fn start(path: PathBuf, sample_rate: u32) -> Result<Self, String> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "failed to create capture directory {}: {e}",
                    parent.display()
                )
            })?;
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = hound::WavWriter::create(&path, spec)
            .map_err(|e| format!("failed to create WAV {}: {e}", path.display()))?;

        Ok(Self {
            path,
            writer: Some(writer),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn append_frames(&mut self, frames: &[AudioFrame]) -> Result<(), String> {
        let Some(writer) = &mut self.writer else {
            return Ok(());
        };

        for &[left, right] in frames {
            let l = (left.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            let r = (right.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            writer
                .write_sample(l)
                .map_err(|e| format!("failed to write WAV sample {}: {e}", self.path.display()))?;
            writer
                .write_sample(r)
                .map_err(|e| format!("failed to write WAV sample {}: {e}", self.path.display()))?;
        }

        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Result<(), String> {
        let Some(writer) = self.writer.take() else {
            return Ok(());
        };

        writer
            .finalize()
            .map_err(|e| format!("failed to finalize WAV {}: {e}", self.path.display()))
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if let Err(e) = self.finish() {
            eprintln!("{e}");
        }
    }
}

#[cfg(feature = "video")]
pub(crate) struct VideoCapture {
    path: PathBuf,
    recorder: Option<VideoRecorder>,
    audio_buf: Vec<f32>,
}

#[cfg(feature = "video")]
impl VideoCapture {
    pub(crate) fn start(
        path: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        sample_rate: u32,
    ) -> Result<Self, String> {
        let recorder = VideoRecorder::new(width, height, fps, 2, sample_rate, &path, None)?;

        Ok(Self {
            path,
            recorder: Some(recorder),
            audio_buf: Vec::new(),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn append_frame(
        &mut self,
        pixels: &[u32],
        audio: &[AudioFrame],
    ) -> Result<(), String> {
        let Some(recorder) = &mut self.recorder else {
            return Ok(());
        };

        self.audio_buf.clear();
        self.audio_buf.reserve(audio.len().saturating_mul(2));
        for &[left, right] in audio {
            self.audio_buf.push(left);
            self.audio_buf.push(right);
        }

        recorder.add_frame(pixels, &self.audio_buf)
    }

    pub(crate) fn finish(&mut self) -> Result<VideoInfo, String> {
        let Some(recorder) = self.recorder.take() else {
            return Ok(VideoInfo { frames: 0, fps: 0 });
        };

        recorder.finish()
    }
}

#[cfg(feature = "video")]
impl Drop for VideoCapture {
    fn drop(&mut self) {
        if let Err(e) = self.finish() {
            eprintln!("{e}");
        }
    }
}

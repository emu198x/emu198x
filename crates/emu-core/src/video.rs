//! Native MP4 video recording: H.264 video + AAC audio in an MP4 container.
//!
//! Encodes emulator frames (RGBA pixels + f32 audio) into a playable MP4
//! without any external tools. Uses openh264 for video, fdk-aac for audio,
//! and muxide for the MP4 container.

#![allow(clippy::cast_possible_truncation)]

use std::io::BufWriter;
use std::path::Path;

use fdk_aac::enc::{
    AudioObjectType, BitRate as AacBitRate, ChannelMode, EncodeInfo, Encoder as AacEncoder,
    EncoderParams, Transport,
};
use muxide::api::{AacProfile, AudioCodec, MuxerBuilder, VideoCodec};
use openh264::encoder::{
    BitRate as H264BitRate, Encoder as H264Encoder, EncoderConfig, FrameRate, FrameType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::OpenH264API;

/// Information returned after recording completes.
pub struct VideoInfo {
    /// Total frames recorded.
    pub frames: u64,
    /// FPS used for recording.
    pub fps: u32,
}

/// Records emulator frames into an MP4 file.
///
/// Feed it RGBA pixel buffers and f32 audio samples each frame. When done,
/// call `finish()` to flush and finalise the MP4.
pub struct VideoRecorder {
    h264: H264Encoder,
    aac: AacEncoder,
    muxer: muxide::api::Muxer<BufWriter<std::fs::File>>,

    /// Source framebuffer width (native emulator pixels).
    src_w: u32,
    /// Source framebuffer height.
    src_h: u32,
    /// Encoded width (after optional aspect-ratio correction).
    width: u32,
    height: u32,
    fps: u32,
    audio_channels: u16,
    audio_sample_rate: u32,

    frame_count: u64,
    /// Accumulated i16 audio samples waiting to fill an AAC frame (1024 per channel).
    audio_buffer: Vec<i16>,
    /// Presentation timestamp for the next AAC frame, in seconds.
    audio_pts: f64,
    /// Reusable RGBA byte buffer to avoid per-frame allocation.
    rgba_buf: Vec<u8>,
    /// Reusable YUV buffer to avoid per-frame allocation.
    yuv_buf: YUVBuffer,
    /// Reusable AAC output buffer.
    aac_output: Vec<u8>,
    /// Reusable buffer for scaled pixels (empty when no scaling needed).
    scale_buf: Vec<u32>,
}

impl VideoRecorder {
    /// Create a recorder that writes an MP4 to `save_path`.
    ///
    /// `audio_channels` is 1 (mono) or 2 (stereo interleaved).
    /// `display_size` overrides the encoded dimensions (for aspect-ratio correction).
    pub fn new(
        width: u32,
        height: u32,
        fps: u32,
        audio_channels: u16,
        audio_sample_rate: u32,
        save_path: &Path,
        display_size: Option<(u32, u32)>,
    ) -> Result<Self, String> {
        let src_w = width;
        let src_h = height;
        let (base_w, base_h) = display_size.unwrap_or((width, height));
        // Ensure even dimensions (required by H.264 / YUV420).
        let enc_w = (base_w + 1) & !1;
        let enc_h = (base_h + 1) & !1;

        // --- H.264 encoder ---
        let api = OpenH264API::from_source();
        let config = EncoderConfig::new()
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .bitrate(H264BitRate::from_bps(2_000_000));
        let h264 = H264Encoder::with_api_config(api, config)
            .map_err(|e| format!("H.264 encoder init: {e}"))?;

        // --- AAC encoder ---
        let channel_mode = if audio_channels >= 2 {
            ChannelMode::Stereo
        } else {
            ChannelMode::Mono
        };
        let aac_bitrate = if audio_channels >= 2 {
            128_000
        } else {
            64_000
        };
        let aac_params = EncoderParams {
            bit_rate: AacBitRate::Cbr(aac_bitrate),
            sample_rate: audio_sample_rate,
            transport: Transport::Adts,
            channels: channel_mode,
            audio_object_type: AudioObjectType::Mpeg4LowComplexity,
        };
        let aac =
            AacEncoder::new(aac_params).map_err(|e| format!("AAC encoder init: {e:?}"))?;

        // --- MP4 muxer ---
        if let Some(parent) = save_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Create output directory: {e}"))?;
        }
        let file = std::fs::File::create(save_path)
            .map_err(|e| format!("Create output file: {e}"))?;
        let muxer = MuxerBuilder::new(BufWriter::new(file))
            .video(VideoCodec::H264, enc_w, enc_h, f64::from(fps))
            .audio(
                AudioCodec::Aac(AacProfile::Lc),
                audio_sample_rate,
                audio_channels,
            )
            .with_fast_start(true)
            .build()
            .map_err(|e| format!("Muxer init: {e}"))?;

        let pixel_count = (enc_w * enc_h) as usize;
        let needs_scale = src_w != enc_w || src_h != enc_h;

        Ok(Self {
            h264,
            aac,
            muxer,
            src_w,
            src_h,
            width: enc_w,
            height: enc_h,
            fps,
            audio_channels,
            audio_sample_rate,
            frame_count: 0,
            audio_buffer: Vec::new(),
            audio_pts: 0.0,
            rgba_buf: vec![0u8; pixel_count * 4],
            yuv_buf: YUVBuffer::new(enc_w as usize, enc_h as usize),
            aac_output: vec![0u8; 8192],
            scale_buf: if needs_scale {
                vec![0u32; pixel_count]
            } else {
                Vec::new()
            },
        })
    }

    /// Add one video frame (RGBA `u32` pixels, packed `0x00RRGGBB`) and its
    /// corresponding audio samples (`f32` in -1..1, mono or interleaved stereo).
    pub fn add_frame(&mut self, pixels: &[u32], audio: &[f32]) -> Result<(), String> {
        self.encode_video(pixels)?;
        self.encode_audio(audio)?;
        self.frame_count += 1;
        Ok(())
    }

    /// Flush remaining audio, finalise the MP4 container.
    pub fn finish(mut self) -> Result<VideoInfo, String> {
        self.flush_audio()?;

        self.muxer
            .finish()
            .map_err(|e| format!("Muxer finish: {e}"))?;

        Ok(VideoInfo {
            frames: self.frame_count,
            fps: self.fps,
        })
    }

    // --- internal ------------------------------------------------------------

    fn encode_video(&mut self, pixels: &[u32]) -> Result<(), String> {
        let w = self.width as usize;
        let h = self.height as usize;
        let expected = w * h;

        // Scale from source dimensions to encode dimensions if needed.
        let src_pixels = if !self.scale_buf.is_empty() {
            crate::mcp::scale_nearest_into(
                pixels,
                self.src_w,
                self.src_h,
                &mut self.scale_buf,
                self.width,
                self.height,
            );
            &self.scale_buf
        } else {
            pixels
        };

        // Convert u32 RGBA to byte RGBA. If the source has fewer pixels than
        // our (possibly padded) dimensions, fill the rest with black.
        for i in 0..expected {
            let pixel = if i < src_pixels.len() {
                src_pixels[i]
            } else {
                0
            };
            let base = i * 4;
            self.rgba_buf[base] = ((pixel >> 16) & 0xFF) as u8; // R
            self.rgba_buf[base + 1] = ((pixel >> 8) & 0xFF) as u8; // G
            self.rgba_buf[base + 2] = (pixel & 0xFF) as u8; // B
            self.rgba_buf[base + 3] = 0xFF; // A
        }

        // Convert RGBA → YUV420
        let rgba_source = RgbaSliceU8::new(&self.rgba_buf, (w, h));
        self.yuv_buf.read_rgb(rgba_source);

        // Encode to H.264
        let bitstream = self
            .h264
            .encode(&self.yuv_buf)
            .map_err(|e| format!("H.264 encode: {e}"))?;

        let h264_data = bitstream.to_vec();
        if h264_data.is_empty() {
            return Ok(());
        }

        let is_keyframe = matches!(bitstream.frame_type(), FrameType::IDR | FrameType::I);
        let pts = self.frame_count as f64 / f64::from(self.fps);

        self.muxer
            .write_video(pts, &h264_data, is_keyframe)
            .map_err(|e| format!("Mux video: {e}"))?;

        Ok(())
    }

    fn encode_audio(&mut self, audio: &[f32]) -> Result<(), String> {
        // Convert f32 → i16 and accumulate.
        for &sample in audio {
            let clamped = sample.clamp(-1.0, 1.0);
            self.audio_buffer
                .push((clamped * f32::from(i16::MAX)) as i16);
        }

        // Encode complete AAC frames (1024 samples per channel).
        let frame_size = 1024 * usize::from(self.audio_channels);
        while self.audio_buffer.len() >= frame_size {
            let chunk: Vec<i16> = self.audio_buffer.drain(..frame_size).collect();
            self.encode_aac_frame(&chunk)?;
        }

        Ok(())
    }

    fn flush_audio(&mut self) -> Result<(), String> {
        if self.audio_buffer.is_empty() {
            return Ok(());
        }

        // Pad with silence to fill the last AAC frame.
        let frame_size = 1024 * usize::from(self.audio_channels);
        self.audio_buffer.resize(frame_size, 0);
        let chunk = std::mem::take(&mut self.audio_buffer);
        self.encode_aac_frame(&chunk)
    }

    fn encode_aac_frame(&mut self, samples: &[i16]) -> Result<(), String> {
        let EncodeInfo { output_size, .. } = self
            .aac
            .encode(samples, &mut self.aac_output)
            .map_err(|e| format!("AAC encode: {e:?}"))?;

        if output_size > 0 {
            self.muxer
                .write_audio(self.audio_pts, &self.aac_output[..output_size])
                .map_err(|e| format!("Mux audio: {e}"))?;
            self.audio_pts += 1024.0 / f64::from(self.audio_sample_rate);
        }

        Ok(())
    }
}

//! Cross-system MP4 video recording driven by an ffmpeg subprocess.
//!
//! The recorder spawns ffmpeg and streams raw RGBA frames to its stdin while
//! the emulator runs. On stop, it closes stdin, waits for ffmpeg to finalise
//! the intermediate video file, and (when audio is provided) launches a second
//! ffmpeg call to mux video + audio into the final MP4.
//!
//! The shell crate stays system-agnostic: callers feed it `CapturedFrame`s
//! emitted by their runtime and a `CapturedAudio` snapshot from the existing
//! `AudioCapture` sink.

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use thiserror::Error;

use crate::capture::{CaptureError, CapturedAudio, CapturedFrame};
use crate::time::MachineTime;

/// Failure surfaced by the video recorder.
#[derive(Debug, Error)]
pub enum VideoRecordingError {
    /// `ffmpeg` was not found on `PATH`.
    #[error("ffmpeg not found on PATH; install ffmpeg to enable video capture")]
    FfmpegNotFound,

    /// Spawning the ffmpeg subprocess failed.
    #[error("failed to spawn ffmpeg: {0}")]
    FfmpegSpawn(#[source] std::io::Error),

    /// Writing a frame to ffmpeg's stdin failed mid-recording.
    #[error("failed to write frame to ffmpeg stdin: {0}")]
    FfmpegStdin(#[source] std::io::Error),

    /// ffmpeg exited with a non-zero status.
    #[error("ffmpeg exited with status {status}: {stderr}")]
    FfmpegFailed {
        /// The reported exit status as a string (`"signal: 9"` or `"code: 1"`).
        status: String,
        /// The captured stderr tail.
        stderr: String,
    },

    /// A pushed frame's geometry did not match the recorder's configuration.
    #[error(
        "frame {actual_width}x{actual_height} does not match recorder \
         {expected_width}x{expected_height}"
    )]
    DimensionMismatch {
        /// The recorder's configured width.
        expected_width: u32,
        /// The recorder's configured height.
        expected_height: u32,
        /// The pushed frame's width.
        actual_width: u32,
        /// The pushed frame's height.
        actual_height: u32,
    },

    /// Converting a captured frame to RGBA bytes failed.
    #[error(transparent)]
    Capture(#[from] CaptureError),

    /// Filesystem I/O failed (temp file write, rename, cleanup).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Summary of one completed recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoRecordingSummary {
    /// The final MP4 file's path.
    pub path: PathBuf,
    /// Total frames written to the recorder.
    pub frames: u64,
    /// Recording duration in milliseconds, computed from machine time.
    pub duration_ms: u64,
    /// Whether the final MP4 contains a muxed audio track.
    pub has_audio: bool,
}

/// One in-flight video recording session.
///
/// While alive, the recorder owns an ffmpeg subprocess writing to a temporary
/// file alongside the requested output. Call [`Self::push_frame`] for each
/// emulator frame and [`Self::finish`] to finalise the MP4.
///
/// Dropping a recorder without calling `finish` aborts the ffmpeg subprocess
/// and removes the partial intermediate file.
#[derive(Debug)]
pub struct VideoRecorder {
    output_path: PathBuf,
    intermediate_path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    started_at: MachineTime,
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    frames_written: u64,
    finished: bool,
}

impl VideoRecorder {
    /// Spawns ffmpeg and begins a new recording session.
    ///
    /// `output_path` is the final MP4 location. While the recording is in
    /// flight, ffmpeg writes to a sibling temp file; this is renamed (or
    /// muxed-and-replaced when audio is supplied) at finish time.
    ///
    /// # Errors
    ///
    /// Returns [`VideoRecordingError::FfmpegNotFound`] if `ffmpeg` is not on
    /// `PATH`, or [`VideoRecordingError::FfmpegSpawn`] if the subprocess fails
    /// to launch.
    pub fn start(
        output_path: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        started_at: MachineTime,
    ) -> Result<Self, VideoRecordingError> {
        if find_ffmpeg().is_none() {
            return Err(VideoRecordingError::FfmpegNotFound);
        }

        let intermediate_path = intermediate_video_path(&output_path);
        if let Some(parent) = intermediate_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let args = video_pass_args(width, height, fps, &intermediate_path);
        let mut command = Command::new("ffmpeg");
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut process = command.spawn().map_err(VideoRecordingError::FfmpegSpawn)?;
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| VideoRecordingError::FfmpegSpawn(stdin_unavailable()))?;

        Ok(Self {
            output_path,
            intermediate_path,
            width,
            height,
            fps,
            started_at,
            process: Some(process),
            stdin: Some(stdin),
            frames_written: 0,
            finished: false,
        })
    }

    /// Returns the configured width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the configured height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the configured frame rate.
    #[must_use]
    pub const fn fps(&self) -> u32 {
        self.fps
    }

    /// Returns the number of frames written so far.
    #[must_use]
    pub const fn frames_written(&self) -> u64 {
        self.frames_written
    }

    /// Returns the machine time the recording started at.
    #[must_use]
    pub const fn started_at(&self) -> MachineTime {
        self.started_at
    }

    /// Writes one captured frame to the ffmpeg stdin pipe.
    ///
    /// # Errors
    ///
    /// Returns [`VideoRecordingError::DimensionMismatch`] if the frame's
    /// geometry differs from the recorder configuration,
    /// [`VideoRecordingError::Capture`] if the frame cannot be converted to
    /// RGBA, or [`VideoRecordingError::FfmpegStdin`] if writing to ffmpeg
    /// fails.
    pub fn push_frame(&mut self, frame: &CapturedFrame) -> Result<(), VideoRecordingError> {
        if frame.width != self.width || frame.height != self.height {
            return Err(VideoRecordingError::DimensionMismatch {
                expected_width: self.width,
                expected_height: self.height,
                actual_width: frame.width,
                actual_height: frame.height,
            });
        }

        let rgba = frame.rgba_pixels()?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| VideoRecordingError::FfmpegStdin(stdin_unavailable()))?;
        stdin
            .write_all(&rgba)
            .map_err(VideoRecordingError::FfmpegStdin)?;
        self.frames_written += 1;
        Ok(())
    }

    /// Finalises the recording.
    ///
    /// Closes the ffmpeg stdin pipe, waits for the video pass to complete,
    /// then either renames the intermediate file to `output_path` (no audio)
    /// or runs a second ffmpeg pass to mux video + audio.
    ///
    /// # Errors
    ///
    /// Returns [`VideoRecordingError::FfmpegFailed`] if either ffmpeg pass
    /// exits non-zero, propagating its stderr tail. Filesystem failures are
    /// surfaced as [`VideoRecordingError::Io`].
    pub fn finish(
        mut self,
        audio: Option<&CapturedAudio>,
    ) -> Result<VideoRecordingSummary, VideoRecordingError> {
        self.finished = true;
        drop(self.stdin.take());

        let process = self
            .process
            .take()
            .ok_or_else(|| VideoRecordingError::FfmpegSpawn(stdin_unavailable()))?;
        let output = process.wait_with_output()?;
        if !output.status.success() {
            let _ = fs::remove_file(&self.intermediate_path);
            return Err(VideoRecordingError::FfmpegFailed {
                status: format_exit_status(&output.status),
                stderr: tail_stderr(&output.stderr),
            });
        }

        let has_audio = audio.is_some();
        if let Some(audio) = audio {
            mux_audio(&self.intermediate_path, &self.output_path, audio)?;
            let _ = fs::remove_file(&self.intermediate_path);
        } else {
            if self.output_path.exists() {
                fs::remove_file(&self.output_path)?;
            }
            fs::rename(&self.intermediate_path, &self.output_path)?;
        }

        Ok(VideoRecordingSummary {
            path: self.output_path.clone(),
            frames: self.frames_written,
            duration_ms: duration_ms(self.fps, self.frames_written),
            has_audio,
        })
    }
}

impl Drop for VideoRecorder {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        drop(self.stdin.take());
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        let _ = fs::remove_file(&self.intermediate_path);
    }
}

/// Locates the `ffmpeg` executable on `PATH`.
#[must_use]
pub fn find_ffmpeg() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for candidate in ffmpeg_candidates(&dir) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn ffmpeg_candidates(dir: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![dir.join("ffmpeg.exe"), dir.join("ffmpeg")]
    } else {
        vec![dir.join("ffmpeg")]
    }
}

fn intermediate_video_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_else(|| OsStr::new("video").to_os_string());
    name.push(".video.tmp.mp4");
    output.with_file_name(name)
}

fn intermediate_audio_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_else(|| OsStr::new("video").to_os_string());
    name.push(".audio.tmp.wav");
    output.with_file_name(name)
}

fn video_pass_args(width: u32, height: u32, fps: u32, output: &Path) -> Vec<String> {
    vec![
        "-y".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-f".to_owned(),
        "rawvideo".to_owned(),
        "-pix_fmt".to_owned(),
        "rgba".to_owned(),
        "-s".to_owned(),
        format!("{width}x{height}"),
        "-r".to_owned(),
        fps.to_string(),
        "-i".to_owned(),
        "-".to_owned(),
        "-c:v".to_owned(),
        "libx264".to_owned(),
        "-pix_fmt".to_owned(),
        "yuv420p".to_owned(),
        "-movflags".to_owned(),
        "+faststart".to_owned(),
        output.to_string_lossy().into_owned(),
    ]
}

fn mux_pass_args(video: &Path, audio: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-i".to_owned(),
        video.to_string_lossy().into_owned(),
        "-i".to_owned(),
        audio.to_string_lossy().into_owned(),
        "-c:v".to_owned(),
        "copy".to_owned(),
        "-c:a".to_owned(),
        "aac".to_owned(),
        "-b:a".to_owned(),
        "192k".to_owned(),
        "-shortest".to_owned(),
        output.to_string_lossy().into_owned(),
    ]
}

fn mux_audio(
    video_path: &Path,
    output_path: &Path,
    audio: &CapturedAudio,
) -> Result<(), VideoRecordingError> {
    let audio_path = intermediate_audio_path(output_path);
    fs::write(&audio_path, audio.wav_bytes())?;

    let result = run_mux(video_path, &audio_path, output_path);
    let _ = fs::remove_file(&audio_path);
    result
}

fn run_mux(video: &Path, audio: &Path, output: &Path) -> Result<(), VideoRecordingError> {
    let args = mux_pass_args(video, audio, output);
    let output_result = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(VideoRecordingError::FfmpegSpawn)?;

    if output_result.status.success() {
        Ok(())
    } else {
        Err(VideoRecordingError::FfmpegFailed {
            status: format_exit_status(&output_result.status),
            stderr: tail_stderr(&output_result.stderr),
        })
    }
}

fn duration_ms(fps: u32, frames: u64) -> u64 {
    if fps == 0 {
        return 0;
    }
    frames.saturating_mul(1000) / u64::from(fps)
}

fn format_exit_status(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!("exit code {code}")
    } else {
        "terminated by signal".to_owned()
    }
}

fn tail_stderr(stderr: &[u8]) -> String {
    const MAX: usize = 4096;
    let start = stderr.len().saturating_sub(MAX);
    String::from_utf8_lossy(&stderr[start..]).into_owned()
}

fn stdin_unavailable() -> std::io::Error {
    std::io::Error::other("ffmpeg stdin pipe is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::PixelFormat;

    fn solid_frame(width: u32, height: u32, rgba: [u8; 4]) -> CapturedFrame {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&rgba);
        }
        CapturedFrame {
            timestamp: MachineTime::new(0),
            format: PixelFormat::Rgba8888,
            width,
            height,
            palette: None,
            pixels,
        }
    }

    #[test]
    fn intermediate_video_path_is_sibling_of_output() {
        let path = intermediate_video_path(Path::new("/tmp/clip.mp4"));
        assert_eq!(path, PathBuf::from("/tmp/clip.mp4.video.tmp.mp4"));
    }

    #[test]
    fn intermediate_audio_path_is_sibling_of_output() {
        let path = intermediate_audio_path(Path::new("/tmp/clip.mp4"));
        assert_eq!(path, PathBuf::from("/tmp/clip.mp4.audio.tmp.wav"));
    }

    #[test]
    fn video_pass_args_declare_rawvideo_and_libx264() {
        let args = video_pass_args(320, 240, 50, Path::new("/tmp/out.mp4"));
        assert!(args.iter().any(|arg| arg == "rawvideo"));
        assert!(args.iter().any(|arg| arg == "rgba"));
        assert!(args.iter().any(|arg| arg == "libx264"));
        assert!(args.iter().any(|arg| arg == "yuv420p"));
        assert!(args.iter().any(|arg| arg == "320x240"));
        assert!(args.iter().any(|arg| arg == "50"));
        assert!(args.iter().any(|arg| arg == "/tmp/out.mp4"));
    }

    #[test]
    fn mux_pass_args_copy_video_and_encode_audio_to_aac() {
        let args = mux_pass_args(
            Path::new("/tmp/v.mp4"),
            Path::new("/tmp/a.wav"),
            Path::new("/tmp/out.mp4"),
        );
        let copy_index = args
            .iter()
            .position(|arg| arg == "-c:v")
            .expect("video codec flag");
        assert_eq!(args[copy_index + 1], "copy");
        let aac_index = args
            .iter()
            .position(|arg| arg == "-c:a")
            .expect("audio codec flag");
        assert_eq!(args[aac_index + 1], "aac");
    }

    #[test]
    fn duration_ms_uses_frames_over_fps() {
        assert_eq!(duration_ms(50, 250), 5_000);
        assert_eq!(duration_ms(0, 250), 0);
    }

    #[test]
    fn push_frame_rejects_dimension_mismatch_without_running_ffmpeg() {
        // Construct a recorder by hand to bypass the ffmpeg spawn so the
        // test runs in any environment. The lifecycle is: drop without
        // finish, which is a no-op when process and stdin are absent.
        let mut recorder = VideoRecorder {
            output_path: PathBuf::from("/tmp/unused.mp4"),
            intermediate_path: PathBuf::from("/tmp/unused.video.tmp.mp4"),
            width: 320,
            height: 240,
            fps: 50,
            started_at: MachineTime::new(0),
            process: None,
            stdin: None,
            frames_written: 0,
            finished: true,
        };
        let frame = solid_frame(64, 48, [0xFF, 0x00, 0x00, 0xFF]);
        let err = recorder.push_frame(&frame).expect_err("should reject");
        assert!(matches!(
            err,
            VideoRecordingError::DimensionMismatch {
                expected_width: 320,
                expected_height: 240,
                actual_width: 64,
                actual_height: 48,
            }
        ));
    }

    #[test]
    fn start_and_finish_round_trip_writes_mp4_when_ffmpeg_is_available() {
        let Some(_) = find_ffmpeg() else {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        };

        let temp_dir = std::env::temp_dir();
        let output = temp_dir.join(format!(
            "emu198x-shell-video-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        let _ = fs::remove_file(&output);

        let width = 32u32;
        let height = 32u32;
        let fps = 25u32;
        let frame = solid_frame(width, height, [0x10, 0x20, 0x30, 0xFF]);

        let mut recorder = VideoRecorder::start(
            output.clone(),
            width,
            height,
            fps,
            MachineTime::new(0),
        )
        .expect("start should spawn ffmpeg");

        for _ in 0..10 {
            recorder.push_frame(&frame).expect("frame should write");
        }
        let summary = recorder.finish(None).expect("finish should succeed");

        assert_eq!(summary.path, output);
        assert_eq!(summary.frames, 10);
        assert!(!summary.has_audio);
        assert!(output.is_file(), "expected mp4 at {}", output.display());

        let metadata = fs::metadata(&output).expect("metadata");
        assert!(metadata.len() > 0, "mp4 should not be empty");

        let _ = fs::remove_file(&output);
    }

    #[test]
    fn start_and_finish_with_audio_muxes_track_when_ffmpeg_is_available() {
        let Some(_) = find_ffmpeg() else {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        };

        let temp_dir = std::env::temp_dir();
        let output = temp_dir.join(format!(
            "emu198x-shell-video-audio-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        let _ = fs::remove_file(&output);

        let width = 16u32;
        let height = 16u32;
        let fps = 25u32;
        let frame = solid_frame(width, height, [0x80, 0x80, 0x80, 0xFF]);

        let mut recorder = VideoRecorder::start(
            output.clone(),
            width,
            height,
            fps,
            MachineTime::new(0),
        )
        .expect("start should spawn ffmpeg");
        for _ in 0..5 {
            recorder.push_frame(&frame).expect("frame should write");
        }

        let audio = CapturedAudio {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![0.0; 44_100 / 5],
        };
        let summary = recorder
            .finish(Some(&audio))
            .expect("finish with audio should succeed");

        assert!(summary.has_audio);
        assert!(output.is_file());
        assert!(!intermediate_video_path(&output).exists());
        assert!(!intermediate_audio_path(&output).exists());

        let _ = fs::remove_file(&output);
    }
}

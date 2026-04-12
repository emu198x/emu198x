//! Shared JSON script execution on top of one headless session.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::control::ControlCommand;
use crate::machine::MachineCore;
use crate::media::{MediaImage, MediaKind, MediaSet};
use crate::query::{QueryError, QueryPathsResult, QueryResult};
use crate::session::{HeadlessSession, SessionError};

/// One user-facing script media kind with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaKind {
    Tape,
    Disk,
    Cartridge,
    Optical,
    Snapshot,
}

impl From<ScriptMediaKind> for MediaKind {
    fn from(value: ScriptMediaKind) -> Self {
        match value {
            ScriptMediaKind::Tape => MediaKind::Tape,
            ScriptMediaKind::Disk => MediaKind::Disk,
            ScriptMediaKind::Cartridge => MediaKind::Cartridge,
            ScriptMediaKind::Optical => MediaKind::Optical,
            ScriptMediaKind::Snapshot => MediaKind::Snapshot,
        }
    }
}

/// One user-facing media transport action with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaTransportAction {
    Start,
    Stop,
}

impl From<ScriptMediaTransportAction> for crate::MediaTransportAction {
    fn from(value: ScriptMediaTransportAction) -> Self {
        match value {
            ScriptMediaTransportAction::Start => crate::MediaTransportAction::Start,
            ScriptMediaTransportAction::Stop => crate::MediaTransportAction::Stop,
        }
    }
}

/// One shared JSON script step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ScriptStep {
    /// Load one media image into a named slot.
    LoadMedia {
        /// Stable slot identifier.
        slot: String,
        /// User-facing JSON media kind.
        kind: ScriptMediaKind,
        /// Path to the media image on disk.
        path: PathBuf,
    },
    /// Start or stop media transport on a named slot.
    MediaTransport {
        /// Stable slot identifier.
        slot: String,
        /// Requested transport action.
        transport: ScriptMediaTransportAction,
    },
    /// Queue generic input events for the next run step.
    Input {
        /// The events to queue.
        events: Vec<crate::InputEvent>,
    },
    /// Run the machine for one number of native frames.
    RunFrames {
        /// Number of native video frames to execute.
        frames: u32,
    },
    /// Resolve one shared query path.
    Query {
        /// The query path to resolve.
        path: String,
    },
    /// List supported query paths, optionally filtered by prefix.
    QueryPaths {
        /// Optional prefix filter.
        prefix: Option<String>,
    },
    /// Restore one snapshot file into the live machine.
    LoadSnapshot {
        /// Path to the snapshot on disk.
        path: PathBuf,
    },
    /// Save the current machine snapshot to disk.
    SaveSnapshot {
        /// Output path for the snapshot.
        path: PathBuf,
    },
    /// Save the latest emitted frame as PNG.
    SaveScreenshot {
        /// Output path for the PNG file.
        path: PathBuf,
    },
    /// Save the captured audio stream as WAV.
    SaveAudioCapture {
        /// Output path for the WAV file.
        path: PathBuf,
        /// Whether to clear captured audio after writing the file.
        #[serde(default = "default_true")]
        reset_after: bool,
    },
}

/// One JSON script made of ordered shared steps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeadlessScript {
    /// The ordered steps to execute.
    pub steps: Vec<ScriptStep>,
}

/// One structured observation emitted by the shared script layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptObservation {
    /// Result of a frame-run step.
    RunFrames {
        /// Number of requested native frames.
        frames: u32,
        /// Machine time reached after the run.
        reached: crate::MachineTime,
        /// Why the machine stopped.
        stop_reason: crate::StopReason,
    },
    /// Result of resolving one query path.
    Query {
        /// Resolved query data.
        result: QueryResult,
    },
    /// Result of listing supported query paths.
    QueryPaths {
        /// Query-path listing response.
        result: QueryPathsResult,
    },
}

impl HeadlessScript {
    /// Parses one script from UTF-8 JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid.
    pub fn from_json_str(text: &str) -> Result<Self, ScriptError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Loads one script from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O or JSON parsing fails.
    pub fn from_path(path: &Path) -> Result<Self, ScriptError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json_str(&text)
    }

    /// Executes this script against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore>(
        &self,
        session: &mut HeadlessSession<M>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes this script and returns any structured observations it emits.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore>(
        &self,
        session: &mut HeadlessSession<M>,
    ) -> Result<Vec<ScriptObservation>, ScriptError> {
        let mut observations = Vec::new();
        for step in &self.steps {
            if let Some(observation) = step.execute_collect(session)? {
                observations.push(observation);
            }
        }

        Ok(observations)
    }
}

impl ScriptStep {
    /// Executes one script step against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore>(
        &self,
        session: &mut HeadlessSession<M>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes one script step and returns any structured observation it
    /// produces.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore>(
        &self,
        session: &mut HeadlessSession<M>,
    ) -> Result<Option<ScriptObservation>, ScriptError> {
        match self {
            Self::LoadMedia { slot, kind, path } => {
                let bytes = std::fs::read(path)?;
                let mut media = MediaSet::new();
                media.push(MediaImage::new(slot.clone(), (*kind).into(), &bytes));
                session.load_media(&media)?;
                Ok(None)
            }
            Self::MediaTransport { slot, transport } => {
                session.command(&ControlCommand::MediaTransport(
                    crate::MediaTransportCommand::new(slot.clone(), (*transport).into()),
                ))?;
                Ok(None)
            }
            Self::Input { events } => {
                session.queue_inputs(events.iter().cloned());
                Ok(None)
            }
            Self::RunFrames { frames } => {
                let result = session.run_frames(*frames)?;
                Ok(Some(ScriptObservation::RunFrames {
                    frames: *frames,
                    reached: result.reached,
                    stop_reason: result.stop_reason,
                }))
            }
            Self::Query { path } => {
                let result = session.query(path)?;
                Ok(Some(ScriptObservation::Query { result }))
            }
            Self::QueryPaths { prefix } => {
                let result = session.query_paths(prefix.as_deref());
                Ok(Some(ScriptObservation::QueryPaths { result }))
            }
            Self::LoadSnapshot { path } => {
                let bytes = std::fs::read(path)?;
                session.restore_snapshot(&bytes)?;
                Ok(None)
            }
            Self::SaveSnapshot { path } => {
                session.save_snapshot(path)?;
                Ok(None)
            }
            Self::SaveScreenshot { path } => {
                session.save_screenshot(path)?;
                Ok(None)
            }
            Self::SaveAudioCapture { path, reset_after } => {
                session.save_audio_capture(path)?;
                if *reset_after {
                    session.clear_audio_capture();
                }
                Ok(None)
            }
        }
    }
}

/// Error surfaced by the shared JSON script layer.
#[derive(Debug, Error)]
pub enum ScriptError {
    /// One filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON parsing failed.
    #[error(transparent)]
    Parse(#[from] serde_json::Error),

    /// Session execution failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Query resolution failed.
    #[error(transparent)]
    Query(#[from] QueryError),
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::error::MachineError;
    use crate::host::{AudioPacket, FramePacket, HostIo, PixelFormat};
    use crate::machine::{
        Family, MachineId, MachineProfile, ProfileId, Region, ResetKind, RunResult, StopReason,
        SupportTier,
    };
    use crate::media::{FirmwareRequirement, MediaSlot, WritebackPolicy};
    use crate::time::{ClockDesc, ClockRate, MachineTime};

    struct DummyMachine {
        profile: MachineProfile,
        time: MachineTime,
        tape_loaded: usize,
        commands: usize,
        restored: usize,
    }

    impl DummyMachine {
        fn new() -> Self {
            Self {
                profile: MachineProfile {
                    machine_id: MachineId::from("dummy-machine"),
                    profile_id: ProfileId::from("dummy-profile"),
                    display_name: "Dummy".into(),
                    family: Family::Spectrum,
                    region: Region::Pal,
                    support_tier: SupportTier::Research,
                    release_year: 1982,
                    summary: "dummy".into(),
                    clock: ClockDesc::new("master-cycle", ClockRate::from_hz(1)),
                    firmware: vec![FirmwareRequirement::new("rom-0", "ROM 0", false)],
                    media_slots: vec![MediaSlot::new(
                        "tape-1",
                        "Tape Deck",
                        MediaKind::Tape,
                        false,
                        WritebackPolicy::InMemoryOnly,
                    )],
                    capabilities: CapabilitySet::new(),
                },
                time: MachineTime::default(),
                tape_loaded: 0,
                commands: 0,
                restored: 0,
            }
        }
    }

    impl MachineCore for DummyMachine {
        fn profile(&self) -> &MachineProfile {
            &self.profile
        }

        fn time(&self) -> MachineTime {
            self.time
        }

        fn reset(&mut self, _kind: ResetKind) {}

        fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
            self.tape_loaded += media.images.len();
            Ok(())
        }

        fn run_until(
            &mut self,
            target: MachineTime,
            host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            self.time = target;
            host.frame_sink.push_frame(FramePacket {
                timestamp: target,
                format: PixelFormat::Indexed8,
                width: 1,
                height: 1,
                palette: Some(&[0x000000FF, 0xFFFFFFFF]),
                pixels: &[1],
            })?;
            host.audio_sink.push_audio(AudioPacket {
                timestamp: target,
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0, 0.25],
            })?;
            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }

        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![0x55])
        }

        fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
            self.restored += 1;
            Ok(())
        }

        fn command(&mut self, _command: &ControlCommand) -> Result<(), MachineError> {
            self.commands += 1;
            Ok(())
        }

        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::new()
        }
    }

    #[test]
    fn headless_script_parses_json_array() {
        let script = HeadlessScript::from_json_str(
            r#"
            [
              {"action":"run_frames","frames":2},
              {"action":"query","path":"session.time"},
              {"action":"save_screenshot","path":"boot.png"}
            ]
            "#,
        )
        .expect("script json should parse");

        assert_eq!(
            script.steps,
            vec![
                ScriptStep::RunFrames { frames: 2 },
                ScriptStep::Query {
                    path: "session.time".to_owned()
                },
                ScriptStep::SaveScreenshot {
                    path: PathBuf::from("boot.png")
                }
            ]
        );
    }

    #[test]
    fn headless_script_executes_media_run_capture_and_snapshot_steps() {
        let temp_dir = std::env::temp_dir();
        let media_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-demo.tap",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-shot.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-audio.wav",
            std::process::id()
        ));
        let snapshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-state.pst",
            std::process::id()
        ));
        std::fs::write(&media_path, [0x13, 0x00, 0x00]).expect("media fixture should write");

        let script = HeadlessScript {
            steps: vec![
                ScriptStep::LoadMedia {
                    slot: "tape-1".to_owned(),
                    kind: ScriptMediaKind::Tape,
                    path: media_path.clone(),
                },
                ScriptStep::MediaTransport {
                    slot: "tape-1".to_owned(),
                    transport: ScriptMediaTransportAction::Start,
                },
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::SaveScreenshot {
                    path: screenshot_path.clone(),
                },
                ScriptStep::SaveAudioCapture {
                    path: audio_path.clone(),
                    reset_after: true,
                },
                ScriptStep::SaveSnapshot {
                    path: snapshot_path.clone(),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should run to completion");

        assert_eq!(session.machine().tape_loaded, 1);
        assert_eq!(session.machine().commands, 1);
        assert_eq!(
            observations,
            vec![ScriptObservation::RunFrames {
                frames: 1,
                reached: MachineTime::new(69888),
                stop_reason: StopReason::ReachedTarget,
            }]
        );
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());
        assert!(snapshot_path.is_file());

        let _ = std::fs::remove_file(media_path);
        let _ = std::fs::remove_file(screenshot_path);
        let _ = std::fs::remove_file(audio_path);
        let _ = std::fs::remove_file(snapshot_path);
    }

    #[test]
    fn headless_script_collects_query_observations() {
        let script = HeadlessScript {
            steps: vec![
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::Query {
                    path: "session.time".to_owned(),
                },
                ScriptStep::QueryPaths {
                    prefix: Some("capture.".to_owned()),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should produce observations");

        assert_eq!(observations.len(), 3);
        assert_eq!(
            observations[1],
            ScriptObservation::Query {
                result: QueryResult {
                    path: "session.time".to_owned(),
                    value: serde_json::json!(69888),
                }
            }
        );
        assert_eq!(
            observations[2],
            ScriptObservation::QueryPaths {
                result: QueryPathsResult {
                    prefix: Some("capture.".to_owned()),
                    paths: vec![
                        "capture.has_audio".to_owned(),
                        "capture.has_frame".to_owned()
                    ],
                }
            }
        );
    }
}

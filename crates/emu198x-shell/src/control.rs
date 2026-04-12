//! Shared host-to-machine control commands beyond raw input events.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// One transport action that may be applied to a loaded media slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MediaTransportAction {
    /// Start or resume media playback/transport.
    Start,
    /// Stop media playback/transport without ejecting it.
    Stop,
}

/// One transport command for a concrete media slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTransportCommand {
    /// Stable slot identifier, for example `tape-1`.
    pub slot: Cow<'static, str>,
    /// Requested transport action.
    pub action: MediaTransportAction,
}

impl MediaTransportCommand {
    /// Creates one media-transport command.
    #[must_use]
    pub fn new(slot: impl Into<Cow<'static, str>>, action: MediaTransportAction) -> Self {
        Self {
            slot: slot.into(),
            action,
        }
    }
}

/// One host-to-machine command on the shared control surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ControlCommand {
    /// Start or stop the transport for a media slot.
    MediaTransport(MediaTransportCommand),
}

impl ControlCommand {
    /// Returns the stable operation identifier for this command.
    #[must_use]
    pub const fn operation_name(&self) -> &'static str {
        match self {
            Self::MediaTransport(_) => "media-transport",
        }
    }
}

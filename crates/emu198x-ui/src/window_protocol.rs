//! The messages between a machine binary and the GTK window process.
//!
//! On Linux the GTK window runs in its own process, `emu198x-gtk-window`, so
//! the machine binaries never link GTK and their headless, script and MCP
//! modes run where GTK is not installed. The machine binary keeps the
//! [`HostCore`](crate::host_core) and talks to the window over the child's
//! standard input and output: frames and window requests go one way, input
//! and menu commands come back. See
//! `knowledge/decisions/gtk-window-process.md`.
//!
//! Each message is a little-endian `u32` length followed by that many bytes
//! of postcard. Both ends are built from this one module, and the first
//! message carries [`PROTOCOL_VERSION`], so a window from another release is
//! refused rather than misread.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use emu198x_shell::{CapturedFrame, MediaKind};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use winit::event::MouseButton;
use winit::keyboard::{Key, KeyCode, ModifiersState};

pub use crate::menu::{AppCommand, FILTER_OPTIONS, SCALE_OPTIONS, filter_name};
use crate::{DrivePortInfo, VariantInfo, VideoFilter};

/// Changed whenever a message changes shape.
pub const PROTOCOL_VERSION: u32 = 2;

/// The largest message either end will read. A frame is a few hundred
/// kilobytes; this only stops a corrupt length from allocating gigabytes.
pub const MAX_MESSAGE_BYTES: u32 = 64 * 1024 * 1024;

/// A media slot, for the File menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotInfo {
    pub id: String,
    pub display_name: String,
    pub kind: MediaKind,
}

/// A machine variant, for the Machine menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantEntry {
    pub id: String,
    pub label: String,
    pub group: Option<String>,
}

impl From<&VariantInfo> for VariantEntry {
    fn from(variant: &VariantInfo) -> Self {
        Self {
            id: variant.id.to_string(),
            label: variant.label.to_string(),
            group: variant.group.as_ref().map(ToString::to_string),
        }
    }
}

/// One drive model a device port can hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveEntry {
    pub id: String,
    pub label: String,
    pub available: bool,
}

/// A device port and its drive models, for the Machine → Drives menu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrivePortEntry {
    pub device: u8,
    pub label: String,
    pub options: Vec<DriveEntry>,
    pub current: String,
}

impl From<&DrivePortInfo> for DrivePortEntry {
    fn from(port: &DrivePortInfo) -> Self {
        Self {
            device: port.device,
            label: port.label.to_string(),
            options: port
                .options
                .iter()
                .map(|option| DriveEntry {
                    id: option.id.to_string(),
                    label: option.label.to_string(),
                    available: option.available,
                })
                .collect(),
            current: port.current.to_string(),
        }
    }
}

/// Everything the window needs to open: its title, its menus, and the
/// picture's size at scale 1 and at the chosen scale, in logical pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowSetup {
    pub version: u32,
    pub title: String,
    pub media_slots: Vec<SlotInfo>,
    pub variants: Vec<VariantEntry>,
    pub current_variant: Option<String>,
    pub drive_ports: Vec<DrivePortEntry>,
    /// `Some(on)` when the machine offers host-layout typing.
    pub host_keyboard: Option<bool>,
    pub state_open: bool,
    pub tape_export: bool,
    pub scale: u32,
    pub filter: VideoFilter,
    pub minimum_size: (f64, f64),
    pub natural_size: (f64, f64),
}

/// How to show a frame: the presentation settings that travel with it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameSettings {
    pub filter: VideoFilter,
    pub pixel_aspect_ratio: f32,
    pub clock_hz: f64,
    pub phosphor_tau_ms: f32,
}

/// A menu check mark the core has moved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkChange {
    Scale(u32),
    Filter(VideoFilter),
    Variant(String),
    PortDrive { device: u8, kind_id: String },
    Turbo(bool),
    HostKeyboard(bool),
    Paused(bool),
}

/// How serious a message box is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// From the machine binary to the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToWindow {
    /// The first message: open the window.
    Open(Box<WindowSetup>),
    /// Show this frame.
    Frame {
        frame: Box<CapturedFrame>,
        settings: FrameSettings,
    },
    Title(String),
    /// Ask for a picture of this many logical pixels.
    PictureSize {
        width: f64,
        height: f64,
    },
    /// Forget earlier frames (phosphor persistence).
    ResetVideoHistory,
    Mark(MarkChange),
    /// Show (`true`) or hide the host's cursor over the picture.
    CursorVisible(bool),
    /// Ask for a file; the window answers with [`FromWindow::FileChosen`].
    PickFile {
        title: String,
        filter_label: String,
        extensions: Vec<String>,
        /// `Some(name)` asks where to save, suggesting `name`.
        save_as: Option<String>,
        all_files: bool,
    },
    /// Show a message box; the window answers with
    /// [`FromWindow::MessageClosed`].
    Message {
        title: String,
        level: Level,
        text: String,
    },
    /// The session is over: close the window and exit.
    Close,
}

/// From the window to the machine binary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FromWindow {
    /// The window is open and drawing.
    Ready,
    /// The window cannot speak this protocol version.
    Incompatible {
        version: u32,
    },
    Key {
        code: KeyCode,
        logical: Key,
        pressed: bool,
        repeat: bool,
    },
    Modifiers(ModifiersState),
    FocusLost,
    /// The pointer moved to `(x, y)` within the picture, whose size is `size`.
    Cursor {
        x: f64,
        y: f64,
        size: Option<(u32, u32)>,
    },
    CursorLeft,
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    /// A menu item was chosen.
    Command(AppCommand),
    FileChosen(Option<PathBuf>),
    MessageClosed,
    /// The user closed the window.
    Closed,
}

/// Write one message.
///
/// # Errors
///
/// Returns an error if the message cannot be encoded or written.
pub fn write_message<T: Serialize>(writer: &mut impl Write, message: &T) -> io::Result<()> {
    let bytes = postcard::to_allocvec(message)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let length = u32::try_from(bytes.len())
        .ok()
        .filter(|length| *length <= MAX_MESSAGE_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "message too large"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

/// Read one message. `Ok(None)` means the other end closed the pipe.
///
/// # Errors
///
/// Returns an error for a truncated, oversized or undecodable message.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let length = u32::from_le_bytes(length);
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut bytes = vec![0u8; length as usize];
    reader.read_exact(&mut bytes)?;
    postcard::from_bytes(&bytes)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{MachineTime, PixelFormat};

    #[test]
    fn messages_round_trip_through_the_framing() {
        let frame = CapturedFrame {
            signal: None,
            timestamp: MachineTime::new(7),
            format: PixelFormat::Rgba8888,
            width: 1,
            height: 1,
            palette: None,
            pixels: vec![1, 2, 3, 4],
        };
        let sent = vec![
            ToWindow::Frame {
                frame: Box::new(frame),
                settings: FrameSettings {
                    filter: VideoFilter::Crt,
                    pixel_aspect_ratio: 1.055,
                    clock_hz: 3_500_000.0,
                    phosphor_tau_ms: 2.0,
                },
            },
            ToWindow::Mark(MarkChange::Variant("zx-128k".into())),
            ToWindow::Close,
        ];
        let mut pipe = Vec::new();
        for message in &sent {
            write_message(&mut pipe, message).expect("write");
        }
        let mut reader = pipe.as_slice();
        for message in &sent {
            let got: ToWindow = read_message(&mut reader).expect("read").expect("a message");
            assert_eq!(&got, message);
        }
        assert_eq!(read_message::<ToWindow>(&mut reader).expect("read"), None);
    }

    #[test]
    fn keys_and_commands_survive_the_trip_back() {
        let sent = [
            FromWindow::Key {
                code: KeyCode::KeyQ,
                logical: Key::Character("q".into()),
                pressed: true,
                repeat: false,
            },
            FromWindow::Command(AppCommand::SetFilter(VideoFilter::Crt)),
            FromWindow::FileChosen(Some(PathBuf::from("/tmp/dizzy.tzx"))),
        ];
        let mut pipe = Vec::new();
        for message in &sent {
            write_message(&mut pipe, message).expect("write");
        }
        let mut reader = pipe.as_slice();
        for message in &sent {
            let got: FromWindow = read_message(&mut reader).expect("read").expect("a message");
            assert_eq!(&got, message);
        }
    }

    #[test]
    fn an_oversized_length_is_refused_before_allocating() {
        let mut pipe = (MAX_MESSAGE_BYTES + 1).to_le_bytes().to_vec();
        pipe.extend([0; 8]);
        assert!(read_message::<FromWindow>(&mut pipe.as_slice()).is_err());
    }
}

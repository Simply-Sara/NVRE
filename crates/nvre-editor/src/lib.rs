//! The editor's session and viewport logic, as a library.
//!
//! The binary is the window around this; keeping the logic here means the
//! session can be driven from a test or an example without a display.

use bevy::prelude::*;

pub mod gizmo;
pub mod launcher;
pub mod object_editor;
pub mod panels;
pub mod session;
pub mod viewport;
pub mod windows;

/// The game's Data folder, as a resource.
///
/// The launcher's file dialog starts here, so picking a plugin does not mean
/// navigating to the Steam folder every time.
#[derive(Resource)]
pub struct DataDir(pub std::path::PathBuf);

/// Raised by the launcher's Open button: open the chosen file.
///
/// A message rather than a call, because the launcher draws the UI and opening a
/// plugin reads hundreds of megabytes — the work belongs in a system, not in the
/// middle of a frame.
#[derive(Message)]
pub struct OpenRequested;

/// Raised by the references panel on a double-click: move the camera to that
/// reference.
///
/// A message rather than a call, for the same reason as [`OpenRequested`]: the
/// panel draws in its own window's schedule, and the camera lives in the
/// viewport's `Update` systems. The message is what carries the intent between
/// them without either knowing about the other.
#[derive(Message)]
pub struct FrameRequested {
    /// The reference to look at.
    pub form_id: u32,
}

/// Why the last attempt to open a plugin failed, for the launcher to show.
#[derive(Resource)]
pub struct OpenFailed(pub String);

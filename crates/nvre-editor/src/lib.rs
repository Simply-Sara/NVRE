//! The editor's session and viewport logic, as a library.
//!
//! The binary is the window around this; keeping the logic here means the
//! session can be driven from a test or an example without a display.

use bevy::prelude::*;

pub mod file_details;
pub mod file_details_window;
pub mod game;
pub mod gizmo;
pub mod launcher;
pub mod object_editor;
pub mod panels;
pub mod plugin_order;
pub mod session;
pub mod strings;
pub mod viewport;
pub mod windows;

/// Which game is open, as a resource.
///
/// A profile is `&'static` because the tables are constants, so this is a `Copy`
/// handle rather than owned data: every panel, window and system can read it
/// without a clone or a lock.
#[derive(Resource, Clone, Copy)]
pub struct Game(pub &'static crate::game::GameProfile);

/// The game's Data folder, as a resource.
///
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

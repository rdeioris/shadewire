//! `--screenshot`: once the scene is ready, waits a few frames, saves the window and exits.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

/// Frames rendered after the scene is ready before the capture.
const FRAMES: u32 = 10;

#[derive(Resource)]
pub struct ShotPath(pub PathBuf);

/// Set when the materials are in place (at once for the fallback scene, on `WorldInstanceReady` for glTF).
#[derive(Resource, Default)]
pub struct Ready(pub bool);

pub fn take(mut commands: Commands, ready: Res<Ready>, path: Option<Res<ShotPath>>, mut frames: Local<u32>) {
    let Some(path) = path else { return };
    if !ready.0 {
        return;
    }
    *frames += 1;
    if *frames != FRAMES {
        return;
    }
    let path = path.0.clone();
    let mut save = save_to_disk(path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save(captured);
            if path.is_file() {
                println!("saved {}", path.display());
                exit.write(AppExit::Success);
            } else {
                eprintln!("error: cannot save the screenshot to {}", path.display());
                exit.write(AppExit::error());
            }
        },
    );
}

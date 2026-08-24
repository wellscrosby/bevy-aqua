//! Self-contained screenshot support for the showcase example.
//!
//! Headless runs render the marked camera into an offscreen image. Both that
//! image and the normal primary window are captured through Bevy's built-in
//! screenshot API, so the public example has no external harness dependency.

use std::path::PathBuf;

use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};

/// Marks the camera used by headless showcase captures.
#[derive(Component, Debug, Default)]
pub struct CaptureCamera;

/// What the showcase saves after its warmup period.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CaptureMode {
    /// Render without saving an image.
    #[default]
    None,
    /// Save one PNG and exit.
    Single { path: PathBuf },
    /// Save numbered PNGs into a directory and exit.
    Sequence { directory: PathBuf, count: u32 },
}

/// Screenshot settings used by [`CapturePlugin`].
#[derive(Resource, Clone, Debug)]
pub struct CaptureConfig {
    pub warmup_frames: u32,
    pub size: UVec2,
    pub mode: CaptureMode,
    pub stride: u32,
}

/// Capture counts available to scene animation systems.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct CaptureProgress {
    pub requested: u32,
    pub completed: u32,
}

/// Orders scene animation before screenshot requests.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureSystems;

/// Adds built-in Bevy screenshot capture to the showcase.
#[derive(Debug)]
pub struct CapturePlugin {
    config: CaptureConfig,
    headless: bool,
}

impl CapturePlugin {
    pub fn headless(config: CaptureConfig) -> Self {
        Self {
            config,
            headless: true,
        }
    }

    pub fn windowed(config: CaptureConfig) -> Self {
        Self {
            config,
            headless: false,
        }
    }
}

#[derive(Resource, Debug, Default)]
struct CaptureState {
    target: Option<Handle<Image>>,
    needs_target: bool,
    frames: u32,
    requested: u32,
    completed: u32,
    since_request: u32,
}

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .insert_resource(CaptureState {
                needs_target: self.headless,
                ..default()
            })
            .init_resource::<CaptureProgress>();
        if self.headless {
            app.add_systems(Update, attach_capture_target.in_set(CaptureSystems));
        }
        app.add_systems(Update, advance_capture.in_set(CaptureSystems));
    }
}

fn attach_capture_target(
    mut commands: Commands,
    config: Res<CaptureConfig>,
    mut state: ResMut<CaptureState>,
    cameras: Query<Entity, With<CaptureCamera>>,
    mut images: ResMut<Assets<Image>>,
) {
    if state.target.is_some() {
        return;
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    let target = images.add(Image::new_target_texture(
        config.size.x,
        config.size.y,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands
        .entity(camera)
        .insert(RenderTarget::Image(target.clone().into()));
    state.target = Some(target);
}

fn advance_capture(
    mut commands: Commands,
    config: Res<CaptureConfig>,
    mut state: ResMut<CaptureState>,
    mut progress: ResMut<CaptureProgress>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.needs_target && state.target.is_none() {
        return;
    }
    let target = state
        .target
        .clone()
        .map_or_else(Screenshot::primary_window, Screenshot::image);
    if state.frames < config.warmup_frames {
        state.frames += 1;
        return;
    }
    let path = match &config.mode {
        CaptureMode::None => return,
        CaptureMode::Single { path } => {
            if state.requested > 0 {
                return;
            }
            path.clone()
        }
        CaptureMode::Sequence { directory, count } => {
            if state.requested >= *count || state.completed < state.requested {
                return;
            }
            if state.since_request + 1 < config.stride.max(1) {
                state.since_request += 1;
                return;
            }
            state.since_request = 0;
            directory.join(format!("frame_{:04}.png", state.requested))
        }
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        error!(%error, path = %path.display(), "failed to create capture directory");
        exit.write(AppExit::Error(
            std::num::NonZero::new(1).expect("one is nonzero"),
        ));
        return;
    }
    state.requested += 1;
    progress.requested = state.requested;
    let output = path.clone();
    commands.spawn(target).observe(save_to_disk(path)).observe(
        move |_: On<ScreenshotCaptured>,
              config: Res<CaptureConfig>,
              mut state: ResMut<CaptureState>,
              mut progress: ResMut<CaptureProgress>,
              mut exit: MessageWriter<AppExit>| {
            info!("captured {}", output.display());
            state.completed += 1;
            progress.completed = state.completed;
            let finished = match config.mode {
                CaptureMode::None => false,
                CaptureMode::Single { .. } => true,
                CaptureMode::Sequence { count, .. } => state.completed >= count,
            };
            if finished {
                exit.write(AppExit::Success);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_stride_is_never_zero() {
        let config = CaptureConfig {
            warmup_frames: 75,
            size: UVec2::new(1280, 720),
            mode: CaptureMode::None,
            stride: 0,
        };
        assert_eq!(config.stride.max(1), 1);
    }
}

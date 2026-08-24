//! One water showcase: island, lake, ponds, ponds-many, river, and the
//! open-ocean `anim-waves` scene behind a single `--scene` flag, with the
//! self-contained profiling and capture CLI.

mod common;
mod config;
#[path = "showcase/scene.rs"]
mod scene;

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ShadowLodOrigin, visibility::NoFrustumCulling},
    core_pipeline::{prepass::DepthPrepass, tonemapping::Tonemapping},
    diagnostic::{DiagnosticPath, DiagnosticsStore},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, FogVolume, VolumetricFog, VolumetricLight,
        atmosphere::ScatteringMedium, light_consts::lux,
    },
    mesh::{Indices, PrimitiveTopology},
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    render::{
        diagnostic::RenderDiagnosticsPlugin,
        render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    },
    shader::ShaderRef,
    winit::WinitPlugin,
};
#[cfg(feature = "reflect")]
use bevy_aqua::ReflectedInWater;
use bevy_aqua::{
    AquaDebug, AquaPlugin, AquaSettings, BedHeightMap, Ocean, OceanWaves, ReflectionMode,
    RiverPath, RiverPoint, SeaState, WaterBody, WaterOptics, WaterShape, WaveModel,
};
#[cfg(feature = "spray")]
use bevy_aqua::{SprayQuality, SpraySettings};
use clap::{Parser, ValueEnum};
use common::capture::{
    CaptureCamera, CaptureConfig, CaptureMode, CapturePlugin, CaptureProgress, CaptureSystems,
};
use config::{ShowcaseArgs, ShowcaseConfig};

const WINDOW_SIZE: UVec2 = UVec2::new(1280, 720);

const SEA_FLOOR_CASCADE_COUNT: usize = 5;
const TERRAIN_RESOLUTION: u32 = 513;
const TERRAIN_SIZE: f32 = 600.0;
const TERRAIN_STEP: f32 = TERRAIN_SIZE / (TERRAIN_RESOLUTION - 1) as f32;
const CAPTURE_FRAME: u32 = 75;
/// Frames captured by `--capture-sequence` and the spacing between them.
const FLOW_SEQUENCE_FRAMES: u32 = 60;
const FLOW_SEQUENCE_STRIDE: u32 = 10;
// The ordered dolly reuses one warmed process; frame 30 is the first verified
// frame with sky, terrain, and both ocean backends fully pipeline-ready.
const FAR_DOLLY_CAPTURE_FRAME: u32 = 30;
const CAPTURE_TIME: f32 = 12.0;

// Frames captured by the open-ocean flight and probe framings.

const FAR_PLANE: f32 = 10_000.0;

const FLIGHT_SPEED: f32 = 8.0;
const FLIGHT_SWAY_SPEED: f32 = 0.25;
const FLIGHT_SWAY_DISTANCE: f32 = 24.0;
const BOUNDARY_HEIGHT: f32 = 18.0;
const BOUNDARY_OFFSET: Vec3 = Vec3::new(0.0, BOUNDARY_HEIGHT, 60.0);
const BOUNDARY_TARGET: Vec3 = Vec3::new(48.0, 0.0, 0.0);
const UI_MARGIN: f32 = 16.0;
const UI_FONT_SIZE: f32 = 20.0;
const BUOY_CAMERA_POSITION: Vec3 = Vec3::new(3.5, 2.35, 6.0);
const BUOY_CAMERA_TARGET: Vec3 = Vec3::new(0.0, 0.55, 0.0);
const BUOY_LAMP_HEIGHT: f32 = 1.88;
const BUOY_BEACON_LUMENS: f32 = 5_000.0;
const BUOY_BEACON_RANGE: f32 = 48.0;
const BUOY_BEACON_PERIOD_SECONDS: f32 = 8.0;
const BUOY_BEACON_DOWNWARD_SLOPE: f32 = 0.28;
const BUOY_UNDERWATER_LIGHT_DEPTH: f32 = 1.2;
const BUOY_UNDERWATER_LIGHT_LUMENS: f32 = 1_200.0;
// Keeps the full buoy and its reflected silhouette inside the low-angle view.
const REFLECTION_LAKE_CAMERA: Vec3 = Vec3::new(11.0, 4.5, 18.0);
const REFLECTION_LAKE_TARGET: Vec3 = Vec3::new(0.0, 2.0, 0.0);
const REFLECTION_LAKE_BUOY_SCALE: f32 = 4.0;
const REFLECTION_LAKE_WIND_DEGREES: f32 = 15.0;
const REFLECTION_LAKE_WIND_SPEED: f32 = 4.0;
const REFLECTION_LAKE_DETAIL_STRENGTH: f32 = 0.02;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Lighting {
    #[default]
    Day,
    Sunset,
    Night,
}

impl Lighting {
    fn label(self) -> &'static str {
        match self {
            Self::Day => "DAY",
            Self::Sunset => "SUNSET",
            Self::Night => "NIGHT",
        }
    }

    fn settings(self) -> LightingSettings {
        // Source: https://docs.rs/bevy_light/0.19.1/bevy_light/light_consts/lux/
        // Bevy's SI lux table: direct sun 100 klx, clear sunrise/sunset
        // 400 lx, and clear full moon 0.05 lx (`bevy::light::lux`).
        match self {
            Self::Day => LightingSettings {
                elevation_degrees: 45.0,
                azimuth_degrees: -35.0,
                color: Color::srgb(1.0, 0.96, 0.9),
                illuminance: lux::DIRECT_SUNLIGHT,
                environment_intensity: 1.0,
                exposure: 13.0,
            },
            Self::Sunset => LightingSettings {
                elevation_degrees: 0.5,
                azimuth_degrees: -85.0,
                color: Color::WHITE,
                illuminance: lux::RAW_SUNLIGHT,
                environment_intensity: 1.0,
                exposure: 13.0,
            },
            Self::Night => LightingSettings {
                elevation_degrees: 35.0,
                azimuth_degrees: 120.0,
                color: Color::srgb(0.3, 0.42, 1.0),
                illuminance: lux::FULL_MOON_NIGHT,
                environment_intensity: 0.08,
                // Full-moon adaptation: water stays near-black without emitters.
                exposure: 0.0,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct LightingSettings {
    elevation_degrees: f32,
    azimuth_degrees: f32,
    color: Color,
    illuminance: f32,
    environment_intensity: f32,
    exposure: f32,
}

/// Terrain and water presets per showcased scenario. Island keeps every
/// accepted default; Lake swaps the heightfield for a raised-rim basin and
/// applies calm, gently flowing water unless overridden explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum Scene {
    #[default]
    Island,
    Lake,
    /// Calm water and an enlarged buoy for planar-reflection inspection.
    ReflectionLake,
    Ponds,
    /// Ten small bodies for per-body GPU cost measurement.
    PondsMany,
    /// Winding river carved through terrain, feeding the lake basin.
    River,
    /// Open-ocean flight: waves, sky, buoy, and reflection probes.
    AnimWaves,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProfilePose {
    #[value(name = "open-2m")]
    OpenOcean2m,
    #[value(name = "open-50m")]
    OpenOcean50m,
    #[value(name = "open-500m")]
    OpenOcean500m,
    #[value(name = "island")]
    IslandOverview,
    BuoyNight,
    LakeShore,
    #[value(name = "ponds")]
    PondsOverview,
    #[value(name = "ponds-many")]
    PondsManyOverview,
    #[value(name = "river")]
    RiverOverview,
    RiverChase,
}

impl ProfilePose {
    const ALL: [Self; 5] = [
        Self::OpenOcean2m,
        Self::OpenOcean50m,
        Self::OpenOcean500m,
        Self::IslandOverview,
        Self::BuoyNight,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::OpenOcean2m => "open-2m",
            Self::OpenOcean50m => "open-50m",
            Self::OpenOcean500m => "open-500m",
            Self::IslandOverview => "island",
            Self::BuoyNight => "buoy-night",
            Self::LakeShore => "lake-shore",
            Self::PondsOverview => "ponds",
            Self::PondsManyOverview => "ponds-many",
            Self::RiverOverview => "river",
            Self::RiverChase => "river-chase",
        }
    }

    const fn scene(self) -> Scene {
        match self {
            Self::OpenOcean2m | Self::OpenOcean50m | Self::OpenOcean500m | Self::BuoyNight => {
                Scene::AnimWaves
            }
            Self::IslandOverview => Scene::Island,
            Self::LakeShore => Scene::Lake,
            Self::PondsOverview => Scene::Ponds,
            Self::PondsManyOverview => Scene::PondsMany,
            Self::RiverOverview | Self::RiverChase => Scene::River,
        }
    }
}

/// Open-ocean (`--scene anim-waves`) presentation options.
#[derive(Debug, Clone, Copy, PartialEq)]
struct OpenOcean {
    height: f32,
    boundary: bool,
    detail_close: bool,
    buoy: bool,
    buoy_spot: bool,
    buoy_lamp: bool,
    buoy_underwater_light: bool,
    cubemap_probe: Option<ProbeFraming>,
    sky_only: Option<ProbeFraming>,
    camera_offset: Vec2,
    light_scale: f32,
    exposure_offset: f32,
}

impl Default for OpenOcean {
    fn default() -> Self {
        Self {
            height: 2.0,
            boundary: false,
            detail_close: false,
            buoy: false,
            buoy_spot: true,
            buoy_lamp: true,
            buoy_underwater_light: false,
            cubemap_probe: None,
            sky_only: None,
            camera_offset: Vec2::ZERO,
            light_scale: 1.0,
            exposure_offset: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone)]
struct Demo {
    scene: Scene,
    near_shore: bool,
    close_up: bool,
    checker: bool,
    gpu_profile: bool,
    active_profile: bool,
    profile_pose: Option<ProfilePose>,
    far_dolly_step: Option<u32>,
    far_dolly_directory: Option<PathBuf>,
    flow_sequence_directory: Option<PathBuf>,
    profile_resolution: Option<UVec2>,
    water_enabled: bool,
    body_optics: Option<WaterOptics>,
    capture_time: f32,
    /// Freeze the sim clock during startup (screenshots, `--time`).
    fixed_time: bool,
    lighting: Lighting,
    ui: bool,
    bloom: bool,
    /// A capture destination freezes the open-ocean flight camera.
    frozen_camera: bool,
    open: OpenOcean,
    #[cfg(feature = "spray")]
    spray: SprayQuality,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            scene: Scene::Island,
            near_shore: false,
            close_up: false,
            checker: false,
            gpu_profile: false,
            active_profile: false,
            profile_pose: None,
            far_dolly_step: None,
            far_dolly_directory: None,
            flow_sequence_directory: None,
            profile_resolution: None,
            water_enabled: true,
            body_optics: None,
            capture_time: CAPTURE_TIME,
            fixed_time: false,
            lighting: Lighting::default(),
            ui: true,
            bloom: true,
            frozen_camera: false,
            open: OpenOcean::default(),
            #[cfg(feature = "spray")]
            spray: SprayQuality::Off,
        }
    }
}

#[derive(Component)]
struct WaterOpticsLabel(String);

#[derive(Resource, Default)]
struct GpuProfile {
    warmup_frames: u32,
    last_measurement: Option<std::time::Instant>,
    samples: Vec<[f64; 12]>,
}

fn main() -> anyhow::Result<()> {
    let arguments = ShowcaseArgs::parse();
    if arguments.profile_matrix() {
        run_profile_matrix(arguments.profile_matrix_resolution());
        return Ok(());
    }
    let ShowcaseConfig {
        debug,
        settings,
        waves,
        demo,
        screenshot,
        headless,
    } = arguments.into_config()?;
    let has_capture_destination = screenshot.is_some()
        || demo.gpu_profile
        || demo.flow_sequence_directory.is_some()
        || demo.far_dolly_directory.is_some();
    anyhow::ensure!(
        !headless || has_capture_destination,
        "--headless requires --screenshot, a capture sequence, or GPU profiling"
    );
    let render_size = demo.profile_resolution.unwrap_or(if demo.gpu_profile {
        UVec2::new(1920, 1080)
    } else {
        WINDOW_SIZE
    });
    let present_mode = if demo.gpu_profile {
        bevy::window::PresentMode::Immediate
    } else {
        bevy::window::PresentMode::AutoVsync
    };
    // Capture schedule: single screenshot at the capture frame; sequences
    // stride frames apart; the far dolly walks 176 steps of two frames.
    let (mode, warmup_frames, stride) = if let Some(path) = screenshot {
        (
            Some(CaptureMode::Single { path }),
            demo.far_dolly_step
                .map_or(CAPTURE_FRAME, |_| FAR_DOLLY_CAPTURE_FRAME),
            1,
        )
    } else if let Some(directory) = demo.far_dolly_directory.clone() {
        (
            Some(CaptureMode::Sequence {
                directory,
                count: 176 + 1,
            }),
            FAR_DOLLY_CAPTURE_FRAME,
            2,
        )
    } else if let Some(directory) = demo.flow_sequence_directory.clone() {
        (
            Some(CaptureMode::Sequence {
                directory,
                count: FLOW_SEQUENCE_FRAMES,
            }),
            CAPTURE_FRAME,
            FLOW_SEQUENCE_STRIDE,
        )
    } else {
        (None, 0, 1)
    };

    let mut app = App::new();
    app.insert_resource(debug)
        .insert_resource(settings)
        .insert_resource(waves)
        .insert_resource(demo.clone())
        .insert_resource(ClearColor(match demo.scene {
            Scene::AnimWaves => Color::srgb(0.002, 0.004, 0.009),
            _ => Color::srgb(0.02, 0.03, 0.05),
        }));
    #[cfg(feature = "spray")]
    app.insert_resource(SpraySettings {
        quality: demo.spray,
        ..default()
    });
    let window_plugin = common::window_plugin(
        headless,
        "bevy-aqua — water showcase",
        render_size,
        present_mode,
    );
    let capture_config = CaptureConfig {
        warmup_frames,
        size: render_size,
        mode: mode.clone().unwrap_or_default(),
        stride,
    };
    if headless {
        let frame_rate = if demo.active_profile { 30.0 } else { 60.0 };
        app.add_plugins(DefaultPlugins.set(window_plugin).disable::<WinitPlugin>())
            .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / frame_rate,
            )))
            .add_plugins(CapturePlugin::headless(capture_config));
    } else {
        app.add_plugins(DefaultPlugins.set(window_plugin));
        if mode.is_some() {
            app.add_plugins(CapturePlugin::windowed(capture_config));
        }
    }
    if demo.water_enabled {
        app.add_plugins(AquaPlugin);
    }
    if demo.scene == Scene::AnimWaves {
        app.add_plugins(MaterialPlugin::<CubemapProbeMaterial>::default());
    }
    app.add_systems(Startup, (scene::setup, set_capture_time))
        .add_systems(
            Update,
            (
                toggle_wave_model,
                cycle_water_optics,
                sync_water_optics_label,
                fly_camera,
                animate_test_buoy,
                rotate_buoy_beacon,
                tune_imported_buoy_material,
                dolly_camera_for_sequence,
                collect_gpu_profile,
            )
                .chain()
                .before(CaptureSystems),
        );
    if demo.gpu_profile {
        app.add_plugins(RenderDiagnosticsPlugin)
            .init_resource::<GpuProfile>();
    }
    app.run();
    Ok(())
}

fn run_profile_matrix(resolution: Option<UVec2>) {
    let executable = std::env::current_exe().expect("resolve the profile harness executable");
    let label = resolution.map_or_else(
        || "1080p".to_string(),
        |resolution| format!("{}x{}", resolution.x, resolution.y),
    );
    println!("# bevy-aqua ocean GPU pose-matrix baseline\n");
    println!(
        "{label} headless; 300 warmup frames + 300 measured frames per row. Times are median milliseconds. `whole` is the sum of all GPU spans reported by Bevy for the frame; `delta` subtracts the paired run with `AquaPlugin` and `Ocean` disabled.\n"
    );
    println!(
        "| Pose | Backend | Ocean | Cascade | Captures | Foam | FFT evolve | FFT H | FFT V | FFT resolve | FFT surface | Combine | Bloom | Custom total | Whole | No-water whole | Delta |"
    );
    println!(
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    );
    for pose in ProfilePose::ALL {
        for backend in ["gerstner", "fft"] {
            let water = run_profile_row(&executable, pose, backend, false, &resolution);
            let no_water = run_profile_row(&executable, pose, backend, true, &resolution);
            let custom_total = water[0] + water[1] + water[2] + water[3] + water[10];
            println!(
                "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} |",
                pose.label(),
                backend,
                water[0],
                water[1],
                water[2],
                water[3],
                water[4],
                water[5],
                water[6],
                water[7],
                water[8],
                water[9],
                water[10],
                custom_total,
                water[11],
                no_water[11],
                water[11] - no_water[11],
            );
        }
    }
}

fn run_profile_row(
    executable: &std::path::Path,
    pose: ProfilePose,
    backend: &str,
    no_water: bool,
    resolution: &Option<UVec2>,
) -> [f64; 12] {
    let mut command = Command::new(executable);
    command.args([
        "--headless",
        "--gpu-profile",
        "--debug",
        "composite",
        "--profile-pose",
        pose.label(),
        "--wave-backend",
        backend,
    ]);
    if let Some(resolution) = resolution {
        command.args([
            "--resolution",
            &format!("{}x{}", resolution.x, resolution.y),
        ]);
    }
    if no_water {
        command.arg("--no-water");
    }
    let output = command.output().expect("run a profile-matrix child");
    assert!(
        output.status.success(),
        "profile row {} {backend} no_water={no_water} failed:\n{}",
        pose.label(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8(output.stdout).expect("profile child output must be UTF-8");
    let row = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PROFILE_MATRIX_ROW|"))
        .unwrap_or_else(|| panic!("profile child emitted no row:\n{stdout}"));
    let values = row
        .split('|')
        .map(|value| {
            value
                .parse::<f64>()
                .expect("profile row values must be numbers")
        })
        .collect::<Vec<_>>();
    values.try_into().unwrap_or_else(|values: Vec<f64>| {
        panic!("profile row has {} values, expected 12", values.len())
    })
}

fn optics_name(optics: &bevy_aqua::WaterOptics) -> &'static str {
    WaterOptics::PRESETS
        .iter()
        .find(|(_, preset)| preset == optics)
        .map_or("custom", |(name, _)| name)
}

fn look_label(base: &str, optics: &bevy_aqua::WaterOptics) -> String {
    format!("{base}  |  optics {}", optics_name(optics))
}

fn sync_water_optics_label(
    settings: Res<AquaSettings>,
    mut labels: Query<(&WaterOpticsLabel, &mut Text)>,
) {
    if !settings.is_changed() {
        return;
    }
    for (label, mut text) in &mut labels {
        text.0 = look_label(&label.0, &settings.water_optics);
    }
}

fn cycle_water_optics(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<AquaSettings>) {
    if keys.just_pressed(KeyCode::KeyL) {
        let current = settings.water_optics;
        let index = WaterOptics::PRESETS
            .iter()
            .position(|(_, preset)| *preset == current)
            .map_or(0, |index| index + 1);
        let (name, next) = WaterOptics::PRESETS[index % WaterOptics::PRESETS.len()];
        settings.water_optics = next;
        info!("Water optics: {name}");
    }
}

fn profile_camera(pose: ProfilePose) -> Transform {
    let open_ocean_x = 220.0;
    match pose {
        ProfilePose::OpenOcean2m => Transform::from_xyz(open_ocean_x, 2.0, 8.0)
            .looking_at(Vec3::new(open_ocean_x, 0.0, 0.0), Vec3::Y),
        ProfilePose::OpenOcean50m => Transform::from_xyz(open_ocean_x, 50.0, 90.0)
            .looking_at(Vec3::new(open_ocean_x, 0.0, 0.0), Vec3::Y),
        ProfilePose::OpenOcean500m => Transform::from_xyz(open_ocean_x, 500.0, 700.0)
            .looking_at(Vec3::new(open_ocean_x, 0.0, 0.0), Vec3::Y),
        ProfilePose::IslandOverview => {
            Transform::from_xyz(68.0, 34.0, 82.0).looking_at(Vec3::ZERO, Vec3::Y)
        }
        ProfilePose::BuoyNight => Transform::from_translation(BUOY_CAMERA_POSITION)
            .looking_at(BUOY_CAMERA_TARGET, Vec3::Y),
        // Standing on the rim looking across the basin.
        ProfilePose::LakeShore => {
            Transform::from_xyz(78.0, 7.0, 30.0).looking_at(Vec3::new(-10.0, -4.0, -8.0), Vec3::Y)
        }
        // High overview taking in both ponds at once.
        // Overview taking in both ponds; the thin band at the top is the
        // world ocean beyond the plateau, which is the honest open-world
        // context for localized bodies.
        ProfilePose::PondsOverview => {
            Transform::from_xyz(-5.0, 48.0, 100.0).looking_at(Vec3::new(5.0, 0.0, 0.0), Vec3::Y)
        }
        // Wide view of the ten-pond cost-measurement field.
        ProfilePose::PondsManyOverview => Transform::from_xyz(-55.0, 135.0, 172.0)
            .looking_at(Vec3::new(25.0, -4.0, -12.0), Vec3::Y),
        // South-west bank looking north-east along the river to the lake.
        ProfilePose::RiverOverview => Transform::from_xyz(-40.0, 26.0, -115.0)
            .looking_at(Vec3::new(140.0, -3.0, 8.0), Vec3::Y),
        // Low chase camera on the upper-reach bank: meander ahead, narrows
        // and chute in view, flow running left-to-right downstream.
        ProfilePose::RiverChase => {
            Transform::from_xyz(-92.0, 10.5, -4.0).looking_at(Vec3::new(-10.0, 2.0, -42.0), Vec3::Y)
        }
    }
}

fn far_dolly_camera(step: u32) -> Transform {
    let open_ocean_x = 220.0;
    Transform::from_xyz(open_ocean_x, 20.0, 250.0 + 2.0 * step as f32)
        .looking_at(Vec3::new(open_ocean_x, 0.0, 0.0), Vec3::Y)
}

fn set_capture_time(demo: Res<Demo>, mut time: ResMut<Time<Virtual>>) {
    if !demo.fixed_time {
        return;
    }
    time.advance_by(Duration::from_secs_f32(demo.capture_time));
    // Frame sequences review MOTION: start at the capture clock and let it run.
    if !demo.active_profile && demo.flow_sequence_directory.is_none() {
        time.pause();
    }
}

/// Stages the far-dolly camera from capture progress so every sequence step
/// renders its own dolly position.
fn dolly_camera_for_sequence(
    demo: Res<Demo>,
    progress: Option<Res<CaptureProgress>>,
    mut cameras: Query<(&RenderTarget, &mut Transform), With<Camera3d>>,
) {
    let Some(progress) = progress else {
        return;
    };
    if demo.far_dolly_directory.is_none() {
        return;
    }
    let (_, mut transform) = cameras
        .iter_mut()
        .find(|(target, _)| !matches!(target, RenderTarget::None { .. }))
        .expect("the far dolly requires the ocean camera");
    *transform = far_dolly_camera(progress.completed.min(176));
}

/// Terrain heightfields, water bodies, sky, lights, camera, and UI for all
/// scenes. Per-scene behaviour follows the examples this showcase replaces.
#[allow(clippy::too_many_lines)]
fn sand_texture() -> Image {
    const SIZE: u32 = 128;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let ripple = ((x + 3 * ((y / 8) % 2)) / 8) % 2;
            let grain = ((13 * x + 7 * y) % 17) as u8;
            let base: u16 = if ripple == 0 { 150 } else { 92 };
            let grain = u16::from(grain);
            data.extend_from_slice(&[
                (base + grain / 3) as u8,
                (3 * base / 4 + grain / 4) as u8,
                (base / 3 + grain / 5) as u8,
                255,
            ]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Nearest,
        ..default()
    });
    image
}

/// Tiling beauty ground for the lake/pond/ponds scenes: sandy grain and a
/// slow mottle, no diagnostic checker and no island-specific radial shape.
fn plateau_texture() -> Image {
    const SIZE: u32 = 256;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let grain = ((13 * x + 7 * y) % 23) as f32 / 23.0;
            let mottle = 0.5
                + 0.5
                    * f32::sin(
                        (x as f32 / SIZE as f32) * std::f32::consts::TAU * 3.0
                            + (y as f32 / SIZE as f32) * std::f32::consts::TAU * 2.0,
                    );
            let shade = 0.82 + 0.10 * grain + 0.08 * mottle;
            let base = [
                (214.0 * shade) as u8,
                (174.0 * shade) as u8,
                (108.0 * shade) as u8,
                255,
            ];
            data.extend_from_slice(&base);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

fn island_texture() -> Image {
    const SIZE: u32 = 512;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let half = 0.5 * TERRAIN_SIZE;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let world_x = -half + (x as f32 + 0.5) * TERRAIN_SIZE / SIZE as f32;
            let world_z = -half + (y as f32 + 0.5) * TERRAIN_SIZE / SIZE as f32;
            let height = terrain_height(world_x, world_z);
            let grain = ((17 * x + 29 * y + 11 * x * y) % 23) as i16 - 11;
            let base = if height < -2.0 {
                [151_i16, 119, 67]
            } else if height < 1.5 {
                [205, 166, 101]
            } else if height < 5.0 {
                [91, 112, 54]
            } else {
                [62, 83, 43]
            };
            data.extend_from_slice(&[
                (base[0] + grain).clamp(0, 255) as u8,
                (base[1] + grain).clamp(0, 255) as u8,
                (base[2] + grain / 2).clamp(0, 255) as u8,
                255,
            ]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Nearest,
        ..default()
    });
    image
}

fn terrain_mesh(scene: Scene, checker: bool) -> Mesh {
    heightfield_mesh(scene_height_fn(scene), checker)
}

/// One height function per terrain scene; the terrain mesh and the
/// [`BedHeightMap`] both sample it on the shared 513-texel grid.
fn scene_height_fn(scene: Scene) -> Box<dyn Fn(f32, f32) -> f32> {
    match scene {
        Scene::Island | Scene::AnimWaves => Box::new(terrain_height),
        Scene::Lake | Scene::ReflectionLake => Box::new(lake_height),
        Scene::Ponds | Scene::PondsMany => Box::new(move |x, z| ponds_height(scene, x, z)),
        Scene::River => {
            let upper = showcase_river_upper();
            let lower = showcase_river_lower();
            Box::new(move |x, z| river_valley_height(&upper, &lower, x, z))
        }
    }
}

/// Builds a sea-floor heightfield from a world-XZ height function.
fn heightfield_mesh(height: impl Fn(f32, f32) -> f32, checker: bool) -> Mesh {
    let count = (TERRAIN_RESOLUTION * TERRAIN_RESOLUTION) as usize;
    let mut positions = Vec::with_capacity(count);
    let mut normals = Vec::with_capacity(count);
    let mut uvs = Vec::with_capacity(count);
    let half = 0.5 * TERRAIN_SIZE;
    for row in 0..TERRAIN_RESOLUTION {
        let z = -half + row as f32 * TERRAIN_STEP;
        for column in 0..TERRAIN_RESOLUTION {
            let x = -half + column as f32 * TERRAIN_STEP;
            positions.push([x, height(x, z), z]);
            let left = height(x - TERRAIN_STEP, z);
            let right = height(x + TERRAIN_STEP, z);
            let back = height(x, z - TERRAIN_STEP);
            let front = height(x, z + TERRAIN_STEP);
            normals.push(
                Vec3::new(left - right, 2.0 * TERRAIN_STEP, back - front)
                    .normalize()
                    .to_array(),
            );
            uvs.push(if checker {
                [x / 8.0, z / 8.0]
            } else {
                [(x + half) / TERRAIN_SIZE, (z + half) / TERRAIN_SIZE]
            });
        }
    }
    let mut indices = Vec::with_capacity(((TERRAIN_RESOLUTION - 1).pow(2) * 6) as usize);
    for row in 0..TERRAIN_RESOLUTION - 1 {
        for column in 0..TERRAIN_RESOLUTION - 1 {
            let index = row * TERRAIN_RESOLUTION + column;
            indices.extend_from_slice(&[
                index,
                index + TERRAIN_RESOLUTION,
                index + 1,
                index + 1,
                index + TERRAIN_RESOLUTION,
                index + TERRAIN_RESOLUTION + 1,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Two raised-rim ponds on a plateau for the localized-water showcase: the
/// plateau occludes the world plane entirely, so ONLY the two bounded
/// `WaterBody` discs render - proof that bodies cost fill where they exist.
fn ponds_height(scene: Scene, x: f32, z: f32) -> f32 {
    const PLATEAU: f32 = 6.0;
    let basin = |center_x: f32, center_z: f32, radius: f32, depth: f32| {
        let distance = ((x - center_x).powi(2) + (z - center_z).powi(2)).sqrt();
        let t = (1.0 - distance / radius).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        depth * smooth
    };
    // Flat bottom out to half radius, then easing banks: wide waterlines.
    let dish = |center_x: f32, center_z: f32, radius: f32, depth: f32| {
        let distance = ((x - center_x).powi(2) + (z - center_z).powi(2)).sqrt();
        let t = ((radius - distance) / (radius * 0.5)).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        depth * smooth
    };
    // Pond A bottoms at -5 m (water level 0); pond B at -1 m (level 3).
    let two = PLATEAU - basin(-40.0, -20.0, 25.0, 11.0) - basin(45.0, 30.0, 18.0, 7.0);
    if scene == Scene::Ponds {
        two
    } else {
        // Ten terraced ponds on a loose grid: each body sits at a visibly
        // different level. Flat-bottomed dishes keep the whole disc wet,
        // easing to the plateau only in the outer band.
        let mut height = PLATEAU;
        for row in 0..2 {
            for column in 0..5 {
                let level = pond_many_level(column, row);
                height -= dish(
                    -120.0 + 60.0 * column as f32,
                    -45.0 + 90.0 * row as f32,
                    26.0,
                    PLATEAU - level + 3.5,
                );
            }
        }
        height
    }
}

/// Surface levels for the ten-pond terrace: columns step 0.8 m apart and
/// rows sit 1.5 m higher, staying under the surrounding plateau.
pub(crate) fn pond_many_level(column: usize, row: usize) -> f32 {
    0.8 * column as f32 + 1.5 * row as f32
}

/// The showcase river centerline: a winding drop from the west hills into
/// the lake basin. Narrow fast sections alternate with calm pools.
/// Upper plateau reach: meanders across the bench, pinches through a
/// narrows, and ends at the chute head where the terrain drops away.
fn showcase_river_upper() -> RiverPath {
    RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(-250.0, -70.0), 16.0, 1.4),
            RiverPoint::new(Vec2::new(-190.0, -30.0), 13.0, 1.9),
            RiverPoint::new(Vec2::new(-125.0, -60.0), 18.0, 1.1),
            RiverPoint::new(Vec2::new(-70.0, -25.0), 12.0, 1.7),
            RiverPoint::new(Vec2::new(-38.0, -42.0), 8.0, 2.8),
            RiverPoint::new(Vec2::new(-14.0, -40.0), 9.0, 3.2),
        ],
    }
}

/// Lower valley reach: catches the drop, widens, and feeds the lake.
fn showcase_river_lower() -> RiverPath {
    RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(16.0, -40.0), 9.0, 3.0),
            RiverPoint::new(Vec2::new(80.0, -22.0), 14.0, 1.9),
            RiverPoint::new(Vec2::new(150.0, 0.0), 18.0, 1.3),
            RiverPoint::new(Vec2::new(205.0, 18.0), 20.0, 0.9),
        ],
    }
}

/// Deterministic rolling relief for the river valley.
fn hills(x: f32, z: f32) -> f32 {
    1.1 * (0.021 * x + 1.3).sin() * (0.017 * z).cos()
        + 0.7 * (0.043 * x + 0.031 * z + 0.7).sin()
        + 0.45 * (0.09 * z + 0.11 * x).sin()
}

const RIVER_PLATEAU_TOP: f32 = 6.0;
const RIVER_VALLEY_FLOOR: f32 = 0.5;

fn smoothstep_0_1(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Carves one reach into `base`: the bed sits a width-scaled depth below the
/// reach level, easing up to the surrounding terrain over a bank blend band.
fn reach_carve(path: &RiverPath, level: f32, x: f32, z: f32, base: f32) -> Option<f32> {
    let sampled = path.sample(Vec2::new(x, z))?;
    if sampled.distance > sampled.half_width + 8.0 {
        return None;
    }
    // Narrow = proportionally deeper, but shallow enough for a clear
    // mountain-stream read: the bed shows through everywhere.
    let bed = level - (0.9 + 0.09 * sampled.half_width * 2.0);
    let blend = smoothstep_0_1((sampled.half_width + 8.0 - sampled.distance) / 8.0);
    Some(base + (bed - base) * blend)
}

/// River-valley terrain: a plateau bench in the west falling to a valley in
/// the east through a chute, both reaches carved into it, with the lake bowl
/// where the lower reach ends.
fn river_valley_height(upper: &RiverPath, lower: &RiverPath, x: f32, z: f32) -> f32 {
    // Plateau-to-valley transition centred on the chute (x ~ 0).
    let drop = smoothstep_0_1((x + 18.0) / 36.0);
    let base = RIVER_PLATEAU_TOP * (1.0 - drop)
        + RIVER_VALLEY_FLOOR * drop
        + hills(x, z) * (0.6 + 0.5 * drop);

    // Lake basin (absolute heights): centre -9 m, rim -1 m at r=55, crest
    // rising to the valley plateau by r=90.
    let lake_x = x - 235.0;
    let lake_z = z - 20.0;
    let lake_radius = (lake_x * lake_x + lake_z * lake_z).sqrt();
    let lake = if lake_radius < 55.0 {
        -9.5 + 8.5 * (lake_radius / 55.0).powi(2) + RIVER_VALLEY_FLOOR
    } else if lake_radius < 90.0 {
        let smooth = smoothstep_0_1((lake_radius - 55.0) / 35.0);
        -1.0 + 6.0 * smooth + RIVER_VALLEY_FLOOR
    } else {
        // Beyond the rim the basin must not influence the relief.
        f32::INFINITY
    };

    // Chute corridor between the reaches: the bed descends from the upper
    // bed elevation to the lower one, forming the rocky drop.
    let chute_t = ((x + 14.0) / 30.0).clamp(0.0, 1.0);
    // Same width-scaled bed rule as `reach_carve` at the chute-head width.
    let chute_bed_rule = 0.9 + 0.09 * 9.0 * 2.0;
    let upper_bed_at_chute = 5.0 - chute_bed_rule;
    let lower_bed_at_chute = -chute_bed_rule;
    let chute_bed =
        upper_bed_at_chute + (lower_bed_at_chute - upper_bed_at_chute) * smoothstep_0_1(chute_t);
    let dz = z + 40.0;
    let chute_half = 6.5;
    let chute = if dz.abs() < chute_half + 6.0 {
        let blend = smoothstep_0_1((chute_half + 6.0 - dz.abs()) / 6.0);
        Some(base + (chute_bed - base) * blend)
    } else {
        None
    };

    let upper_carve = reach_carve(upper, 5.0, x, z, base);
    let lower_carve = reach_carve(lower, 0.0, x, z, base);
    let mut height = base.min(lake);
    if let Some(candidate) = chute {
        height = height.min(candidate);
    }
    if let Some(candidate) = upper_carve {
        height = height.min(candidate);
    }
    if let Some(candidate) = lower_carve {
        height = height.min(candidate);
    }
    height
}

fn terrain_height(x: f32, z: f32) -> f32 {
    let ellipse = (x / 58.0).powi(2) + (z / 42.0).powi(2);
    let island = 28.0 * (-ellipse).exp();
    let shoal = 2.0 * (-((x - 35.0) / 15.0).powi(2) - ((z + 18.0) / 8.0).powi(2)).exp();
    -20.0 + island + shoal
}

/// Raised-rim basin for the lake/pond recipe: a -9 m bowl easing to the
/// shore, a crest rising above sea level between r = 55..90 m, and a plateau
/// that occludes the world plane out to the grid edge so the water reads as
/// bounded without any clipping machinery.
fn lake_height(x: f32, z: f32) -> f32 {
    let radius = (x * x + z * z).sqrt();
    if radius < 55.0 {
        -9.0 + 8.0 * (radius / 55.0).powi(2)
    } else if radius < 90.0 {
        let t = ((radius - 55.0) / 35.0).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        -1.0 + 6.0 * smooth
    } else {
        5.0
    }
}

fn collect_gpu_profile(
    diagnostics: Res<DiagnosticsStore>,
    waves: Res<OceanWaves>,
    demo: Res<Demo>,
    profile: Option<ResMut<GpuProfile>>,
    mut exit: MessageWriter<AppExit>,
) {
    const WARMUP_FRAMES: u32 = 300;
    const SAMPLE_COUNT: usize = 300;
    let Some(mut profile) = profile else {
        return;
    };
    profile.warmup_frames += 1;
    if profile.warmup_frames == 30 || profile.warmup_frames == WARMUP_FRAMES {
        let mut paths = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.path().as_str().ends_with("elapsed_gpu"))
            .map(|diagnostic| diagnostic.path().as_str())
            .collect::<Vec<_>>();
        paths.sort_unstable();
        eprintln!("GPU diagnostic paths: {paths:?}");
    }
    if profile.warmup_frames <= WARMUP_FRAMES {
        return;
    }

    let paths = [
        DiagnosticPath::new("render/main_transmissive_pass_3d/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_cascade_compute/elapsed_gpu"),
        DiagnosticPath::new("render/early prepass/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_seafloor_copy/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_foam_compute/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_fft_evolve/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_fft_horizontal/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_fft_vertical/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_fft_resolve/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_fft_surface/elapsed_gpu"),
        DiagnosticPath::new("render/aqua_cascade_combine/elapsed_gpu"),
        DiagnosticPath::new("render/bloom/elapsed_gpu"),
    ];
    let frame_anchor = DiagnosticPath::new("render/upscaling/elapsed_gpu");
    let Some(anchor) = diagnostics.get(&frame_anchor) else {
        return;
    };
    let Some(latest) = anchor.measurements().last() else {
        return;
    };
    let measurement_time = latest.time;
    if profile
        .last_measurement
        .is_some_and(|last| measurement_time <= last)
    {
        return;
    }
    let values = paths.map(|path| {
        diagnostics
            .get(&path)
            .into_iter()
            .flat_map(|diagnostic| diagnostic.measurements())
            .filter(|measurement| measurement.time == measurement_time)
            .map(|measurement| measurement.value)
            .collect::<Vec<_>>()
    });
    let whole_frame = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path().as_str().ends_with("elapsed_gpu"))
        .flat_map(|diagnostic| diagnostic.measurements())
        .filter(|measurement| measurement.time == measurement_time)
        .map(|measurement| measurement.value)
        .sum::<f64>();
    if whole_frame <= 0.0 {
        return;
    }
    if demo.water_enabled
        && (values[0].len() != 1
            || values[2].len() != values[3].len() + 1
            || values[3].len() > SEA_FLOOR_CASCADE_COUNT
            || values[4].len() != 1
            || values[11].len() > 1
            || (waves.model == WaveModel::Analytic
                && (values[1].len() != 1
                    || values[5..11]
                        .iter()
                        .any(|measurements| !measurements.is_empty())))
            || (waves.model == WaveModel::Spectral
                && values[5..11]
                    .iter()
                    .any(|measurements| measurements.len() != 1)))
    {
        return;
    }
    profile.last_measurement = Some(measurement_time);
    if demo.water_enabled {
        let cascade_compute = if waves.model == WaveModel::Spectral {
            values[5..11]
                .iter()
                .map(|measurements| measurements[0])
                .sum()
        } else {
            values[1][0]
        };
        profile.samples.push([
            values[0][0],
            cascade_compute,
            values[2][..values[3].len()].iter().sum::<f64>() + values[3].iter().sum::<f64>(),
            values[4][0],
            values[5].first().copied().unwrap_or(0.0),
            values[6].first().copied().unwrap_or(0.0),
            values[7].first().copied().unwrap_or(0.0),
            values[8].first().copied().unwrap_or(0.0),
            values[9].first().copied().unwrap_or(0.0),
            values[10].first().copied().unwrap_or(0.0),
            values[11].first().copied().unwrap_or(0.0),
            whole_frame,
        ]);
    } else {
        profile.samples.push([
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            whole_frame,
        ]);
    }
    if profile.samples.len() != SAMPLE_COUNT {
        return;
    }

    let columns: [Vec<f64>; 12] = std::array::from_fn(|column| {
        let mut values = profile
            .samples
            .iter()
            .map(|sample| sample[column])
            .collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        values
    });
    let medians: [f64; 12] = std::array::from_fn(|column| {
        let values = &columns[column];
        let upper = values.len() / 2;
        0.5 * (values[upper - 1] + values[upper])
    });
    println!(
        "GPU_SPANS ocean vertex+fragment: {:.4} ms | cascade compute: {:.4} ms | capture passes: {:.4} ms | foam compute: {:.4} ms | whole reported span sum: {:.4} ms",
        medians[0], medians[1], medians[2], medians[3], medians[11],
    );
    println!("BLOOM_SPAN {:.4} ms", medians[10]);
    if waves.model == WaveModel::Spectral {
        println!(
            "FFT_SUBSPANS evolve: {:.4} ms | horizontal: {:.4} ms | vertical: {:.4} ms | resolve: {:.4} ms | surface: {:.4} ms | combine+gather: {:.4} ms",
            medians[4], medians[5], medians[6], medians[7], medians[8], medians[9],
        );
    }
    println!(
        "PROFILE_MATRIX_ROW|{}",
        medians
            .iter()
            .map(|value| format!("{value:.6}"))
            .collect::<Vec<_>>()
            .join("|")
    );
    exit.write(AppExit::Success);
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct CubemapProbeMaterial {}

impl Material for CubemapProbeMaterial {
    fn vertex_shader() -> ShaderRef {
        "cubemap_probe.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "cubemap_probe.wgsl".into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
enum ProbeFraming {
    Horizon,
    Sweep,
}

impl ProbeFraming {
    fn fov(self) -> f32 {
        match self {
            Self::Horizon => 20.0_f32.to_radians(),
            Self::Sweep => 100.0_f32.to_radians(),
        }
    }
}

#[derive(Component)]
struct FlyingCamera;

/// Visual-only wave follower for the local test prop; this is not buoyancy.
#[derive(Component)]
struct ApproximateBuoyMotion {
    rest_xz: Vec2,
}

#[derive(Component)]
struct BuoyLampMarker;

#[derive(Component)]
struct RotatingBuoyBeacon;

fn toggle_wave_model(keys: Res<ButtonInput<KeyCode>>, mut waves: ResMut<OceanWaves>) {
    if keys.just_pressed(KeyCode::KeyF) {
        waves.model = match waves.model {
            WaveModel::Analytic => WaveModel::Spectral,
            WaveModel::Spectral => WaveModel::Analytic,
        };
        info!("AnimWaves model: {:?}", waves.model);
    }
}

fn fly_camera(
    demo: Res<Demo>,
    time: Res<Time>,
    // Terrain scenes spawn no flying camera.
    camera: Option<Single<&mut Transform, With<FlyingCamera>>>,
) {
    let Some(mut camera) = camera else {
        return;
    };
    if demo.scene != Scene::AnimWaves || demo.frozen_camera || demo.open.boundary {
        return;
    }
    let elapsed = time.elapsed_secs();
    let motion = Vec2::new(
        elapsed * FLIGHT_SPEED,
        (elapsed * FLIGHT_SWAY_SPEED).sin() * FLIGHT_SWAY_DISTANCE,
    );
    **camera = open_ocean_camera(&demo.open, motion);
}

#[derive(Debug, Clone, Copy)]
struct BuoyOptions {
    lamp_active: bool,
    spot: bool,
    underwater_light: bool,
    scale: f32,
}

fn spawn_test_buoy(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    options: BuoyOptions,
) {
    assert!(
        Path::new("assets/test/ocean_buoy.glb").is_file(),
        "reflection-lake/--buoy requires local assets/test/ocean_buoy.glb; this CC0 test asset is not packaged",
    );
    let lamp_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.25, 0.04),
        emissive: if options.lamp_active {
            LinearRgba::rgb(8.0, 1.2, 0.12)
        } else {
            LinearRgba::BLACK
        },
        ..default()
    });
    commands
        .spawn((
            ApproximateBuoyMotion {
                rest_xz: Vec2::ZERO,
            },
            Transform::from_scale(Vec3::splat(options.scale)),
            Visibility::default(),
            #[cfg(feature = "reflect")]
            ReflectedInWater,
            #[cfg(not(feature = "reflect"))]
            (),
        ))
        .with_children(|parent| {
            parent.spawn(WorldAssetRoot(
                asset_server.load(GltfAssetLabel::Scene(0).from_asset("test/ocean_buoy.glb")),
            ));
            parent.spawn((
                BuoyLampMarker,
                Mesh3d(meshes.add(Sphere::new(0.055))),
                MeshMaterial3d(lamp_material),
                Transform::from_xyz(0.0, BUOY_LAMP_HEIGHT, 0.0),
            ));
            if options.lamp_active {
                // A modest omnidirectional practical keeps the painted frame
                // readable while the stronger beacon owns the rotating beam.
                parent.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.4, 0.12),
                        intensity: 300.0,
                        range: 10.0,
                        radius: 0.06,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, BUOY_LAMP_HEIGHT, 0.0),
                ));
            }
            if options.lamp_active && options.spot {
                parent.spawn((
                    RotatingBuoyBeacon,
                    SpotLight {
                        color: Color::srgb(1.0, 0.6, 0.25),
                        intensity: BUOY_BEACON_LUMENS,
                        range: BUOY_BEACON_RANGE,
                        radius: 0.06,
                        inner_angle: 0.10,
                        outer_angle: 0.22,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    buoy_beacon_transform(0.0),
                ));
            }
            if options.underwater_light {
                parent.spawn((
                    PointLight {
                        color: Color::srgb(0.04, 0.38, 1.0),
                        intensity: BUOY_UNDERWATER_LIGHT_LUMENS,
                        range: 12.0,
                        radius: 0.12,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, -BUOY_UNDERWATER_LIGHT_DEPTH, 0.0),
                ));
            }
        });
}

fn animate_test_buoy(
    time: Res<Time<Virtual>>,
    mut buoys: Query<(&ApproximateBuoyMotion, &mut Transform)>,
) {
    let seconds = time.elapsed_secs();
    for (motion, mut transform) in &mut buoys {
        let (height, surface_normal) = approximate_buoy_surface(seconds, motion.rest_xz);
        transform.translation = Vec3::new(motion.rest_xz.x, height, motion.rest_xz.y);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, surface_normal);
    }
}

fn rotate_buoy_beacon(
    time: Res<Time<Virtual>>,
    mut beacons: Query<&mut Transform, With<RotatingBuoyBeacon>>,
) {
    for mut transform in &mut beacons {
        *transform = buoy_beacon_transform(time.elapsed_secs());
    }
}

fn buoy_beacon_transform(seconds: f32) -> Transform {
    let phase = seconds * std::f32::consts::TAU / BUOY_BEACON_PERIOD_SECONDS;
    let direction = Vec3::new(phase.cos(), -BUOY_BEACON_DOWNWARD_SLOPE, phase.sin()).normalize();
    let position = Vec3::new(0.0, BUOY_LAMP_HEIGHT, 0.0);
    Transform::from_translation(position).looking_at(position + direction, Vec3::Y)
}

/// Two analytic wave components for presentation only; this does not query
/// Aqua's GPU displacement and is not a buoyancy implementation.
fn approximate_buoy_surface(seconds: f32, rest_xz: Vec2) -> (f32, Vec3) {
    let phase_a = 0.72 * seconds + 0.19 * rest_xz.x + 0.11 * rest_xz.y;
    let phase_b = 1.13 * seconds - 0.08 * rest_xz.x + 0.23 * rest_xz.y;
    let height = 0.16 * phase_a.sin() + 0.07 * phase_b.sin();
    let slope = Vec2::new(
        0.16 * 0.19 * phase_a.cos() - 0.07 * 0.08 * phase_b.cos(),
        0.16 * 0.11 * phase_a.cos() + 0.07 * 0.23 * phase_b.cos(),
    );
    let surface_normal = Vec3::new(-slope.x, 1.0, -slope.y).normalize();
    (height, surface_normal)
}

fn tune_imported_buoy_material(
    demo: Res<Demo>,
    mut complete: Local<bool>,
    asset_server: Res<AssetServer>,
    mut mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>, Without<BuoyLampMarker>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !(demo.open.buoy || demo.scene == Scene::ReflectionLake) || *complete {
        return;
    }
    for mut mesh_material in &mut mesh_materials {
        let Some(path) = asset_server.get_path(mesh_material.id()) else {
            continue;
        };
        if !path.path().ends_with("test/ocean_buoy.glb") {
            continue;
        }
        let Some(mut material) = materials.get(mesh_material.id()).cloned() else {
            continue;
        };
        // The source has one strength-10 emissive material. Clone it so the
        // test's real lamp and small lens marker exclusively own radiance.
        material.emissive = LinearRgba::BLACK;
        material.emissive_texture = None;
        mesh_material.0 = materials.add(material);
        *complete = true;
    }
}

/// Sun rotation from elevation/azimuth degrees.
fn sun_transform(settings: LightingSettings) -> Transform {
    let elevation = settings.elevation_degrees.to_radians();
    let azimuth = settings.azimuth_degrees.to_radians();
    let direction_to_light = Vec3::new(
        elevation.cos() * azimuth.cos(),
        elevation.sin(),
        elevation.cos() * azimuth.sin(),
    );
    Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, direction_to_light))
}

fn open_ocean_camera(open: &OpenOcean, centre: Vec2) -> Transform {
    let centre = Vec3::new(centre.x, 0.0, centre.y);
    if open.boundary {
        return Transform::from_translation(centre + BOUNDARY_OFFSET)
            .looking_at(centre + BOUNDARY_TARGET, Vec3::Y);
    }
    if open.buoy {
        return Transform::from_translation(centre + BUOY_CAMERA_POSITION)
            .looking_at(centre + BUOY_CAMERA_TARGET, Vec3::Y);
    }
    if open.detail_close {
        return Transform::from_translation(centre + Vec3::new(0.0, 3.0, 7.0))
            .looking_at(centre, Vec3::Y);
    }

    let (distance, look_ahead) = match open.height {
        height if height < 10.0 => (32.0, 0.8),
        height if height < 100.0 => (2.0 * height, 0.8),
        height => (0.8 * height, 0.0),
    };
    let position = centre + Vec3::new(0.0, open.height, distance);
    let target = centre + Vec3::new(0.0, 0.0, -look_ahead * distance);
    Transform::from_translation(position).looking_at(target, Vec3::Y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_pose_labels_are_unique_across_the_matrix() {
        let mut labels = ProfilePose::ALL
            .into_iter()
            .map(ProfilePose::label)
            .collect::<Vec<_>>();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn far_dolly_moves_slowly_across_the_tier_boundary() {
        let first = far_dolly_camera(0);
        let middle = far_dolly_camera(88);
        let last = far_dolly_camera(176);
        assert_eq!(first.translation, Vec3::new(220.0, 20.0, 250.0));
        assert_eq!(middle.translation, Vec3::new(220.0, 20.0, 426.0));
        assert_eq!(last.translation, Vec3::new(220.0, 20.0, 602.0));
        assert_eq!(
            middle.translation - first.translation,
            last.translation - middle.translation
        );
    }

    #[test]
    fn open_ocean_profile_altitudes_are_exact() {
        let altitude = |pose| profile_camera(pose).translation.y;
        assert_eq!(altitude(ProfilePose::OpenOcean2m), 2.0);
        assert_eq!(altitude(ProfilePose::OpenOcean50m), 50.0);
        assert_eq!(altitude(ProfilePose::OpenOcean500m), 500.0);
    }

    #[test]
    fn river_valley_carves_reaches_below_their_levels_with_banks_above() {
        let upper = showcase_river_upper();
        let lower = showcase_river_lower();
        let height = |x: f32, z: f32| river_valley_height(&upper, &lower, x, z);
        // Upper-reach beds sit below the 5 m plateau water plane; banks rise
        // above it.
        for x in [-250.0, -190.0, -125.0, -70.0] {
            for z in [-70.0, -30.0, -60.0, -25.0] {
                let Some(sampled) = upper.sample(Vec2::new(x, z)) else {
                    continue;
                };
                if sampled.within_bank() {
                    let bed = height(x, z);
                    assert!(
                        bed < 3.6,
                        "upper bed at ({x},{z}) is {bed}, must be submerged under level 5"
                    );
                }
            }
        }
        // Lower-reach beds sit below level 0.
        for x in [80.0, 150.0] {
            let Some(sampled) = lower.sample(Vec2::new(x, -10.0)) else {
                panic!("lower reach should cover ({x}, -10)");
            };
            if sampled.within_bank() {
                let bed = height(x, -10.0);
                assert!(bed < -1.0, "lower bed at ({x},-10) is {bed}");
            }
        }
        // Flow runs downstream on both reaches (positive X overall).
        assert!(upper.sample(Vec2::new(-125.0, -60.0)).unwrap().flow.x > 0.0);
        assert!(lower.sample(Vec2::new(150.0, 0.0)).unwrap().flow.x > 0.0);
        // Lake bed below its water level; rim above.
        assert!(height(235.0, 20.0) < -8.0);
        assert!(height(235.0, -65.0) > 1.0);
        // The chute descends between the reaches: bed falls from above the
        // upper bed to below the lower one across the corridor.
        let chute_head = height(0.0, -40.0);
        assert!(
            chute_head < upper_bed_at(9.0),
            "chute bed must drop below the upper bed elevation, got {chute_head}"
        );
        // Far corners stay on the valley/plateau relief, above both water
        // planes (5 m plateau, 0 m valley).
        assert!(height(-299.0, 299.0) > 5.5);
        assert!(height(299.0, 299.0) > 0.2);
    }

    fn upper_bed_at(half_width: f32) -> f32 {
        5.0 - (0.9 + 0.09 * half_width * 2.0)
    }
    #[test]
    fn ponds_heightfield_carves_two_basins_below_their_levels() {
        // Pond A: level 0 water needs bed below 0; plateau stays at 6 m.
        assert!(ponds_height(Scene::Ponds, -40.0, -20.0) < -4.5);
        assert!(ponds_height(Scene::Ponds, -40.0, -9.0) < 0.0); // inside the shoreline
        // Pond B: level 3 water needs bed below 3.
        assert!(ponds_height(Scene::Ponds, 45.0, 30.0) < -0.5);
        assert!(ponds_height(Scene::Ponds, 45.0, 36.0) < 3.0);
        // Plateau between and beyond both basins occludes the world plane.
        assert_eq!(ponds_height(Scene::Ponds, 0.0, 5.0), 6.0);
        assert_eq!(ponds_height(Scene::Ponds, -299.0, 299.0), 6.0);
        // Ten-pond terrace: every bed sits 3.5 m under its own level, and
        // the shared ground between basins stays at the plateau.
        for row in 0..2 {
            for column in 0..5 {
                let x = -120.0 + 60.0 * column as f32;
                let z = -45.0 + 90.0 * row as f32;
                let level = pond_many_level(column, row);
                assert!(
                    ponds_height(Scene::PondsMany, x, z) < level - 3.0,
                    "bed at ({x},{z}) must sit below its level {level}"
                );
            }
        }
        assert_eq!(ponds_height(Scene::PondsMany, 0.0, 0.0), 6.0);
    }

    #[test]
    fn lake_heightfield_bounds_water_with_a_rim() {
        // The bowl stays below sea level and deepens toward the centre.
        assert!(lake_height(0.0, 0.0) < -8.0);
        assert!(lake_height(40.0, 0.0) < -1.5);
        // The rim rises monotonically above sea level across its band.
        let mut previous = lake_height(55.0, 0.0);
        assert!(previous >= -1.5);
        for step in 1..=35 {
            let current = lake_height(55.0 + step as f32, 0.0);
            assert!(
                current > previous,
                "rim must rise: r={}",
                55.0 + step as f32
            );
            previous = current;
        }
        // The plateau occludes the world plane everywhere past the crest.
        assert_eq!(lake_height(120.0, 0.0), 5.0);
        assert_eq!(lake_height(299.0, -299.0), 5.0);
    }
    #[test]
    fn approximate_buoy_motion_is_bounded_and_tilts() {
        let (height_a, normal_a) = approximate_buoy_surface(0.0, Vec2::ZERO);
        let (height_b, normal_b) = approximate_buoy_surface(1.0, Vec2::ZERO);
        assert!(height_a.abs() <= 0.23 && height_b.abs() <= 0.23);
        assert!((normal_a.length() - 1.0).abs() < 1e-6);
        assert!((normal_b.length() - 1.0).abs() < 1e-6);
        assert_ne!(height_a, height_b);
        assert_ne!(normal_a, normal_b);
    }

    #[test]
    fn focused_beacon_rotates_and_keeps_a_downward_slope() {
        let direction_a = *buoy_beacon_transform(0.0).forward();
        let direction_b = *buoy_beacon_transform(0.25 * BUOY_BEACON_PERIOD_SECONDS).forward();
        assert!(direction_a.y < 0.0 && direction_b.y < 0.0);
        let horizontal_a = direction_a.xz().normalize();
        let horizontal_b = direction_b.xz().normalize();
        assert!(horizontal_a.dot(horizontal_b).abs() < 1e-5);
    }
}

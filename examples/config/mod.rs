//! Declarative CLI and scene presets for the showcase.

use super::*;
use clap::{Args as ClapArgs, Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "showcase",
    about = "Aqua water scenes, diagnostics, captures, and GPU profiles",
    args_override_self = true,
    after_long_help = "Interactive anim-waves keys: F toggles the wave model; L cycles water optics.\n\
The reflection-lake scene and buoy flags require assets/test/ocean_buoy.glb."
)]
pub(super) struct ShowcaseArgs {
    #[command(flatten, next_help_heading = "Presentation")]
    presentation: PresentationArgs,
    #[command(flatten, next_help_heading = "Runtime")]
    runtime: RuntimeArgs,
    #[command(flatten, next_help_heading = "Capture and profiling")]
    capture: CaptureArgs,
    #[command(flatten, next_help_heading = "Water and waves")]
    water: WaterArgs,
    #[command(flatten, next_help_heading = "Open-ocean scene")]
    open: OpenOceanArgs,
    /// Ocean spray quality (requires the `spray` feature).
    #[cfg(feature = "spray")]
    #[arg(
        long,
        value_enum,
        default_value = "off",
        help_heading = "Water and waves"
    )]
    spray: SprayChoice,
}

#[derive(Debug, ClapArgs)]
struct PresentationArgs {
    /// Scene recipe to render.
    #[arg(long, value_enum, default_value = "island")]
    scene: Scene,
    /// Material output or diagnostic view.
    #[arg(long, value_enum, default_value = "composite")]
    pub(super) debug: DebugChoice,
    /// Directional light and exposure preset.
    #[arg(long, value_enum, default_value = "day")]
    lighting: Lighting,
    /// Override the scene's water-optics preset.
    #[arg(long, value_enum)]
    water_optics: Option<WaterOpticsChoice>,
}

#[derive(Debug, ClapArgs)]
struct RuntimeArgs {
    /// Disable the water plugin and primary water surface.
    #[arg(long)]
    no_water: bool,
    /// Use the close shoreline camera.
    #[arg(long)]
    near_shore: bool,
    /// Use the close diagnostic camera.
    #[arg(long)]
    close_up: bool,
    /// Use the unlit checker diagnostic terrain.
    #[arg(long)]
    checker: bool,
    /// Hide showcase labels.
    #[arg(long)]
    ui_off: bool,
    /// Disable showcase bloom.
    #[arg(long)]
    bloom_off: bool,
}

#[derive(Debug, ClapArgs)]
struct CaptureArgs {
    /// Run without a window surface.
    #[arg(long)]
    pub(super) headless: bool,
    /// Save one settled frame.
    #[arg(long, value_name = "PATH")]
    pub(super) screenshot: Option<PathBuf>,
    /// Freeze simulation at this time in seconds.
    #[arg(long, value_parser = nonnegative_f32)]
    time: Option<f32>,
    #[command(flatten)]
    profile: ProfileArgs,
    #[command(flatten)]
    sequence: SequenceArgs,
}

#[derive(Debug, ClapArgs)]
struct ProfileArgs {
    /// Print GPU pass timing for one pose.
    #[arg(long)]
    gpu_profile: bool,
    /// Profile with the reduced active cadence.
    #[arg(long)]
    gpu_profile_active: bool,
    /// Run the full pose/backend profile matrix.
    #[arg(long)]
    profile_matrix: bool,
    /// Fixed camera pose for profiling.
    #[arg(long, value_enum)]
    profile_pose: Option<ProfilePose>,
    /// Profile render size as WIDTHxHEIGHT.
    #[arg(long, value_parser = resolution)]
    resolution: Option<UVec2>,
}

#[derive(Debug, ClapArgs)]
struct SequenceArgs {
    /// Capture the flow sequence into this directory.
    #[arg(long, value_name = "DIR")]
    capture_sequence: Option<PathBuf>,
    /// Capture the complete far-tier dolly into this directory.
    #[arg(long, value_name = "DIR")]
    far_dolly_sequence: Option<PathBuf>,
    /// Select one far-tier dolly step in 1..=176 (requires --screenshot).
    #[arg(long, value_parser = positive_u32)]
    far_dolly_step: Option<u32>,
}

#[derive(Debug, ClapArgs)]
struct WaterArgs {
    #[command(flatten)]
    pub(super) waves: WaveArgs,
    /// Reflection source.
    #[arg(long, value_enum)]
    reflections: Option<ReflectionChoice>,
    /// Disable procedural bed caustics.
    #[arg(long)]
    caustics_off: bool,
}

#[derive(Debug, ClapArgs)]
struct WaveArgs {
    /// Wave synthesis backend.
    #[arg(long, value_enum, default_value = "gerstner")]
    wave_backend: WaveBackend,
    /// Wave-energy preset. Scene defaults apply when omitted.
    #[arg(long, value_enum)]
    sea_state: Option<SeaStateChoice>,
    /// Disable depth-based shallow-water attenuation.
    #[arg(long)]
    no_attenuation: bool,
    /// JONSWAP wind speed in metres per second.
    #[arg(long, value_parser = positive_f32)]
    wind_speed: Option<f32>,
    /// JONSWAP fetch distance in metres.
    #[arg(long, value_parser = positive_f32)]
    fetch: Option<f32>,
    /// Wind direction in world-space degrees.
    #[arg(long, allow_hyphen_values = true, value_parser = finite_f32)]
    wind_degrees: Option<f32>,
    /// Global current in metres per second as X Z.
    #[arg(
        long,
        num_args = 2,
        value_names = ["X", "Z"],
        allow_hyphen_values = true,
        value_parser = finite_f32,
        action = clap::ArgAction::Set
    )]
    flow: Option<Vec<f32>>,
}

#[derive(Debug, ClapArgs)]
struct OpenOceanArgs {
    #[command(flatten)]
    view: OpenViewArgs,
    #[command(flatten)]
    buoy: BuoyArgs,
    #[command(flatten)]
    probe: ProbeArgs,
}

#[derive(Debug, ClapArgs)]
struct OpenViewArgs {
    /// Open-ocean camera height in metres.
    #[arg(long, default_value = "2", value_parser = positive_f32)]
    height: f32,
    /// Frame the LOD 0/1 boundary.
    #[arg(long)]
    boundary: bool,
    /// Use the close detail framing.
    #[arg(long)]
    detail_close: bool,
    /// Offset the open-ocean camera by X Z metres.
    #[arg(
        long,
        num_args = 2,
        value_names = ["X", "Z"],
        allow_hyphen_values = true,
        value_parser = finite_f32,
        action = clap::ArgAction::Set
    )]
    camera_offset: Option<Vec<f32>>,
    /// Scale direct-light illuminance.
    #[arg(long, default_value = "1", value_parser = nonnegative_f32)]
    light_scale: f32,
    /// Add an exposure offset in EV.
    #[arg(
        long,
        default_value = "0",
        allow_hyphen_values = true,
        value_parser = finite_f32
    )]
    exposure_offset: f32,
}

#[derive(Debug, ClapArgs)]
struct BuoyArgs {
    /// Show the buoy with its rotating spot beacon.
    #[arg(long)]
    buoy: bool,
    /// Show the buoy with a point beacon.
    #[arg(long, conflicts_with = "buoy_spot")]
    buoy_point: bool,
    /// Show the buoy with its rotating spot beacon.
    #[arg(long, conflicts_with = "buoy_point")]
    buoy_spot: bool,
    /// Disable the buoy lamp.
    #[arg(long)]
    buoy_lamp_off: bool,
    /// Add the submerged buoy light.
    #[arg(long)]
    buoy_underwater_light: bool,
}

#[derive(Debug, ClapArgs)]
struct ProbeArgs {
    /// Show a water cubemap probe framing.
    #[arg(long, value_enum, conflicts_with = "sky_only")]
    cubemap_probe: Option<ProbeFraming>,
    /// Show the atmosphere without the water surface.
    #[arg(long, value_enum)]
    sky_only: Option<ProbeFraming>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DebugChoice {
    Path,
    Validity,
    Transmission,
    Unrefracted,
    Fog,
    SeaFloor,
    Composite,
    Reflection,
    ReflectionFraction,
    LightRadianceProbe,
    WaveHeight,
    FarTier,
    Foam,
    FoamBilinear,
}

impl From<DebugChoice> for AquaDebug {
    fn from(value: DebugChoice) -> Self {
        match value {
            DebugChoice::Path => Self::WaterPath,
            DebugChoice::Validity => Self::RefractionValidity,
            DebugChoice::Transmission => Self::Transmission,
            DebugChoice::Unrefracted => Self::TransmissionUnrefracted,
            DebugChoice::Fog => Self::BeerLambert,
            DebugChoice::SeaFloor => Self::SeaFloorDepth,
            DebugChoice::Composite => Self::ShallowComposite,
            DebugChoice::Reflection => Self::ReflectionSanity,
            DebugChoice::ReflectionFraction => Self::ReflectionFraction,
            DebugChoice::LightRadianceProbe => Self::LightRadiance,
            DebugChoice::WaveHeight => Self::WaveHeight,
            DebugChoice::FarTier => Self::FarTier,
            DebugChoice::Foam => Self::FoamDensity,
            DebugChoice::FoamBilinear => Self::FoamDensityBilinear,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum WaterOpticsChoice {
    DeepOcean,
    Coastal,
    Tropical,
    ClearFresh,
}

impl From<WaterOpticsChoice> for WaterOptics {
    fn from(value: WaterOpticsChoice) -> Self {
        match value {
            WaterOpticsChoice::DeepOcean => Self::DEEP_OCEAN,
            WaterOpticsChoice::Coastal => Self::COASTAL,
            WaterOpticsChoice::Tropical => Self::TROPICAL,
            WaterOpticsChoice::ClearFresh => Self::CLEAR_FRESH,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum WaveBackend {
    Gerstner,
    Fft,
}

impl From<WaveBackend> for WaveModel {
    fn from(value: WaveBackend) -> Self {
        match value {
            WaveBackend::Gerstner => Self::Analytic,
            WaveBackend::Fft => Self::Spectral,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SeaStateChoice {
    Calm,
    Moderate,
    Rough,
}

impl From<SeaStateChoice> for SeaState {
    fn from(value: SeaStateChoice) -> Self {
        match value {
            SeaStateChoice::Calm => Self::Calm,
            SeaStateChoice::Moderate => Self::Moderate,
            SeaStateChoice::Rough => Self::Rough,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReflectionChoice {
    Cubemap,
    Planar,
}

impl From<ReflectionChoice> for ReflectionMode {
    fn from(value: ReflectionChoice) -> Self {
        match value {
            ReflectionChoice::Cubemap => Self::Cubemap,
            ReflectionChoice::Planar => Self::Planar {
                scale: 0.5,
                distortion: 0.02,
            },
        }
    }
}

#[cfg(feature = "spray")]
#[derive(Debug, Clone, Copy, ValueEnum)]
enum SprayChoice {
    Off,
    Low,
    High,
}

#[cfg(feature = "spray")]
impl From<SprayChoice> for SprayQuality {
    fn from(value: SprayChoice) -> Self {
        match value {
            SprayChoice::Off => Self::Off,
            SprayChoice::Low => Self::Low,
            SprayChoice::High => Self::High,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct WavePreset {
    sea_state: Option<SeaState>,
    flow: Option<Vec2>,
    wind_degrees: Option<f32>,
    wind_speed: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ScenePreset {
    waves: WavePreset,
    water_optics: Option<WaterOptics>,
    detail_strength: Option<f32>,
    atmospheric_sunlight_at_sunset: bool,
}

impl Scene {
    fn preset(self) -> ScenePreset {
        match self {
            Self::Lake => ScenePreset {
                waves: WavePreset {
                    sea_state: Some(SeaState::Calm),
                    flow: Some(Vec2::new(1.2, 0.0)),
                    wind_degrees: Some(25.0),
                    ..default()
                },
                ..default()
            },
            Self::ReflectionLake => ScenePreset {
                waves: WavePreset {
                    sea_state: Some(SeaState::Calm),
                    flow: Some(Vec2::ZERO),
                    wind_degrees: Some(REFLECTION_LAKE_WIND_DEGREES),
                    wind_speed: Some(REFLECTION_LAKE_WIND_SPEED),
                },
                detail_strength: Some(REFLECTION_LAKE_DETAIL_STRENGTH),
                ..default()
            },
            Self::River => ScenePreset {
                waves: WavePreset {
                    sea_state: Some(SeaState::Calm),
                    wind_degrees: Some(20.0),
                    ..default()
                },
                water_optics: Some(WaterOptics::CLEAR_FRESH),
                ..default()
            },
            Self::Ponds | Self::PondsMany => ScenePreset {
                water_optics: Some(WaterOptics::CLEAR_FRESH),
                ..default()
            },
            Self::AnimWaves => ScenePreset {
                atmospheric_sunlight_at_sunset: true,
                ..default()
            },
            Self::Island => ScenePreset::default(),
        }
    }
}

pub(super) struct ShowcaseConfig {
    pub(super) debug: AquaDebug,
    pub(super) settings: AquaSettings,
    pub(super) waves: OceanWaves,
    pub(super) demo: Demo,
    pub(super) screenshot: Option<PathBuf>,
    pub(super) headless: bool,
}

impl ShowcaseArgs {
    pub(super) fn profile_matrix(&self) -> bool {
        self.capture.profile.profile_matrix
    }

    pub(super) fn profile_matrix_resolution(&self) -> Option<UVec2> {
        self.capture.profile.resolution
    }

    pub(super) fn into_config(self) -> anyhow::Result<ShowcaseConfig> {
        let debug = AquaDebug::from(self.presentation.debug);
        let mut demo = Demo {
            scene: self.presentation.scene,
            lighting: self.presentation.lighting,
            ..default()
        };
        self.runtime.apply(&mut demo);
        self.open.apply(&mut demo);
        let (screenshot, headless) = self.capture.apply(&mut demo)?;
        self.open.validate(demo.scene)?;
        let (settings, waves) = self.water.build(
            demo.scene,
            debug,
            demo.lighting,
            self.presentation.water_optics,
        );
        if demo.scene.preset().water_optics.is_some() {
            demo.body_optics = Some(settings.water_optics);
        }
        #[cfg(feature = "spray")]
        {
            demo.spray = self.spray.into();
        }
        Ok(ShowcaseConfig {
            debug,
            settings,
            waves,
            demo,
            screenshot,
            headless,
        })
    }
}

impl RuntimeArgs {
    fn apply(&self, demo: &mut Demo) {
        demo.water_enabled = !self.no_water;
        demo.near_shore = self.near_shore;
        demo.close_up = self.close_up;
        demo.checker = self.checker;
        demo.ui = !self.ui_off;
        demo.bloom = !self.bloom_off;
    }
}

impl OpenOceanArgs {
    fn validate(&self, scene: Scene) -> anyhow::Result<()> {
        let view_option = self.view.height != 2.0
            || self.view.boundary
            || self.view.detail_close
            || self.view.camera_offset.is_some()
            || self.view.light_scale != 1.0
            || self.view.exposure_offset != 0.0;
        let buoy_option = self.buoy.buoy
            || self.buoy.buoy_point
            || self.buoy.buoy_spot
            || self.buoy.buoy_lamp_off
            || self.buoy.buoy_underwater_light;
        let probe_option = self.probe.cubemap_probe.is_some() || self.probe.sky_only.is_some();
        anyhow::ensure!(
            scene == Scene::AnimWaves || !(view_option || buoy_option || probe_option),
            "open-ocean options require --scene anim-waves or an open-ocean --profile-pose"
        );
        Ok(())
    }

    fn apply(&self, demo: &mut Demo) {
        demo.open.height = self.view.height;
        demo.open.boundary = self.view.boundary;
        demo.open.detail_close = self.view.detail_close;
        demo.open.camera_offset = vector2(self.view.camera_offset.as_deref()).unwrap_or(Vec2::ZERO);
        demo.open.light_scale = self.view.light_scale;
        demo.open.exposure_offset = self.view.exposure_offset;
        demo.open.buoy = self.buoy.buoy
            || self.buoy.buoy_point
            || self.buoy.buoy_spot
            || self.buoy.buoy_lamp_off
            || self.buoy.buoy_underwater_light;
        demo.open.buoy_spot = !self.buoy.buoy_point;
        demo.open.buoy_lamp = !self.buoy.buoy_lamp_off;
        demo.open.buoy_underwater_light = self.buoy.buoy_underwater_light;
        demo.open.cubemap_probe = self.probe.cubemap_probe;
        demo.open.sky_only = self.probe.sky_only;
    }
}

impl CaptureArgs {
    fn apply(self, demo: &mut Demo) -> anyhow::Result<(Option<PathBuf>, bool)> {
        let ProfileArgs {
            gpu_profile,
            gpu_profile_active,
            profile_matrix: _,
            profile_pose,
            resolution,
        } = self.profile;
        demo.gpu_profile = gpu_profile || gpu_profile_active;
        demo.active_profile = gpu_profile_active;
        demo.profile_pose = profile_pose;
        demo.profile_resolution = resolution;
        if let Some(pose) = profile_pose {
            demo.scene = pose.scene();
        }
        if profile_pose == Some(ProfilePose::BuoyNight) {
            demo.lighting = Lighting::Night;
            demo.open.buoy = true;
        }
        self.sequence
            .apply(demo, self.screenshot.as_ref(), self.headless)?;
        if let Some(time) = self.time {
            demo.capture_time = time;
            demo.fixed_time = true;
        } else if self.screenshot.is_some() {
            demo.capture_time = CAPTURE_TIME;
            demo.fixed_time = true;
        }
        demo.frozen_camera = self.screenshot.is_some();
        Ok((self.screenshot, self.headless))
    }
}

impl SequenceArgs {
    fn apply(
        self,
        demo: &mut Demo,
        screenshot: Option<&PathBuf>,
        headless: bool,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.far_dolly_sequence.is_none() || self.far_dolly_step.is_none(),
            "--far-dolly-sequence conflicts with --far-dolly-step"
        );
        if let Some(step) = self.far_dolly_step {
            anyhow::ensure!(step <= 176, "--far-dolly-step expects 1 through 176");
            anyhow::ensure!(
                screenshot.is_some(),
                "--far-dolly-step selects a camera pose; pass --screenshot to capture it"
            );
            demo.far_dolly_step = Some(step);
        }
        if let Some(directory) = self.far_dolly_sequence {
            anyhow::ensure!(
                screenshot.is_none(),
                "--far-dolly-sequence supplies its own screenshot paths"
            );
            demo.far_dolly_directory = Some(directory);
            demo.far_dolly_step = Some(0);
        }
        if let Some(directory) = self.capture_sequence {
            anyhow::ensure!(
                headless,
                "--capture-sequence runs headless; pass --headless"
            );
            anyhow::ensure!(
                screenshot.is_none(),
                "--capture-sequence supplies its own screenshot paths"
            );
            demo.flow_sequence_directory = Some(directory);
        }
        Ok(())
    }
}

impl WaterArgs {
    fn build(
        self,
        scene: Scene,
        debug: AquaDebug,
        lighting: Lighting,
        optics: Option<WaterOpticsChoice>,
    ) -> (AquaSettings, OceanWaves) {
        let preset = scene.preset();
        let mut settings = AquaSettings::default();
        let mut waves = OceanWaves::default();
        waves.model = self.waves.wave_backend.into();
        waves.sea_state = self
            .waves
            .sea_state
            .map(Into::into)
            .or(preset.waves.sea_state)
            .unwrap_or(waves.sea_state);
        waves.flow = vector2(self.waves.flow.as_deref())
            .or(preset.waves.flow)
            .unwrap_or(waves.flow);
        waves.wind_direction_degrees = self
            .waves
            .wind_degrees
            .or(preset.waves.wind_degrees)
            .unwrap_or(waves.wind_direction_degrees);
        waves.wind_speed = self
            .waves
            .wind_speed
            .or(preset.waves.wind_speed)
            .unwrap_or(waves.wind_speed);
        if let Some(fetch) = self.waves.fetch {
            waves.fetch = fetch;
        }
        if self.waves.no_attenuation || !debug_allows_attenuation(debug) {
            waves.shallow_water_attenuation = 0.0;
        }
        settings.water_optics = optics
            .map(Into::into)
            .or(preset.water_optics)
            .unwrap_or(settings.water_optics);
        if let Some(reflections) = self.reflections {
            settings.reflections = reflections.into();
        }
        if self.caustics_off {
            settings.caustics = None;
        }
        if let Some(detail_strength) = preset.detail_strength {
            settings.detail_strength = detail_strength;
        }
        settings.atmospheric_sunlight =
            preset.atmospheric_sunlight_at_sunset && lighting == Lighting::Sunset;
        (settings, waves)
    }
}

fn debug_allows_attenuation(debug: AquaDebug) -> bool {
    matches!(
        debug,
        AquaDebug::SeaFloorDepth
            | AquaDebug::ShallowComposite
            | AquaDebug::FoamDensity
            | AquaDebug::FoamDensityBilinear
    )
}

fn vector2(values: Option<&[f32]>) -> Option<Vec2> {
    values.map(|values| Vec2::new(values[0], values[1]))
}

fn finite_f32(value: &str) -> Result<f32, String> {
    let value = value.parse::<f32>().map_err(|error| error.to_string())?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("value must be finite".into())
    }
}

fn nonnegative_f32(value: &str) -> Result<f32, String> {
    let value = finite_f32(value)?;
    if value >= 0.0 {
        Ok(value)
    } else {
        Err("value must be nonnegative".into())
    }
}

fn positive_f32(value: &str) -> Result<f32, String> {
    let value = finite_f32(value)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err("value must be positive".into())
    }
}

fn positive_u32(value: &str) -> Result<u32, String> {
    let value = value.parse::<u32>().map_err(|error| error.to_string())?;
    if value > 0 {
        Ok(value)
    } else {
        Err("value must be positive".into())
    }
}

fn resolution(value: &str) -> Result<UVec2, String> {
    bevy_bench::parse_resolution(std::ffi::OsStr::new(value)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(arguments: &[&str]) -> ShowcaseConfig {
        ShowcaseArgs::try_parse_from(std::iter::once("showcase").chain(arguments.iter().copied()))
            .expect("valid showcase arguments")
            .into_config()
            .expect("valid showcase configuration")
    }

    #[test]
    fn scene_presets_are_declarative_and_explicit_values_win() {
        let lake = config(&["--scene", "lake"]);
        assert_eq!(lake.waves.sea_state, SeaState::Calm);
        assert_eq!(lake.waves.flow, Vec2::new(1.2, 0.0));
        assert_eq!(lake.waves.wind_direction_degrees, 25.0);

        let reflection = config(&["--scene", "reflection-lake"]);
        assert_eq!(reflection.waves.sea_state, SeaState::Calm);
        assert_eq!(reflection.waves.flow, Vec2::ZERO);
        assert_eq!(reflection.waves.wind_direction_degrees, 15.0);
        assert_eq!(reflection.waves.wind_speed, 4.0);
        assert_eq!(reflection.settings.detail_strength, 0.02);

        let explicit = config(&[
            "--scene",
            "reflection-lake",
            "--sea-state",
            "rough",
            "--flow",
            "2",
            "-3",
            "--wind-degrees",
            "70",
            "--wind-speed",
            "9",
        ]);
        assert_eq!(explicit.waves.sea_state, SeaState::Rough);
        assert_eq!(explicit.waves.flow, Vec2::new(2.0, -3.0));
        assert_eq!(explicit.waves.wind_direction_degrees, 70.0);
        assert_eq!(explicit.waves.wind_speed, 9.0);
        assert_eq!(explicit.settings.detail_strength, 0.02);

        for scene in ["ponds", "ponds-many", "river"] {
            let default = config(&["--scene", scene]);
            assert_eq!(default.settings.water_optics, WaterOptics::CLEAR_FRESH);
            assert_eq!(default.demo.body_optics, Some(WaterOptics::CLEAR_FRESH));
            let tropical = config(&["--scene", scene, "--water-optics", "tropical"]);
            assert_eq!(tropical.settings.water_optics, WaterOptics::TROPICAL);
            assert_eq!(tropical.demo.body_optics, Some(WaterOptics::TROPICAL));
        }

        let repeated = config(&[
            "--scene",
            "lake",
            "--sea-state",
            "rough",
            "--scene",
            "river",
        ]);
        assert_eq!(repeated.demo.scene, Scene::River);
        assert_eq!(repeated.waves.sea_state, SeaState::Rough);
        assert_eq!(repeated.waves.wind_direction_degrees, 20.0);

        let repeated_vectors = config(&[
            "--scene",
            "anim-waves",
            "--flow",
            "1",
            "2",
            "--flow",
            "3",
            "4",
            "--camera-offset",
            "5",
            "6",
            "--camera-offset",
            "7",
            "8",
        ]);
        assert_eq!(repeated_vectors.waves.flow, Vec2::new(3.0, 4.0));
        assert_eq!(
            repeated_vectors.demo.open.camera_offset,
            Vec2::new(7.0, 8.0)
        );
    }

    #[test]
    fn final_scene_lighting_and_capture_rules_are_preserved() {
        let sunset = config(&["--scene", "anim-waves", "--lighting", "sunset"]);
        assert!(sunset.settings.atmospheric_sunlight);
        let island = config(&["--lighting", "sunset"]);
        assert!(!island.settings.atmospheric_sunlight);

        let screenshot = config(&["--screenshot", "capture.png"]);
        assert!(screenshot.demo.fixed_time);
        assert!(screenshot.demo.frozen_camera);
        assert_eq!(screenshot.demo.capture_time, CAPTURE_TIME);
        let timed = config(&["--time", "3.5", "--screenshot", "capture.png"]);
        assert_eq!(timed.demo.capture_time, 3.5);

        let buoy_pose = config(&["--scene", "island", "--profile-pose", "buoy-night"]);
        assert_eq!(buoy_pose.demo.scene, Scene::AnimWaves);
        assert_eq!(buoy_pose.demo.lighting, Lighting::Night);
        assert!(buoy_pose.demo.open.buoy);
        let island_pose = config(&["--scene", "river", "--profile-pose", "island"]);
        assert_eq!(island_pose.demo.scene, Scene::Island);
        assert_eq!(
            island_pose.demo.profile_pose,
            Some(ProfilePose::IslandOverview)
        );
        for (pose, scene) in [
            ("open-2m", Scene::AnimWaves),
            ("open-50m", Scene::AnimWaves),
            ("open-500m", Scene::AnimWaves),
            ("lake-shore", Scene::Lake),
            ("ponds", Scene::Ponds),
            ("ponds-many", Scene::PondsMany),
            ("river", Scene::River),
            ("river-chase", Scene::River),
        ] {
            assert_eq!(config(&["--profile-pose", pose]).demo.scene, scene);
        }

        let matrix = ShowcaseArgs::try_parse_from([
            "showcase",
            "--profile-matrix",
            "--resolution=2560x1440",
        ])
        .expect("matrix arguments");
        assert_eq!(
            matrix.profile_matrix_resolution(),
            Some(UVec2::new(2560, 1440))
        );

        for arguments in [
            ["showcase", "--wind-speed", "0"],
            ["showcase", "--wind-speed", "-1"],
            ["showcase", "--fetch", "0"],
            ["showcase", "--fetch", "-1"],
        ] {
            assert!(ShowcaseArgs::try_parse_from(arguments).is_err());
        }
        assert!(
            ShowcaseArgs::try_parse_from([
                "showcase",
                "--cubemap-probe",
                "horizon",
                "--sky-only",
                "sweep",
            ])
            .is_err()
        );

        let invalid_open = ShowcaseArgs::try_parse_from(["showcase", "--height", "5"])
            .expect("syntactically valid arguments")
            .into_config();
        assert!(invalid_open.is_err());
        config(&["--scene", "anim-waves", "--height", "5"]);
        config(&["--bloom-off"]);

        let missing_screenshot =
            ShowcaseArgs::try_parse_from(["showcase", "--far-dolly-step", "8"])
                .expect("syntactically valid arguments")
                .into_config();
        assert!(missing_screenshot.is_err());
        config(&["--far-dolly-step", "8", "--screenshot", "capture.png"]);
    }
}

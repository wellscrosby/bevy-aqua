//! Headless GPU gate for Aqua's motion-vector sign and recenter invariants.
//!
//! Run with:
//! `cargo run -p aqua-motion --example motion_vector_correctness_gate`
//!
//! The shader writes a 64x64 `Rg16Float` render attachment. The assertions use
//! raw readback bytes from a central mask. No PNG or color conversion is part
//! of this gate.

use std::time::Duration;

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::{RenderAssetUsages, embedded_asset},
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    reflect::TypePath,
    render::{
        gpu_readback::{Readback, ReadbackComplete},
        render_resource::{AsBindGroup, TextureFormat, TextureUsages},
    },
    shader::ShaderRef,
    sprite_render::{Material2d, Material2dPlugin},
    window::ExitCondition,
};

const SIZE: u32 = 64;
const MASK_MIN: u32 = 16;
const MASK_MAX: u32 = 48;
const SHADER_PATH: &str =
    "embedded://motion_vector_correctness_gate/motion_vector_correctness_gate.wgsl";

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins((
            ScheduleRunnerPlugin::run_loop(Duration::from_millis(16)),
            GateShaderPlugin,
            Material2dPlugin::<GateMaterial>::default(),
        ))
        .init_resource::<ReadbackRequest>()
        .init_resource::<GateProgress>()
        .add_systems(Startup, setup)
        .add_systems(Update, request_readbacks)
        .run();
}

struct GateShaderPlugin;

impl Plugin for GateShaderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "examples", "motion_vector_correctness_gate.wgsl");
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct GateMaterial {
    /// Current/previous camera X followed by current/previous displacement X.
    #[uniform(0)]
    frame: Vec4,
    /// Current/previous ring origin X, stable anchor X, and view width.
    #[uniform(1)]
    anchor: Vec4,
}

impl Material2d for GateMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

#[derive(Clone, Copy)]
struct GateCase {
    name: &'static str,
    frame: Vec4,
    anchor: Vec4,
    expected_x: f32,
    expected_bits: u16,
}

const CASES: [GateCase; 3] = [
    GateCase {
        name: "camera-only",
        // A +1 m camera translation over a 16 m orthographic span is -1/8 NDC.
        frame: Vec4::new(1.0, 0.0, 0.0, 0.0),
        anchor: Vec4::new(0.0, 0.0, 0.0, 16.0),
        expected_x: -0.0625,
        expected_bits: 0xac00,
    },
    GateCase {
        name: "uniform-displacement-only",
        frame: Vec4::new(0.0, 0.0, 1.0, 0.0),
        anchor: Vec4::new(0.0, 0.0, 0.0, 16.0),
        expected_x: 0.0625,
        expected_bits: 0x2c00,
    },
    GateCase {
        name: "forced-ring-entity-recenter-with-still-camera-and-water",
        // The current ring moved +1 m while the physical Eulerian anchor did
        // not. Using the ring transform would incorrectly produce +0.0625.
        frame: Vec4::ZERO,
        anchor: Vec4::new(1.0, 0.0, 0.0, 16.0),
        expected_x: 0.0,
        expected_bits: 0x0000,
    },
];

#[derive(Resource)]
struct GateTargets(Vec<(Handle<Image>, GateCase)>);

#[derive(Resource, Default)]
struct ReadbackRequest {
    frames: u32,
    next: usize,
    in_flight: bool,
    readback_entity: Option<Entity>,
}

#[derive(Resource, Default)]
struct GateProgress {
    passed: usize,
}

#[derive(Component)]
struct GateReadback(GateCase);

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GateMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(128.0, 128.0));
    let mut targets = Vec::with_capacity(CASES.len());

    for (layer, case) in CASES.into_iter().enumerate() {
        let mut image = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rg16Float, None);
        image.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
        image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
        let target = images.add(image);

        commands.spawn((
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Image(target.clone().into()),
            Msaa::Off,
            RenderLayers::layer(layer),
        ));
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(GateMaterial {
                frame: case.frame,
                anchor: case.anchor,
            })),
            RenderLayers::layer(layer),
        ));
        targets.push((target, case));
    }

    commands.insert_resource(GateTargets(targets));
}

fn request_readbacks(
    mut commands: Commands,
    targets: Res<GateTargets>,
    mut request: ResMut<ReadbackRequest>,
) {
    request.frames += 1;
    if let Some(entity) = request.readback_entity.take() {
        // Readback otherwise extracts again every frame until map completion.
        // One extracted copy is sufficient for this one-shot gate.
        commands.entity(entity).remove::<Readback>();
    }
    if request.in_flight || request.frames < 8 || request.next == targets.0.len() {
        return;
    }
    let (target, case) = &targets.0[request.next];
    request.in_flight = true;
    let entity = commands
        .spawn((Readback::texture(target.clone()), GateReadback(*case)))
        .observe(validate_readback)
        .id();
    request.readback_entity = Some(entity);
}

fn validate_readback(
    event: On<ReadbackComplete>,
    labels: Query<&GateReadback>,
    mut progress: ResMut<GateProgress>,
    mut request: ResMut<ReadbackRequest>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let case = labels
        .get(event.entity)
        .expect("readback entity must retain its gate label")
        .0;
    validate_raw_rg16float(&event.data, case);
    commands.entity(event.entity).despawn();

    progress.passed += 1;
    request.next += 1;
    request.in_flight = false;
    info!(case = case.name, "motion-vector case passed");
    if progress.passed == CASES.len() {
        println!("motion-vector correctness gate passed: all 3 raw Rg16Float cases");
        exit.write(AppExit::Success);
    }
}

fn validate_raw_rg16float(data: &[u8], case: GateCase) {
    const BYTES_PER_PIXEL: usize = 4;
    let expected_len = (SIZE * SIZE) as usize * BYTES_PER_PIXEL;
    assert_eq!(
        data.len(),
        expected_len,
        "{}: unexpected raw Rg16Float byte count",
        case.name
    );

    for y in MASK_MIN..MASK_MAX {
        for x in MASK_MIN..MASK_MAX {
            let offset = ((y * SIZE + x) as usize) * BYTES_PER_PIXEL;
            let raw_x = u16::from_le_bytes([data[offset], data[offset + 1]]);
            let raw_y = u16::from_le_bytes([data[offset + 2], data[offset + 3]]);
            let actual_x = f16_to_f32(raw_x);
            let actual_y = f16_to_f32(raw_y);
            assert_eq!(
                raw_x, case.expected_bits,
                "{}: x bits mismatch at ({x}, {y})",
                case.name
            );
            assert_eq!(
                actual_x, case.expected_x,
                "{}: x mismatch at ({x}, {y}), raw=0x{raw_x:04x}",
                case.name
            );
            assert_eq!(
                raw_y, 0x0000,
                "{}: y bits mismatch at ({x}, {y})",
                case.name
            );
            assert_eq!(actual_y, 0.0, "{}: y mismatch at ({x}, {y})", case.name);
        }
    }
}

/// Exact binary16-to-binary32 conversion for raw texture validation.
fn f16_to_f32(bits: u16) -> f32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exponent = ((bits >> 10) & 0x1f) as u32;
    let fraction = (bits & 0x03ff) as u32;
    let out = match exponent {
        0 => {
            if fraction == 0 {
                sign
            } else {
                let shift = fraction.leading_zeros() - 21;
                let normalized = (fraction << shift) & 0x03ff;
                sign | ((113 - shift) << 23) | (normalized << 13)
            }
        }
        0x1f => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | ((exponent + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(out)
}

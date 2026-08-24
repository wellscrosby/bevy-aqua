//! Raw GPU gate for Aqua's production motion-vector prepass.
//!
//! Run this with a windowed renderer:
//! `cargo run --features motion --example motion_prepass_production_gate`.
//! The `cage` headless surface currently rasterizes no water for this example
//! (`water_pixels=0`), so its camera/wave failures are not Aqua motion defects.

use std::{
    borrow::Cow,
    num::NonZeroU8,
    sync::{Mutex, mpsc},
    time::Duration,
};

use bevy::{
    app::ScheduleRunnerPlugin,
    camera::RenderTarget,
    core_pipeline::{
        prepass::{DepthPrepass, NoBackgroundMotionVectors, ViewPrepassTextures},
        schedule::Core3d,
        tonemapping::Tonemapping,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{
            BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingType, Buffer,
            BufferBindingType, BufferDescriptor, BufferUsages, ComputePassDescriptor,
            ComputePipeline, MapMode, PipelineCompilationOptions, PipelineLayoutDescriptor,
            PollType, RawComputePipelineDescriptor, ShaderModuleDescriptor, ShaderSource,
            ShaderStages, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo,
            TextureAspect, TextureFormat, TextureSampleType, TextureUsages, TextureViewDimension,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewDepthTexture,
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy_aqua::{AquaMotionSystems, AquaPlugin, AquaSettings, Ocean, OceanWaves, ReflectionMode};
use bevy_aqua_core::CascadeMaterial;

const SIZE: u32 = 64;
const ROW_BYTES: u32 = SIZE * 4;
const EXPECTED_X: u16 = 0xac00; // IEEE binary16 -0.0625

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
enum Case {
    #[default]
    None,
    Reset,
    Stationary,
    Recenter,
    Camera,
    Waves,
    DepthOn,
    DepthOff,
}

#[derive(Resource, Clone, Copy, Default, ExtractResource)]
struct CaptureCase(Case);

#[derive(Resource)]
struct Gate {
    tick: u32,
    camera: Entity,
}

#[derive(Clone)]
struct Sample {
    case: Case,
    motion: Vec<u8>,
    depth: Vec<u8>,
}

#[derive(Resource)]
struct SamplesRx(Mutex<mpsc::Receiver<Sample>>);
#[derive(Resource)]
struct SamplesTx(mpsc::Sender<Sample>);

#[derive(Resource)]
struct Readback {
    motion: Buffer,
    depth: Buffer,
    gpu_motion: Buffer,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

fn main() {
    run_app();
}

fn run_app() {
    let (tx, rx) = mpsc::channel();
    let settings = AquaSettings {
        reflections: ReflectionMode::Cubemap,
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(settings)
        .insert_resource(OceanWaves {
            shallow_water_attenuation: 0.0,
            ..default()
        })
        .insert_resource(CaptureCase::default())
        .insert_resource(SamplesRx(Mutex::new(rx)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(1)))
        .add_plugins(ExtractResourcePlugin::<CaptureCase>::default())
        .add_plugins(AquaPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            PostUpdate,
            drive_gate.before(bevy_aqua_core::CascadeMaterialsUpdated),
        )
        .add_systems(
            PostUpdate,
            force_ring_recenter
                .after(bevy_aqua_core::CascadeMaterialsUpdated)
                .before(TransformSystems::Propagate),
        )
        .add_systems(Last, collect_and_finish);
    let render = app.sub_app_mut(RenderApp);
    render
        .insert_resource(SamplesTx(tx))
        .add_systems(ExtractSchedule, prepare_readback)
        .add_systems(Core3d, copy_prepass.after(AquaMotionSystems::Draw))
        .add_systems(Render, map_readback.after(RenderSystems::Render));
    let result = app.run();
    if let AppExit::Error(code) = result {
        std::process::exit(i32::from(code.get()));
    }
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut time: ResMut<Time<Virtual>>,
) {
    time.pause();
    let mut target = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, None);
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = images.add(target);
    commands.insert_resource(Ocean::default());
    let camera = commands
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            Projection::Orthographic(OrthographicProjection {
                scale: 1.0,
                near: 0.1,
                far: 100.0,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 20.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
            DepthPrepass,
            // Isolate Aqua's raw writes; production keeps Bevy background vectors enabled.
            NoBackgroundMotionVectors,
            Msaa::Off,
            Tonemapping::None,
        ))
        .id();
    commands.insert_resource(Gate { tick: 0, camera });
}

fn drive_gate(
    mut gate: ResMut<Gate>,
    mut case: ResMut<CaptureCase>,
    mut cameras: Query<&mut Transform>,
    mut time: ResMut<Time<Virtual>>,
    mut water_visibility: Query<&mut Visibility, With<MeshMaterial3d<CascadeMaterial>>>,
) {
    case.0 = Case::None;
    match gate.tick {
        // Pipeline and retained displacement history warm while virtual time is frozen.
        250 => {
            cameras.get_mut(gate.camera).unwrap().translation.x = 0.0;
            // A discontinuous clock jump invalidates all Aqua motion for one frame.
            time.advance_by(Duration::from_secs(1));
            case.0 = Case::Reset;
        }
        252 => case.0 = Case::Stationary,
        254 => case.0 = Case::Recenter,
        256 => {
            cameras.get_mut(gate.camera).unwrap().translation.x += 4.0;
            case.0 = Case::Camera;
        }
        258 => time.advance_by(Duration::from_secs_f32(1.0 / 60.0)),
        259 => {
            time.advance_by(Duration::from_secs_f32(1.0 / 60.0));
            case.0 = Case::Waves;
        }
        261 => case.0 = Case::DepthOn,
        263 => {
            for mut visibility in &mut water_visibility {
                *visibility = Visibility::Hidden;
            }
            case.0 = Case::DepthOff;
        }
        265 => {
            for mut visibility in &mut water_visibility {
                *visibility = Visibility::Inherited;
            }
        }
        _ => {}
    }
    gate.tick += 1;
}

fn force_ring_recenter(
    gate: Res<Gate>,
    mut water: Query<&mut Transform, With<MeshMaterial3d<CascadeMaterial>>>,
) {
    // `drive_gate` has already advanced 254 -> 255 in this same frame.
    if gate.tick == 255 {
        for mut transform in &mut water {
            transform.translation.x += 4.0;
        }
    }
}

fn prepare_readback(
    mut commands: Commands,
    device: Res<RenderDevice>,
    buffers: Option<Res<Readback>>,
) {
    if buffers.is_some() {
        return;
    }
    let make = |label, usage| {
        device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: u64::from(ROW_BYTES * SIZE),
            usage,
            mapped_at_creation: false,
        })
    };
    let layout = device.create_bind_group_layout(
        "aqua_gate_readback_layout",
        &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    );
    // SAFETY: this fixed WGSL is validated by wgpu and uses no unchecked shader input.
    let shader = unsafe {
        device.create_shader_module(ShaderModuleDescriptor {
            label: Some("aqua_gate_readback"),
            source: ShaderSource::Wgsl(Cow::Borrowed(
                r#"
@group(0) @binding(0) var motion: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> motion_out: array<u32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= 64u || id.y >= 64u) { return; }
    let i = id.y * 64u + id.x;
    motion_out[i] = pack2x16float(textureLoad(motion, vec2<i32>(id.xy), 0).xy);
}"#,
            )),
        })
    };
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("aqua_gate_readback_pipeline_layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
        label: Some("aqua_gate_readback_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });
    commands.insert_resource(Readback {
        motion: make(
            "aqua_gate_motion",
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        ),
        depth: make(
            "aqua_gate_depth",
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        ),
        gpu_motion: make(
            "aqua_gate_gpu_motion",
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        ),
        layout,
        pipeline,
    });
}

fn copy_prepass(
    mut context: RenderContext,
    case: Res<CaptureCase>,
    buffers: Res<Readback>,
    device: Res<RenderDevice>,
    view: ViewQuery<(&ViewPrepassTextures, &ViewDepthTexture)>,
) {
    if case.0 == Case::None {
        return;
    }
    let (prepass, depth) = view.into_inner();
    let motion = prepass
        .motion_vectors
        .as_ref()
        .expect("AquaMotionPlugin did not create motion vectors");
    let group = device.create_bind_group(
        "aqua_gate_readback_group",
        &buffers.layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: bevy::render::render_resource::BindingResource::TextureView(
                    &motion.texture.default_view,
                ),
            },
            BindGroupEntry {
                binding: 1,
                resource: buffers.gpu_motion.as_entire_binding(),
            },
        ],
    );
    {
        let mut pass = context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("aqua_gate_readback"),
                timestamp_writes: None,
            });
        pass.set_pipeline(&buffers.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(8, 8, 1);
    }
    let size = u64::from(ROW_BYTES * SIZE);
    context.command_encoder().copy_buffer_to_buffer(
        &buffers.gpu_motion,
        0,
        &buffers.motion,
        0,
        size,
    );
    context.command_encoder().copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &depth.texture,
            mip_level: 0,
            origin: bevy::render::render_resource::Origin3d::ZERO,
            aspect: TextureAspect::DepthOnly,
        },
        TexelCopyBufferInfo {
            buffer: &buffers.depth,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_BYTES),
                rows_per_image: None,
            },
        },
        bevy::render::render_resource::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
}
fn map_readback(
    case: Res<CaptureCase>,
    buffers: Res<Readback>,
    device: Res<RenderDevice>,
    tx: Res<SamplesTx>,
    mut pending: Local<Case>,
) {
    let completed = *pending;
    *pending = case.0;
    if completed == Case::None {
        return;
    }
    fn read(buffer: &Buffer, device: &RenderDevice) -> Vec<u8> {
        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::sync_channel(1);
        slice.map_async(MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = slice.get_mapped_range().to_vec();
        buffer.unmap();
        bytes
    }
    tx.0.send(Sample {
        case: completed,
        motion: read(&buffers.motion, &device),
        depth: read(&buffers.depth, &device),
    })
    .unwrap();
}

fn bits(bytes: &[u8], pixel: usize, component: usize) -> u16 {
    let offset = pixel * 4 + component * 2;
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn pass(ok: bool) -> &'static str {
    if ok { "PASS" } else { "FAIL" }
}

fn collect_and_finish(
    gate: Res<Gate>,
    rx: Res<SamplesRx>,
    mut samples: Local<Vec<Sample>>,
    mut exit: MessageWriter<AppExit>,
) {
    samples.extend(rx.0.lock().unwrap().try_iter());
    if gate.tick < 275 {
        return;
    }
    let get = |case| samples.iter().find(|sample| sample.case == case).unwrap();
    let reset = get(Case::Reset);
    let stationary = get(Case::Stationary);
    let recenter = get(Case::Recenter);
    let camera = get(Case::Camera);
    let wave = get(Case::Waves);
    let pixels = 0..(SIZE * SIZE) as usize;
    let exact_zero = |sample: &Sample| sample.motion.iter().all(|&byte| byte == 0);
    // With background vectors disabled in this gate, nonzero camera-motion
    // texels are exactly the production Aqua coverage mask.
    let water = pixels
        .clone()
        .filter(|&i| bits(&camera.motion, i, 0) & 0x7fff != 0)
        .collect::<Vec<_>>();
    let exact_camera = water
        .iter()
        .filter(|&&i| bits(&camera.motion, i, 0) == EXPECTED_X)
        .count();
    let reset_ok = exact_zero(reset);
    let stationary_ok = exact_zero(stationary);
    let recenter_ok = exact_zero(recenter);
    let camera_ok = !water.is_empty()
        && exact_camera * 100 >= water.len() * 95
        && water.iter().all(|&i| {
            matches!(bits(&camera.motion, i, 0), EXPECTED_X | 0xabff)
                && bits(&camera.motion, i, 1) == 0
        });
    let waves_ok = (0..(SIZE * SIZE) as usize).any(|i| {
        let x = bits(&wave.motion, i, 0);
        let y = bits(&wave.motion, i, 1);
        (x & 0x7c00) != 0x7c00 && (y & 0x7c00) != 0x7c00 && (x & 0x7fff != 0 || y & 0x7fff != 0)
    });
    let depth_ok = get(Case::DepthOn).depth == get(Case::DepthOff).depth;
    let i = water.first().copied().unwrap_or((SIZE * SIZE / 2) as usize);
    println!(
        "AQUA_GATE reset={} bits=({:#06x},{:#06x}) stationary={} recenter={} camera={} bits=({:#06x},{:#06x})",
        pass(reset_ok),
        bits(&reset.motion, i, 0),
        bits(&reset.motion, i, 1),
        pass(stationary_ok),
        pass(recenter_ok),
        pass(camera_ok),
        bits(&camera.motion, i, 0),
        bits(&camera.motion, i, 1)
    );
    println!(
        "AQUA_GATE waves={} bits=({:#06x},{:#06x}) depth={} bytes={} water_pixels={}",
        pass(waves_ok),
        bits(&wave.motion, i, 0),
        bits(&wave.motion, i, 1),
        pass(depth_ok),
        wave.depth.len(),
        water.len()
    );
    if reset_ok && stationary_ok && recenter_ok && camera_ok && waves_ok && depth_ok {
        exit.write(AppExit::Success);
    } else {
        exit.write(AppExit::Error(NonZeroU8::new(1).unwrap()));
    }
}

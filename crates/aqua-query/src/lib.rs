//! GPU-sampled wave queries for gameplay such as buoyancy.

#![warn(unreachable_pub)]
//!
//! Insert [`WaveQuery`] on any entity with a transform to receive the rendered
//! water displacement and surface normal at its origin every frame in
//! [`WaveSurface`]. Samples follow the same AnimWaves cascades, LOD blending,
//! and detail-LOD clamps as the visible surface, so objects float on exactly
//! what players see. Results arrive with roughly one frame of latency because
//! they are computed on the GPU and read back asynchronously.
//!
//! Every probe — ocean, bounded pond, or river body — goes through the one
//! compute dispatch and readback: extraction resolves each probe against
//! the registered bodies (`aqua-sdf`) into a request carrying its surface
//! level and, inside rivers, the baked current plus channel geometry. The
//! shader synthesizes river waves in closed form (the shared
//! `aqua_core::river` module) instead of sampling the cascades there,
//! mirroring the material's vertex path.
//!
//! Sample points beyond the coarsest cascade ring return zero displacement,
//! matching the horizon fade of the rendered waves. At most `MAX_QUERIES`
//! sample points are submitted per frame; over-budget entities keep their
//! previous sample.

use std::collections::HashMap;

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    core_pipeline::{Core3dSystems, schedule::Core3d},
    prelude::*,
    render::{
        MainWorld, Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{Readback, ReadbackComplete},
        render_asset::RenderAssets,
        render_resource::{
            ShaderType,
            binding_types::{
                sampler, storage_buffer, storage_buffer_read_only, texture_2d_array, uniform_buffer,
            },
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        storage::{GpuShaderBuffer, ShaderBuffer},
        texture::GpuImage,
    },
};

use aqua_core::{
    AnimWavesUniformSlot, Data, Ocean, OceanView, ResolvedWaterBodies, ResolvedWaterBody, pass,
};
use aqua_sdf::FlowSample;

const SHADER_PATH: &str = "embedded://aqua_query/wave_query.wgsl";
pub(crate) const MAX_QUERIES: u32 = 256;
const WORKGROUP_SIZE: u32 = 64;
const RESULT_FLOATS: usize = 12;
const VALIDITY_INDEX: usize = 7;
const SLOT_INDEX: usize = 3;

/// Marks an entity for per-frame water sampling at its world origin.
///
/// The entity needs a transform; child entities inherit their parent and make
/// multi-point hulls straightforward. Inserting this component automatically
/// inserts [`WaveSurface`].
///
/// # Examples
///
/// ```
/// use bevy::prelude::*;
/// use aqua_core::Ocean;
/// use aqua_query::{AquaQueryPlugin, WaveQuery};
///
/// # fn spawn_buoy(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
///     commands.spawn((
///         WaveQuery,
///         Mesh3d(meshes.add(Sphere::new(0.5))),
///         Transform::from_xyz(4.0, 0.0, -2.0),
///     ));
/// }
/// ```
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
#[require(WaveSurface)]
pub struct WaveQuery;

/// The sampled water surface at a [`WaveQuery`] entity's origin.
///
/// Refreshed every frame with roughly one frame of latency. Until the first
/// result arrives, or while no surface contains the query, [`WaveSurface::valid`]
/// is false.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct WaveSurface {
    /// World-space wave displacement relative to the owning surface's mean
    /// plane. Add this to a query transform placed at that plane's level.
    pub displacement: Vec3,
    /// Unit surface normal at the sample point.
    pub normal: Vec3,
    /// Whether this query currently resolves to an ocean or bounded body.
    pub valid: bool,
    /// Instantaneous breaking-crest source in `[0, 1]`, derived from the
    /// same horizontal-displacement compression that feeds persistent foam.
    pub crest: f32,
}

impl Default for WaveSurface {
    fn default() -> Self {
        Self {
            displacement: Vec3::ZERO,
            normal: Vec3::Y,
            valid: false,
            crest: 0.0,
        }
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct QueryRequest {
    world_xz: Vec2,
    slot: f32,
    // 0: ocean cascade, 1: flat bounded body, 2: analytic river.
    kind: f32,
    // River synthesis inputs: xy local current (m/s), z signed bank
    // margin (m), w channel half width (m). Positive width selects the
    // river analytic path even when authored current is zero.
    flow: Vec4,
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct QueryResult {
    displacement_slot: Vec4,
    normal_validity: Vec4,
    signals: Vec4,
}

#[derive(Resource, Debug, Default)]
struct Registry {
    next_slot: u32,
    slots: HashMap<Entity, u32>,
    entities: HashMap<u32, Entity>,
}

impl Registry {
    fn assign(&mut self, entity: Entity) -> Option<u32> {
        if self.slots.contains_key(&entity) {
            return None;
        }
        self.next_slot += 1;
        let slot = self.next_slot;
        self.slots.insert(entity, slot);
        self.entities.insert(slot, entity);
        Some(slot)
    }

    fn reclaim(&mut self, entity: Entity) {
        if let Some(slot) = self.slots.remove(&entity) {
            self.entities.remove(&slot);
        }
    }
}

#[derive(Resource, Clone, ExtractResource)]
struct Buffers {
    requests: Handle<ShaderBuffer>,
    results: Handle<ShaderBuffer>,
}

#[derive(Resource, Default)]
struct Batch {
    bytes: Vec<u8>,
    count: usize,
}

/// Registers extraction, GPU sampling, and async readback for [`WaveQuery`].
#[derive(Debug, Default, Clone, Copy)]
pub struct AquaQueryPlugin;

impl Plugin for AquaQueryPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "wave_query.wgsl");
        app.init_resource::<Registry>()
            .add_systems(Startup, init_buffers)
            .add_systems(Update, (assign_slots, reclaim_slots))
            .add_plugins(ExtractResourcePlugin::<Buffers>::default());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, init_pipeline)
                .add_systems(ExtractSchedule, extract_wave_queries)
                .add_systems(
                    Render,
                    prepare_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                )
                .add_systems(
                    Core3d,
                    dispatch_wave_query
                        .after(aqua_core::AnimWavesWritten)
                        .before(Core3dSystems::MainPass),
                );
        }
    }
}

fn init_buffers(mut commands: Commands, mut buffers: ResMut<Assets<ShaderBuffer>>) {
    let requests = buffers.add(ShaderBuffer::with_size(
        MAX_QUERIES as usize * QueryRequest::SHADER_SIZE.get() as usize,
        RenderAssetUsages::default(),
    ));
    let results = buffers.add(ShaderBuffer::with_size(
        MAX_QUERIES as usize * QueryResult::SHADER_SIZE.get() as usize,
        RenderAssetUsages::default(),
    ));
    commands.insert_resource(Buffers {
        requests: requests.clone(),
        results: results.clone(),
    });
    commands
        .spawn(Readback::buffer(results))
        .observe(apply_results);
}

fn assign_slots(spawned: Query<Entity, Added<WaveQuery>>, mut registry: ResMut<Registry>) {
    for entity in &spawned {
        registry.assign(entity);
    }
}

fn reclaim_slots(mut removed: RemovedComponents<WaveQuery>, mut registry: ResMut<Registry>) {
    for entity in removed.read() {
        registry.reclaim(entity);
    }
}

fn pack_requests(submissions: &[(u32, Vec2, f32, Vec4)]) -> (Vec<u8>, usize) {
    let count = submissions.len().min(MAX_QUERIES as usize);
    let mut requests = Vec::with_capacity(count);
    for &(slot, world_xz, kind, flow) in submissions.iter().take(count) {
        requests.push(QueryRequest {
            world_xz,
            slot: slot as f32,
            kind,
            flow,
        });
    }
    let mut bytes = Vec::with_capacity(count * QueryRequest::SHADER_SIZE.get() as usize);
    let mut wrapper = bevy::render::render_resource::encase::StorageBuffer::new(&mut bytes);
    wrapper.write(&requests).expect("packed requests write");
    (bytes, count)
}

fn decode_results(data: &[u8], entities: &HashMap<u32, Entity>) -> Vec<(Entity, Vec3, Vec3, f32)> {
    let floats: Vec<f32> = data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    let mut samples = Vec::new();
    for record in floats.as_chunks::<RESULT_FLOATS>().0 {
        if record[VALIDITY_INDEX] != 1.0 {
            continue;
        }
        let slot = record[SLOT_INDEX] as u32;
        let Some(entity) = entities.get(&slot) else {
            continue;
        };
        samples.push((
            *entity,
            Vec3::from_slice(&record[0..3]),
            Vec3::from_slice(&record[4..7]),
            record[8].clamp(0.0, 1.0),
        ));
    }
    samples
}

fn apply_results(
    event: On<ReadbackComplete>,
    registry: Res<Registry>,
    mut surfaces: Query<&mut WaveSurface>,
) {
    let merged = decode_results(&event.data, &registry.entities);
    for (entity, displacement, normal, crest) in merged {
        if let Ok(mut surface) = surfaces.get_mut(entity) {
            surface.displacement = displacement;
            surface.normal = normal.normalize_or_zero();
            surface.valid = true;
            surface.crest = crest;
        }
    }
}

#[derive(Debug)]
enum ProbeResolution {
    Ocean,
    Body,
    River { flowed: FlowSample },
}

fn probe_resolution(
    bodies: &[ResolvedWaterBody],
    ocean_level: Option<f32>,
    world_xz: Vec2,
) -> Option<ProbeResolution> {
    for body in bodies {
        if let Some(flowed) = body.flow_at(world_xz)
            && flowed.margin >= 0.0
        {
            return Some(ProbeResolution::River { flowed });
        }
        if body.contains(world_xz) {
            return Some(ProbeResolution::Body);
        }
    }
    ocean_level.map(|_| ProbeResolution::Ocean)
}

fn extract_wave_queries(mut main_world: ResMut<MainWorld>, mut commands: Commands) {
    let bodies = main_world.resource::<ResolvedWaterBodies>().0.clone();
    let ocean_level = main_world.get_resource::<Ocean>().map(|ocean| ocean.level);
    let mut submissions: Vec<(u32, Vec2, f32, Vec4)> = Vec::new();
    let mut invalid = Vec::new();
    let mut probes = main_world.query::<(Entity, &GlobalTransform, &WaveQuery)>();
    for (entity, transform, _) in probes.iter(&main_world) {
        if submissions.len() >= MAX_QUERIES as usize {
            break;
        }
        let Some(slot) = main_world.resource::<Registry>().slots.get(&entity) else {
            continue;
        };
        let position = transform.translation();
        let world_xz = position.xz();
        let Some(resolution) = probe_resolution(&bodies, ocean_level, world_xz) else {
            invalid.push(entity);
            continue;
        };
        match resolution {
            ProbeResolution::River { flowed, .. } => {
                submissions.push((
                    *slot,
                    world_xz,
                    2.0,
                    Vec4::new(
                        flowed.flow.x,
                        flowed.flow.y,
                        flowed.margin,
                        flowed.half_width,
                    ),
                ));
            }
            ProbeResolution::Body => {
                submissions.push((*slot, world_xz, 1.0, Vec4::ZERO));
            }
            ProbeResolution::Ocean => {
                submissions.push((*slot, world_xz, 0.0, Vec4::ZERO));
            }
        }
    }
    for entity in invalid {
        if let Some(mut surface) = main_world.get_mut::<WaveSurface>(entity) {
            *surface = WaveSurface::default();
        }
    }
    // Stable order keeps the GPU submission deterministic across frames.
    submissions.sort_by_key(|(slot, ..)| *slot);
    let (bytes, count) = pack_requests(&submissions);
    commands.insert_resource(Batch { bytes, count });
}

const QUERY: &str = "Wave query";
const SAMPLE: &str = "sample";

fn pass_table() -> Vec<pass::PassSpec> {
    vec![pass::PassSpec {
        key: QUERY,
        shader: pass::ShaderSource::Path(SHADER_PATH),
        entry_points: &[SAMPLE],
        layout: BindGroupLayoutDescriptor::new(
            QUERY,
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    texture_2d_array(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    uniform_buffer::<aqua_core::AnimWavesUniform>(false),
                    storage_buffer_read_only::<QueryRequest>(false),
                    storage_buffer::<QueryResult>(false),
                ),
            ),
        ),
    }]
}

#[derive(Resource)]
struct Prepared {
    passes: pass::Passes,
    sampler: Sampler,
    groups: pass::Groups,
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("aqua wave query"),
        ..default()
    });
    commands.insert_resource(Prepared {
        passes: pass::Passes::new(&asset_server, &cache, pass_table()),
        sampler,
        groups: pass::Groups::default(),
    });
}

fn prepare_bind_groups(
    resources: (Res<Data>, Res<AnimWavesUniformSlot>, Option<Res<Buffers>>),
    assets: (
        Res<RenderAssets<GpuImage>>,
        Res<RenderAssets<GpuShaderBuffer>>,
    ),
    device: (Res<RenderDevice>, Res<PipelineCache>),
    mut prepared: ResMut<Prepared>,
) {
    let (data, slot, buffers) = resources;
    let (images, ssbos) = assets;
    let anim_waves = data.texture();
    let (Some(buffers), Some(output), Some(uniform)) =
        (buffers.as_ref(), images.get(&anim_waves), slot.0.as_ref())
    else {
        return;
    };
    let (Some(requests), Some(results)) =
        (ssbos.get(&buffers.requests), ssbos.get(&buffers.results))
    else {
        return;
    };
    if prepared.groups.created() {
        return;
    }
    let group = pass::bind_group(
        &device.0,
        &device.1,
        &prepared.passes,
        QUERY,
        QUERY,
        &BindGroupEntries::sequential((
            &output.texture_view,
            &prepared.sampler,
            uniform,
            BufferBinding {
                buffer: &requests.buffer,
                offset: 0,
                size: None,
            },
            BufferBinding {
                buffer: &results.buffer,
                offset: 0,
                size: None,
            },
        )),
    );
    prepared.groups.register("query", group);
}

fn dispatch_wave_query(
    view: ViewQuery<Option<&OceanView>>,
    resources: (Res<Batch>, Res<Prepared>, Option<Res<Buffers>>),
    assets: (Res<RenderAssets<GpuShaderBuffer>>, Res<RenderQueue>),
    cache: Res<PipelineCache>,
    mut context: RenderContext,
) {
    if view.into_inner().is_none() {
        return;
    }
    let (batch, prepared, buffers) = resources;
    let (ssbos, queue) = assets;
    if batch.count == 0 {
        return;
    }
    let Some(group) = prepared.groups.get("query") else {
        return;
    };
    let Some(ready) = prepared.passes.ready_all(&cache, &[(QUERY, SAMPLE)]) else {
        return;
    };
    let Some(buffers) = buffers.as_ref() else {
        return;
    };
    let Some(gpu_requests) = ssbos.get(&buffers.requests) else {
        return;
    };
    // Queue writes land before this frame's encoder submission, so the
    // dispatch always samples the positions extracted this frame.
    queue.write_buffer(&gpu_requests.buffer, 0, &batch.bytes);
    let steps = vec![pass::Step::Dispatch {
        pipeline: ready.get(QUERY, SAMPLE),
        group,
        workgroups: [batch.count.div_ceil(WORKGROUP_SIZE as usize) as u32, 1, 1],
    }];
    pass::run_spans(&mut context, &[pass::Span::new("aqua_wave_query", steps)]);
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;

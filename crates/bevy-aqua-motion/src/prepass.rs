use std::{
    any::TypeId,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

static QUEUE_LOGGED: AtomicBool = AtomicBool::new(false);
static DRAW_LOGGED: AtomicBool = AtomicBool::new(false);

use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    camera::{MainPassResolutionOverride, Viewport},
    core_pipeline::{
        Core3dSystems,
        prepass::{
            DepthPrepass, MotionVectorPrepass, OpaqueNoLightmap3dBatchSetKey,
            OpaqueNoLightmap3dBinKey, ViewPrepassTextures,
        },
        schedule::Core3d,
    },
    ecs::system::{SystemParamItem, lifetimeless::SRes},
    material::key::{ErasedMaterialPipelineKey, ErasedMeshPipelineKey},
    pbr::{
        DrawMesh, MeshPipelineKey, PreparedMaterial, PrepassPipeline, PrepassPipelineSpecializer,
        RenderMaterialInstances, RenderMeshInstances, SetMaterialBindGroup, SetMeshBindGroup,
        SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderDebugFlags, RenderStartup, RenderSystems,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        diagnostic::RecordDiagnostics,
        erased_render_asset::ErasedRenderAssets,
        mesh::{RenderMesh, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedPhaseItem, BinnedRenderPhasePlugin, BinnedRenderPhaseType,
            DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult,
            SetItemPipeline, TrackedRenderPass, ViewBinnedRenderPhases,
        },
        render_resource::{
            BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId,
            PipelineCache, RenderPassDescriptor, RenderPipelineDescriptor, SamplerBindingType,
            ShaderStages, SpecializedMeshPipeline, SpecializedMeshPipelineError,
            SpecializedMeshPipelines, StoreOp, TextureSampleType,
            binding_types::{sampler, texture_2d_array, uniform_buffer},
        },
        renderer::{RenderContext, ViewQuery},
        sync_world::MainEntity,
        view::{ExtractedView, NoIndirectDrawing, RenderVisibleEntities, ViewDepthTexture},
    },
};
use bevy_aqua_core::{CascadeMaterial, Data, OceanView};

use crate::history::{History, PreviousFrame};

#[derive(Resource, Clone)]
struct MotionPrepassPipeline {
    prepass: PrepassPipeline,
    material_properties: Arc<bevy::material::MaterialProperties>,
    shader: Handle<Shader>,
    history_layout: BindGroupLayoutDescriptor,
}

impl SpecializedMeshPipeline for MotionPrepassPipeline {
    type Key = ErasedMaterialPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let base = PrepassPipelineSpecializer {
            pipeline: self.prepass.clone(),
            properties: self.material_properties.clone(),
        };
        let mut descriptor = base.specialize(key, layout)?;
        descriptor.label = Some("aqua_motion_prepass_pipeline".into());
        descriptor.vertex.shader = self.shader.clone();
        let fragment = descriptor
            .fragment
            .as_mut()
            .expect("motion-vector prepass always has a fragment stage");
        fragment.shader = self.shader.clone();
        descriptor.layout.push(self.history_layout.clone());
        let depth = descriptor
            .depth_stencil
            .as_mut()
            .expect("3d prepass always has depth state");
        // Reverse-Z compare comes from Bevy's prepass specializer. Do not let
        // this transmissive surface modify the opaque prepass depth buffer.
        depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::GreaterEqual);
        depth.depth_write_enabled = Some(false);
        Ok(descriptor)
    }
}

struct AquaMotion3d {
    batch_set_key: OpaqueNoLightmap3dBatchSetKey,
    _bin_key: OpaqueNoLightmap3dBinKey,
    representative_entity: (Entity, MainEntity),
    batch_range: Range<u32>,
    extra_index: PhaseItemExtraIndex,
}

impl PhaseItem for AquaMotion3d {
    fn entity(&self) -> Entity {
        self.representative_entity.0
    }
    fn main_entity(&self) -> MainEntity {
        self.representative_entity.1
    }
    fn draw_function(&self) -> bevy::render::render_phase::DrawFunctionId {
        self.batch_set_key.draw_function
    }
    fn batch_range(&self) -> &Range<u32> {
        &self.batch_range
    }
    fn batch_range_mut(&mut self) -> &mut Range<u32> {
        &mut self.batch_range
    }
    fn extra_index(&self) -> PhaseItemExtraIndex {
        self.extra_index.clone()
    }
    fn batch_range_and_extra_index_mut(&mut self) -> (&mut Range<u32>, &mut PhaseItemExtraIndex) {
        (&mut self.batch_range, &mut self.extra_index)
    }
}

impl BinnedPhaseItem for AquaMotion3d {
    type BatchSetKey = OpaqueNoLightmap3dBatchSetKey;
    type BinKey = OpaqueNoLightmap3dBinKey;
    fn new(
        batch_set_key: Self::BatchSetKey,
        bin_key: Self::BinKey,
        representative_entity: (Entity, MainEntity),
        batch_range: Range<u32>,
        extra_index: PhaseItemExtraIndex,
    ) -> Self {
        Self {
            batch_set_key,
            _bin_key: bin_key,
            representative_entity,
            batch_range,
            extra_index,
        }
    }
}

impl bevy::render::render_phase::CachedRenderPipelinePhaseItem for AquaMotion3d {
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.batch_set_key.pipeline
    }
}

type DrawAquaMotionPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetMeshBindGroup<2>,
    SetMaterialBindGroup<3>,
    SetMotionHistoryBindGroup<4>,
    DrawMesh,
);

struct SetMotionHistoryBindGroup<const I: usize>;

impl<P: bevy::render::render_phase::PhaseItem, const I: usize> RenderCommand<P>
    for SetMotionHistoryBindGroup<I>
{
    type Param = SRes<History>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        history: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let _span = bevy::log::trace_span!("aqua_motion_prepass_draw").entered();
        let Some(group) = history.into_inner().group.as_ref() else {
            return RenderCommandResult::Failure("aqua motion history bind group is not ready");
        };
        pass.set_bind_group(I, group, &[]);
        if !DRAW_LOGGED.swap(true, Ordering::Relaxed) {
            bevy::log::trace!("drew first Aqua motion prepass item");
        }
        RenderCommandResult::Success
    }
}

pub(crate) fn add_render_systems(app: &mut App) {
    embedded_asset!(app, "motion_prepass.wgsl");
    let Some(_) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    app.add_plugins(BinnedRenderPhasePlugin::<
        AquaMotion3d,
        bevy::pbr::MeshPipeline,
    >::new(RenderDebugFlags::default()));
    let render = app.sub_app_mut(RenderApp);
    render
        .init_resource::<SpecializedMeshPipelines<MotionPrepassPipeline>>()
        .init_resource::<DrawFunctions<AquaMotion3d>>()
        .add_render_command::<AquaMotion3d, DrawAquaMotionPrepass>()
        .add_systems(
            RenderStartup,
            init_pipeline
                .after(super::history::init_history)
                .after(bevy::pbr::init_prepass_pipeline),
        )
        .add_systems(
            Render,
            queue_motion_prepass.in_set(RenderSystems::QueueMeshes),
        )
        .add_systems(
            Core3d,
            draw_motion_prepass
                .in_set(crate::AquaMotionSystems::Draw)
                .after(Core3dSystems::Prepass)
                .before(Core3dSystems::MainPass),
        );
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    prepass: Res<PrepassPipeline>,
) {
    let history_layout = BindGroupLayoutDescriptor::new(
        "aqua_motion_history",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                texture_2d_array(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<PreviousFrame>(false),
            ),
        ),
    );
    // The prepared CascadeMaterial properties are instance-independent. They
    // are installed lazily by the queue system once that asset is ready.
    commands.insert_resource(MotionPrepassBootstrap {
        prepass: prepass.clone(),
        shader: load_embedded_asset!(asset_server.as_ref(), "motion_prepass.wgsl"),
        history_layout,
    });
}

#[derive(Resource)]
struct MotionPrepassBootstrap {
    prepass: PrepassPipeline,
    shader: Handle<Shader>,
    history_layout: BindGroupLayoutDescriptor,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn queue_motion_prepass(
    data: Option<Res<Data>>,
    bootstrap: Res<MotionPrepassBootstrap>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    render_materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    material_instances: Res<RenderMaterialInstances>,
    mesh_instances: Res<RenderMeshInstances>,
    mesh_allocator: Res<MeshAllocator>,
    gpu_preprocessing: Res<GpuPreprocessingSupport>,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MotionPrepassPipeline>>,
    draw_functions: Res<DrawFunctions<AquaMotion3d>>,
    mut phases: ResMut<ViewBinnedRenderPhases<AquaMotion3d>>,
    views: Query<(
        &ExtractedView,
        &RenderVisibleEntities,
        &Msaa,
        Option<&DepthPrepass>,
        Option<&MotionVectorPrepass>,
        Option<&OceanView>,
        Option<&ViewPrepassTextures>,
        Has<NoIndirectDrawing>,
    )>,
) {
    // Each active view's bins are reset before queueing, and inactive views
    // are removed below. This prevents retained render entities from going stale.
    let mut live_views = std::collections::HashSet::new();
    let _span = bevy::log::trace_span!("aqua_motion_prepass_queue").entered();
    let Some(data) = data else {
        phases.clear();
        return;
    };
    let target = data.material().id().untyped();
    let draw_function = draw_functions.read().id::<DrawAquaMotionPrepass>();

    for (view, visible, msaa, depth, motion, ocean, textures, no_indirect) in &views {
        if motion.is_none()
            || ocean.is_none()
            || !textures.is_some_and(|textures| textures.motion_vectors.is_some())
        {
            continue;
        }
        let retained = view.retained_view_entity;
        let mode = gpu_preprocessing.min(if no_indirect {
            GpuPreprocessingMode::PreprocessingOnly
        } else {
            GpuPreprocessingMode::Culling
        });
        phases.prepare_for_new_frame(retained, mode);
        live_views.insert(retained);
        let phase = phases
            .get_mut(&retained)
            .expect("phase was prepared for the active OceanView");
        let Some(visible_meshes) = visible.get::<Mesh3d>() else {
            continue;
        };
        let visible_meshes = visible_meshes.entities_cpu_culling.iter().copied().chain(
            visible_meshes
                .entities_gpu_culling
                .iter()
                .map(|(main_entity, render_entity)| (*render_entity, *main_entity)),
        );
        for (render_entity, main_entity) in visible_meshes {
            let Some(material_instance) = material_instances.instances.get(&main_entity) else {
                continue;
            };
            if material_instance.asset_id != target {
                continue;
            }
            let Some(material) = render_materials.get(material_instance.asset_id) else {
                continue;
            };
            let Some(mesh_instance) = mesh_instances.render_mesh_queue_data(main_entity) else {
                continue;
            };
            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id()) else {
                continue;
            };
            let Some(slabs) = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id()) else {
                continue;
            };

            let mut mesh_key = MeshPipelineKey::from_msaa_samples(msaa.samples())
                | MeshPipelineKey::MOTION_VECTOR_PREPASS
                | MeshPipelineKey::from_bits_retain(mesh.key_bits.bits());
            // Preserve optional depth-prepass specialization without changing
            // writes: this custom pipeline always leaves depth untouched.
            if depth.is_some() {
                mesh_key |= MeshPipelineKey::DEPTH_PREPASS;
            }
            let erased_key = ErasedMaterialPipelineKey {
                mesh_key: ErasedMeshPipelineKey::new(mesh_key),
                material_key: material.properties.material_key.clone(),
                type_id: TypeId::of::<CascadeMaterial>(),
            };
            let specializer = MotionPrepassPipeline {
                prepass: bootstrap.prepass.clone(),
                material_properties: material.properties.clone(),
                shader: bootstrap.shader.clone(),
                history_layout: bootstrap.history_layout.clone(),
            };
            let pipeline: CachedRenderPipelineId = match pipelines.specialize(
                &pipeline_cache,
                &specializer,
                erased_key,
                &mesh.layout,
            ) {
                Ok(pipeline) => pipeline,
                Err(error) => {
                    bevy::log::error!(?main_entity, %error, "failed to specialize Aqua motion prepass");
                    continue;
                }
            };

            phase.add(
                OpaqueNoLightmap3dBatchSetKey {
                    draw_function,
                    pipeline,
                    material_bind_group_index: Some(material.binding.group.0),
                    slabs,
                },
                OpaqueNoLightmap3dBinKey {
                    asset_id: mesh_instance.mesh_asset_id().into(),
                },
                (render_entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::mesh(mesh_instance.should_batch(), &gpu_preprocessing),
            );
            if !QUEUE_LOGGED.swap(true, Ordering::Relaxed) {
                bevy::log::trace!(?main_entity, "queued first Aqua motion prepass item");
            }
        }
    }
    phases.retain(|retained, _| live_views.contains(retained));
}

type AquaMotionViewQueryData = (
    &'static bevy::render::camera::ExtractedCamera,
    &'static ExtractedView,
    &'static ViewDepthTexture,
    &'static ViewPrepassTextures,
    Option<&'static MainPassResolutionOverride>,
    Option<&'static OceanView>,
    Option<&'static MotionVectorPrepass>,
);

fn draw_motion_prepass(
    world: &World,
    view: ViewQuery<AquaMotionViewQueryData>,
    phases: Res<ViewBinnedRenderPhases<AquaMotion3d>>,
    mut ctx: RenderContext,
) {
    let view_entity = view.entity();
    let (camera, extracted_view, depth, textures, resolution_override, ocean, motion) =
        view.into_inner();
    if ocean.is_none() || motion.is_none() {
        return;
    }
    let Some(motion_vectors) = textures.motion_vectors.as_ref() else {
        return;
    };
    let Some(phase) = phases.get(&extracted_view.retained_view_entity) else {
        return;
    };
    if phase.is_empty() {
        return;
    }

    let color_attachments = [None, Some(motion_vectors.get_attachment()), None, None];
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("aqua_motion_prepass"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    let pass_span = diagnostics.pass_span(&mut pass, "aqua_motion_prepass");
    if let Some(viewport) =
        Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution_override)
    {
        pass.set_camera_viewport(&viewport);
    }
    if let Err(error) = phase.render(&mut pass, world, view_entity) {
        bevy::log::error!(?error, "failed to render Aqua motion phase");
    }
    pass_span.end(&mut pass);
}

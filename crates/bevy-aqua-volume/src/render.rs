//! Render-world fullscreen underwater volume composite.

use bevy::{
    asset::load_embedded_asset,
    core_pipeline::{
        FullscreenShader,
        schedule::{Core3d, Core3dSystems},
    },
    pbr::{
        MeshPipelineSystems, MeshPipelineViewLayoutKey, MeshPipelineViewLayouts, MeshViewBindGroup,
        ViewKeyCache,
    },
    prelude::*,
    render::{
        GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems,
        diagnostic::RecordDiagnostics,
        render_asset::RenderAssets,
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
            FragmentState, LoadOp, Operations, PipelineCache, RenderPassColorAttachment,
            RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
            SamplerDescriptor, ShaderStages, ShaderType, SpecializedRenderPipeline,
            SpecializedRenderPipelines, StoreOp, Texture, TextureDescriptor, TextureDimension,
            TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
            TextureViewDimension, UniformBuffer,
            binding_types::{
                sampler, texture_2d, texture_2d_array, texture_depth_2d,
                texture_depth_2d_multisampled, uniform_buffer,
            },
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::GpuImage,
        view::{ExtractedView, ViewDepthTexture, ViewTarget},
    },
    shader::ShaderDefVal,
};
use bevy_aqua_core::{
    AnimWavesUniform, AnimWavesUniformSlot, Data, GpuLayout, GpuWave, LOD_COUNT, OceanView,
    WAVE_SLOTS, pass,
};

use super::ExtractedVolume;

pub(super) fn add(app: &mut App) {
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .init_gpu_resource::<SpecializedRenderPipelines<VolumePipeline>>()
        .add_systems(
            RenderStartup,
            init_pipeline.after(MeshPipelineSystems),
        )
        .add_systems(
            Render,
            (
                prepare_pipelines.in_set(RenderSystems::Prepare),
                prepare_depth_usages
                    .in_set(RenderSystems::Prepare)
                    .before(bevy::core_pipeline::core_3d::prepare_core_3d_depth_textures),
            ),
        )
        .add_systems(
            Core3d,
            draw_volume
                .after(Core3dSystems::MainPass)
                .before(Core3dSystems::EarlyPostProcess),
        );
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct VolumeUniform {
    extinction: Vec4,
    environment: Vec4,
    sea: Vec4,
}

#[derive(Resource)]
struct VolumePipeline {
    mesh_view_layouts: MeshPipelineViewLayouts,
    sampler: Sampler,
    waves_sampler: Sampler,
    _fallback_waves: Texture,
    fallback_waves_view: TextureView,
    layout: BindGroupLayoutDescriptor,
    layout_msaa: BindGroupLayoutDescriptor,
    fullscreen_shader: FullscreenShader,
    fragment_shader: Handle<Shader>,
}

#[derive(Resource)]
struct Prepared {
    uniform: Option<UniformBuffer<VolumeUniform>>,
    anim_waves: Option<UniformBuffer<AnimWavesUniform>>,
}

impl VolumePipeline {
    fn new(
        render_device: &RenderDevice,
        mesh_view_layouts: MeshPipelineViewLayouts,
        fullscreen_shader: FullscreenShader,
        fragment_shader: Handle<Shader>,
    ) -> Self {
        let entries = BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_depth_2d(),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<VolumeUniform>(false),
                texture_2d_array(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<AnimWavesUniform>(false),
            ),
        );
        let entries_msaa = BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_depth_2d_multisampled(),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<VolumeUniform>(false),
                texture_2d_array(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<AnimWavesUniform>(false),
            ),
        );
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        });
        let waves_sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("aqua_volume_waves"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        });
        let fallback_waves = render_device.create_texture(&TextureDescriptor {
            label: Some("aqua_volume_waves_fallback"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: LOD_COUNT as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let fallback_waves_view = fallback_waves.create_view(&TextureViewDescriptor {
            label: Some("aqua_volume_waves_fallback_view"),
            dimension: Some(TextureViewDimension::D2Array),
            ..default()
        });
        Self {
            mesh_view_layouts,
            sampler,
            waves_sampler,
            _fallback_waves: fallback_waves,
            fallback_waves_view,
            layout: BindGroupLayoutDescriptor::new("aqua_volume_layout", &entries),
            layout_msaa: BindGroupLayoutDescriptor::new("aqua_volume_layout_msaa", &entries_msaa),
            fullscreen_shader,
            fragment_shader,
        }
    }
}

fn init_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    mesh_view_layouts: Res<MeshPipelineViewLayouts>,
    fullscreen_shader: Res<FullscreenShader>,
    asset_server: Res<AssetServer>,
) {
    let fragment_shader = load_embedded_asset!(asset_server.as_ref(), "volume.wgsl");
    commands.insert_resource(VolumePipeline::new(
        &render_device,
        mesh_view_layouts.clone(),
        fullscreen_shader.clone(),
        fragment_shader,
    ));
    commands.insert_resource(Prepared {
        uniform: None,
        anim_waves: None,
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct VolumePipelineKey {
    mesh_pipeline_view_key: MeshPipelineViewLayoutKey,
    target_format: TextureFormat,
    samples: u32,
}

impl SpecializedRenderPipeline for VolumePipeline {
    type Key = VolumePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let view_layout = self
            .mesh_view_layouts
            .get_view_layout(key.mesh_pipeline_view_key);
        let volume_layout = if key.samples > 1 {
            self.layout_msaa.clone()
        } else {
            self.layout.clone()
        };
        let mut shader_defs = Vec::new();
        if key.samples > 1 {
            shader_defs.push(ShaderDefVal::from("MULTISAMPLED"));
        }
        if key
            .mesh_pipeline_view_key
            .contains(MeshPipelineViewLayoutKey::ATMOSPHERE)
        {
            shader_defs.push(ShaderDefVal::from("ATMOSPHERE"));
        }
        RenderPipelineDescriptor {
            label: Some("aqua_volume_pipeline".into()),
            layout: vec![view_layout.main_layout, volume_layout],
            vertex: self.fullscreen_shader.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.fragment_shader.clone(),
                shader_defs,
                targets: vec![Some(ColorTargetState {
                    format: key.target_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        }
    }
}

#[derive(Component)]
struct VolumePipelineId(CachedRenderPipelineId);

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<VolumePipeline>>,
    pipeline: Res<VolumePipeline>,
    view_key_cache: Res<ViewKeyCache>,
    cameras: Query<(Entity, &ExtractedView, &Msaa), With<OceanView>>,
) {
    for (entity, view, msaa) in &cameras {
        let Some(mesh_pipeline_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };
        let pipeline_id = pipelines.specialize(
            &pipeline_cache,
            &pipeline,
            VolumePipelineKey {
                mesh_pipeline_view_key: (*mesh_pipeline_key).into(),
                target_format: view.target_format,
                samples: msaa.samples(),
            },
        );
        commands
            .entity(entity)
            .insert(VolumePipelineId(pipeline_id));
    }
}

fn prepare_depth_usages(mut cameras: Query<&mut Camera3d, With<OceanView>>) {
    for mut camera in &mut cameras {
        camera.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
    }
}

fn volume_uniform(volume: &ExtractedVolume) -> VolumeUniform {
    let optics = volume.optics;
    VolumeUniform {
        extinction: optics.extinction.extend(optics.scatter_scale.max(0.0)),
        environment: Vec4::new(
            volume.optics.scattering_asymmetry,
            if volume.sample_waves { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ),
        sea: Vec4::new(volume.surface_level, 0.0, 0.0, 0.0),
    }
}

fn fallback_anim_waves(layout: GpuLayout) -> AnimWavesUniform {
    AnimWavesUniform {
        layout,
        waves: [GpuWave::default(); WAVE_SLOTS],
        ranges: [UVec4::ZERO; LOD_COUNT],
        time: Vec4::ZERO,
        flow: Vec4::ZERO,
    }
}

fn draw_volume(
    view: ViewQuery<(
        Option<&OceanView>,
        Option<&VolumePipelineId>,
        Option<&MeshViewBindGroup>,
        &ViewTarget,
        &ViewDepthTexture,
        &Msaa,
    )>,
    volume: Res<ExtractedVolume>,
    pipeline: Res<VolumePipeline>,
    pipeline_cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    data: Option<Res<Data>>,
    waves_slot: Option<Res<AnimWavesUniformSlot>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut prepared: ResMut<Prepared>,
    mut ctx: RenderContext,
) {
    let (ocean_view, pipeline_id, view_bind_group, view_target, depth, msaa) = view.into_inner();
    if ocean_view.is_none() || !volume.active {
        return;
    }
    let Some(pipeline_id) = pipeline_id else {
        return;
    };
    let Some(view_bind_group) = view_bind_group else {
        return;
    };
    let Some(gpu_pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.0) else {
        return;
    };

    pass::write_uniform(
        &mut prepared.uniform,
        volume_uniform(&volume),
        &device,
        &queue,
    );
    let slot_binding = waves_slot
        .as_ref()
        .and_then(|slot| slot.0.as_ref().and_then(UniformBuffer::binding));
    if slot_binding.is_none() {
        let waves_layout = data.as_ref().map_or_else(
            || GpuLayout::new(&bevy_aqua_core::layout(Vec2::ZERO), Vec2::ZERO, 0.0),
            |data| data.layout().clone(),
        );
        pass::write_uniform(
            &mut prepared.anim_waves,
            fallback_anim_waves(waves_layout),
            &device,
            &queue,
        );
    }
    let Some(uniform) = prepared.uniform.as_ref().and_then(UniformBuffer::binding) else {
        return;
    };
    let Some(anim_waves) = slot_binding.or_else(|| {
        prepared
            .anim_waves
            .as_ref()
            .and_then(UniformBuffer::binding)
    }) else {
        return;
    };

    let waves_view = data
        .as_ref()
        .and_then(|data| images.get(&data.texture()))
        .map(|image| &image.texture_view)
        .unwrap_or(&pipeline.fallback_waves_view);

    let post_process = view_target.post_process_write();
    let layout = if msaa.samples() > 1 {
        &pipeline.layout_msaa
    } else {
        &pipeline.layout
    };
    let bind_group = ctx.render_device().create_bind_group(
        Some("aqua_volume_bind_group"),
        &pipeline_cache.get_bind_group_layout(layout),
        &BindGroupEntries::sequential((
            post_process.source,
            depth.view(),
            &pipeline.sampler,
            uniform,
            waves_view,
            &pipeline.waves_sampler,
            anim_waves,
        )),
    );

    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut render_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("aqua_volume"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Default::default()),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    let pass_span = diagnostics.pass_span(&mut render_pass, "aqua_volume");
    render_pass.set_render_pipeline(gpu_pipeline);
    render_pass.set_bind_group(0, &view_bind_group.main, &view_bind_group.main_offsets);
    render_pass.set_bind_group(1, &bind_group, &[]);
    render_pass.draw(0..3, 0..1);
    pass_span.end(&mut render_pass);
}

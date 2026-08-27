//! One declarative pass table and one command-encoding helper, shared by the
//! AnimWaves, foam, and wave-query render systems.
//!
//! Each render module declares its compute passes as [`PassSpec`] rows (shader,
//! entry points, sequential bind-group layout). [`Passes`] queues one pipeline
//! per entry point and resolves them once compiled; [`Groups`] holds the bind
//! groups created from those layouts; [`run_spans`] encodes ordered dispatches
//! and copies under diagnostics spans.

use std::borrow::Cow;

use bevy::render::render_resource::encase::private::WriteInto;
use bevy::{
    asset::AssetPath,
    prelude::*,
    render::{
        diagnostic::RecordDiagnostics,
        render_resource::*,
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
    shader::ShaderDefVal,
};

/// Where one pass row's compute shader comes from: an asset path (usually
/// embedded) or a pre-registered handle for literal WGSL owned by another
/// crate (single-source shader consts, e.g. the Stockham transform in
/// `bevy-aqua-fft`).
#[derive(Clone, Debug)]
pub enum ShaderSource {
    Path(&'static str),
    Handle(Handle<Shader>),
}

/// One row of a module's pass table.
#[derive(Debug, Clone)]
pub struct PassSpec {
    /// Lookup key shared by every entry point that uses one layout.
    pub key: &'static str,
    pub shader: ShaderSource,
    /// Queued once per entry point; looked up as `(key, entry)`.
    pub entry_points: &'static [&'static str],
    /// Per-entry naga_oil defs, same length as `entry_points`, or empty.
    ///
    /// WebGPU's naga round-trip keeps one compute entry per module and can
    /// rename functions, so multi-entry shaders specialize to a single `main`
    /// and look the variants up by `entry_points`.
    pub shader_defs: &'static [&'static [&'static str]],
    /// WGSL compute function queued for every variant. `None` uses each lookup name.
    pub wgsl_entry: Option<&'static str>,
    /// Sequential compute layout, generated from the entry list.
    pub layout: BindGroupLayoutDescriptor,
}

/// Every compute pipeline of one render module, queued from its pass table.
#[derive(Resource, Debug)]
pub struct Passes {
    specs: Vec<PassSpec>,
    ids: Vec<Vec<CachedComputePipelineId>>,
}

impl Passes {
    /// Loads the shaders and queues one pipeline per entry point. Call once at
    /// `RenderStartup` from the module's pass table.
    pub fn new(asset_server: &AssetServer, cache: &PipelineCache, specs: Vec<PassSpec>) -> Self {
        let shader_handles: Vec<Handle<Shader>> = specs
            .iter()
            .map(|spec| match &spec.shader {
                ShaderSource::Path(path) => asset_server.load(AssetPath::parse(path)),
                ShaderSource::Handle(handle) => handle.clone(),
            })
            .collect();
        let ids = specs
            .iter()
            .zip(shader_handles.iter().cloned())
            .map(|(spec, shader)| {
                debug_assert!(
                    spec.shader_defs.is_empty()
                        || spec.shader_defs.len() == spec.entry_points.len()
                );
                spec.entry_points
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| {
                        let shader_defs = spec
                            .shader_defs
                            .get(index)
                            .copied()
                            .unwrap_or(&[])
                            .iter()
                            .copied()
                            .map(ShaderDefVal::from)
                            .collect();
                        let wgsl_entry = spec.wgsl_entry.unwrap_or(*entry);
                        cache.queue_compute_pipeline(ComputePipelineDescriptor {
                            label: Some(Cow::Owned(format!("{}::{entry}", spec.key))),
                            layout: vec![spec.layout.clone()],
                            shader: shader.clone(),
                            shader_defs,
                            entry_point: Some(Cow::Borrowed(wgsl_entry)),
                            ..default()
                        })
                    })
                    .collect()
            })
            .collect();
        Self { specs, ids }
    }

    fn locate(&self, key: &str, entry: &str) -> Option<CachedComputePipelineId> {
        let (spec, ids) = self
            .specs
            .iter()
            .zip(&self.ids)
            .find(|(spec, _)| spec.key == key)?;
        spec.entry_points
            .iter()
            .copied()
            .zip(ids)
            .find(|(point, _)| *point == entry)
            .map(|(_, id)| *id)
    }

    /// The layout of a table row, for creating its bind groups.
    pub fn layout(&self, key: &str) -> Option<&BindGroupLayoutDescriptor> {
        self.specs
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| &spec.layout)
    }

    /// Resolves one `(key, entry)` pipeline.
    ///
    /// Aqua's shader-error policy is fail closed: cold or failed pipelines
    /// return `None`, and callers skip the complete dependent span. The Bevy
    /// [`PipelineCache`] owns the compile diagnostic; Aqua never panics the
    /// camera schedule for an asset or composition error.
    pub fn ready<'c>(
        &self,
        cache: &'c PipelineCache,
        key: &str,
        entry: &str,
    ) -> Option<&'c ComputePipeline> {
        let id = self.locate(key, entry)?;
        match cache.get_compute_pipeline_state(id) {
            CachedPipelineState::Ok(_) => cache.get_compute_pipeline(id),
            CachedPipelineState::Err(_)
            | CachedPipelineState::Queued
            | CachedPipelineState::Creating(_) => None,
        }
    }

    /// Resolves every requested `(key, entry)` pass at once, or none, so write
    /// systems open no diagnostics spans on a cold or failed frame.
    pub fn ready_all<'c>(
        &self,
        cache: &'c PipelineCache,
        passes: &[(&'static str, &'static str)],
    ) -> Option<Ready<'c>> {
        let pipelines = passes
            .iter()
            .map(|&(key, entry)| self.ready(cache, key, entry))
            .collect::<Option<Vec<_>>>()?;
        Some(Ready {
            keys: passes.to_vec(),
            pipelines,
        })
    }
}

/// Pipelines resolved by [`Passes::ready_all`], looked back up by `(key, entry)`.
#[derive(Debug)]
pub struct Ready<'c> {
    keys: Vec<(&'static str, &'static str)>,
    pipelines: Vec<&'c ComputePipeline>,
}

impl<'c> Ready<'c> {
    /// Looks up a pipeline resolved by a previous [`Passes::ready_all`].
    pub fn get(&self, key: &str, entry: &str) -> &'c ComputePipeline {
        let index = self
            .keys
            .iter()
            .position(|&(k, e)| k == key && e == entry)
            .expect("requested pass was part of ready_all");
        self.pipelines[index]
    }
}

/// The bind groups of one render module, created once and looked up by key.
#[derive(Resource, Default, Debug)]
pub struct Groups {
    groups: Vec<(&'static str, BindGroup)>,
}

impl Groups {
    /// True once at least one group has been registered.
    pub fn created(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Looks up a registered bind group by its key.
    pub fn get(&self, key: &str) -> Option<&BindGroup> {
        self.groups
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, group)| group)
    }

    /// Registers one bind group under `key`, replacing an existing value.
    pub fn register(&mut self, key: &'static str, group: BindGroup) {
        if let Some((_, existing)) = self.groups.iter_mut().find(|(name, _)| *name == key) {
            *existing = group;
        } else {
            self.groups.push((key, group));
        }
    }
}

/// Creates one bind group from a pass-table row's sequential layout.
pub fn bind_group<const N: usize>(
    device: &RenderDevice,
    cache: &PipelineCache,
    passes: &Passes,
    pass_key: &str,
    label: &'static str,
    entries: &BindGroupEntries<'_, N>,
) -> BindGroup {
    let layout = passes
        .layout(pass_key)
        .unwrap_or_else(|| panic!("unknown pass {pass_key}"));
    let layout = cache.get_bind_group_layout(layout);
    device.create_bind_group(Some(label), &layout, entries)
}

/// Writes a per-frame uniform, creating the GPU buffer on first use.
pub fn write_uniform<T: ShaderType + WriteInto + Clone>(
    slot: &mut Option<UniformBuffer<T>>,
    value: T,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    let uniform = slot.get_or_insert_with(|| UniformBuffer::from(value.clone()));
    uniform.set(value);
    uniform.write_buffer(device, queue);
}

/// One command inside a diagnostics span.
#[derive(Debug)]
pub enum Step<'a> {
    /// One compute dispatch through a prepared pipeline and bind group.
    Dispatch {
        pipeline: &'a ComputePipeline,
        group: &'a BindGroup,
        /// Workgroup grid `[x, y, z]`.
        workgroups: [u32; 3],
    },
    /// Full-extent texture copy at default origin (foam state publish).
    CopyTexture {
        source: &'a Texture,
        target: &'a Texture,
        extent: Extent3d,
    },
}

/// A diagnostics span covering an ordered list of commands.
#[derive(Debug)]
pub struct Span<'a> {
    pub label: &'static str,
    pub steps: Vec<Step<'a>>,
}

impl<'a> Span<'a> {
    /// Creates one span from its label and command list.
    pub fn new(label: &'static str, steps: Vec<Step<'a>>) -> Self {
        Self { label, steps }
    }
}

/// Encodes every span's dispatches and copies through the same
/// begin/set/dispatch sequence the former per-system writers used.
pub fn run_spans(context: &mut RenderContext, spans: &[Span]) {
    let recorder = context.diagnostic_recorder();
    let diagnostics = recorder.as_deref();
    for span in spans {
        let timed = diagnostics.time_span(context.command_encoder(), span.label);
        for step in &span.steps {
            match *step {
                Step::Dispatch {
                    pipeline,
                    group,
                    workgroups,
                } => {
                    let descriptor = ComputePassDescriptor {
                        label: Some(span.label),
                        ..default()
                    };
                    let mut pass = context.command_encoder().begin_compute_pass(&descriptor);
                    pass.set_bind_group(0, group, &[]);
                    pass.set_pipeline(pipeline);
                    let [x, y, z] = workgroups;
                    pass.dispatch_workgroups(x, y, z);
                }
                Step::CopyTexture {
                    source,
                    target,
                    extent,
                } => {
                    context.command_encoder().copy_texture_to_texture(
                        TexelCopyTextureInfo {
                            texture: source,
                            mip_level: 0,
                            origin: Origin3d::default(),
                            aspect: TextureAspect::All,
                        },
                        TexelCopyTextureInfo {
                            texture: target,
                            mip_level: 0,
                            origin: Origin3d::default(),
                            aspect: TextureAspect::All,
                        },
                        extent,
                    );
                }
            }
        }
        timed.end(context.command_encoder());
    }
}

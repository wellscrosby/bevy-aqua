//! Global water fields: one baked pair of textures covering every
//! resolved [`crate::WaterBody`] + [`crate::WaterShape`] entities.
//!
//! The camera-centred ring tiles are the only water mesh. Each texel
//! resolves which body owns the point: `level_id` stores the surface level
//! (r) and the one-based body slot (g); `flow` stores the per-texel river
//! sample (xy current, z signed bank margin, w channel half-width). Bounded
//! vertices read their level here and
//! fragments resolve optics, banks, and culling from the slot parameters.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, ShaderType, TextureDimension, TextureFormat},
};

use crate::cascade::BodyParams;
use crate::{AmortizedBake, ResolvedWaterBody};

/// Hard cap on registered bounded bodies (uniform array size).
pub const MAX_BODIES: usize = 16;

/// Uniform mirror of the baked fields, matching `FieldParams` in
/// `cascade/common.wgsl`.
#[derive(ShaderType, Debug, Clone, Copy)]
pub struct FieldParams {
    /// xy: region minimum in metres; zw: region size in metres.
    pub region: Vec4,
    /// x: bounded body count; y: 1.0 when the Ocean resource is present;
    /// z: metres per texel; w: reserved.
    pub meta: Vec4,
    /// Per-slot body parameters; slot i lives at bodies[i - 1].
    pub bodies: [BodyParams; MAX_BODIES],
}

impl FieldParams {
    /// The inert all-ocean uniform used before any bake.
    pub fn none() -> Self {
        Self {
            region: Vec4::ZERO,
            meta: Vec4::ZERO,
            bodies: [BodyParams::ocean(); MAX_BODIES],
        }
    }
}

/// The baked global fields and the uniform that maps them.
#[derive(Resource, Debug)]
pub struct WaterFields {
    pub params: FieldParams,
    pub level_id: Handle<Image>,
    pub flow: Handle<Image>,
    /// Rebake scheduler: runs the CPU bake only when the body set changes.
    pub bakes: AmortizedBake<(bool, Vec<ResolvedWaterBody>)>,
}

impl FromWorld for WaterFields {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        let mut level_id = Image::new_fill(
            Extent3d::default(),
            TextureDimension::D2,
            &[0; 4], // one Rg16Float texel; new_fill tiles it over the image
            TextureFormat::Rg16Float,
            RenderAssetUsages::default(),
        );
        level_id.sampler = ImageSampler::linear();
        let mut flow = Image::new_fill(
            Extent3d::default(),
            TextureDimension::D2,
            &[0; 8], // one Rgba16Float texel
            TextureFormat::Rgba16Float,
            RenderAssetUsages::default(),
        );
        flow.sampler = ImageSampler::linear();
        Self {
            params: FieldParams::none(),
            level_id: images.add(level_id),
            flow: images.add(flow),
            bakes: AmortizedBake::new(),
        }
    }
}

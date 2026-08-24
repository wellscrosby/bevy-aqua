//! Shared camera-tracking state written by the umbrella and read by every
//! render participant.

use bevy::prelude::*;

/// The active camera's world-XZ position, refreshed each frame.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewPos(pub Vec2);

/// The projected detail LOD: how many coarse cascades the current camera
/// altitude makes visually irrelevant.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewDetail(pub f32);

/// The ocean root's Y translation (sea level) for the active frame.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewSeaLevel(pub f32);

/// The active camera's order discriminator.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewOrder(pub isize);

/// Marks the one 3D camera whose view drives Aqua's cascades this frame.
#[derive(Component, Debug, Clone, Copy, bevy::render::extract_component::ExtractComponent)]
pub struct OceanView;

const MIN_SCREEN_SAMPLES_PER_WAVE: f32 = 4.0;
const BASE_MIN_WAVELENGTH: f32 = 0.75;

/// The continuous detail-LOD value for a camera at `altitude` above sea
/// level with the given projection and viewport height. The vertex path
/// clamps cascade selection by this so tiny near-camera waves fade before
/// they alias; the wave-query pass mirrors the same clamp in its shader.
pub fn projected_detail_lod(altitude: f32, projection: &Projection, viewport_height: f32) -> f32 {
    let world_height = match projection {
        Projection::Perspective(projection) => 2.0 * altitude * (0.5 * projection.fov).tan(),
        Projection::Orthographic(projection) => projection.area.height(),
        Projection::Custom(_) => return 0.0,
    };
    let minimum_wavelength = MIN_SCREEN_SAMPLES_PER_WAVE * world_height / viewport_height;
    (minimum_wavelength / BASE_MIN_WAVELENGTH)
        .log2()
        .clamp(0.0, (crate::cascade::LOD_COUNT - 1) as f32)
}

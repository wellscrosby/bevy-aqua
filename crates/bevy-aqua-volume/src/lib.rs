//! Underwater volume composite for Aqua.
//!
//! When the camera sits below the local water surface, a fullscreen Core3d
//! pass applies RGB Beer-Lambert transmittance and closed-form in-scatter
//! along the underwater segment. Empty far-plane pixels integrate a bounded
//! path through the water instead of reconstructing the far clip as the path
//! end. Extinction is absorption plus scatter. Particle scatter is a weak
//! coefficient times [`WaterOptics::scatter_scale`] and
//! [`WaterOptics::scatter_tint`], plus molecular Rayleigh, clamped below `σt`,
//! so red dies instead of glowing.
//! In-scatter colour is the downwelling light after a normalized mix of
//! particle Henyey-Greenstein and Rayleigh. The cascade surface converts
//! the same integral to air as water-leaving radiance along the camera ray
//! that sampled transmission (in-water radiance / n²).
//!
//! Each directional light is refracted at the surface (Snell, Fresnel) and
//! then attenuated along `depth / L.y`, so a lower sun dies faster with
//! depth. Looking toward the sun is brighter via Henyey-Greenstein using
//! [`WaterOptics::scattering_asymmetry`]. `VolumetricLight` is not used.
//!
//! Opaque scene colour is scaled by that same surface-to-hit sun path before
//! the camera-path integral, from the depth buffer. Meshes do not opt in.
//!
//! The cascade mesh writes the water-to-air interface on back faces. This
//! pass integrates camera-to-hit, or to the mean plane on empty upward
//! pixels. A single cascade sample at the camera keeps a crest underwater
//! and rejects air. Above water the pass is skipped.

#![warn(unreachable_pub)]

use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::extract_resource::{ExtractResource, ExtractResourcePlugin},
};
use bevy_aqua_core::{
    AquaSettings, Ocean, OceanView, ResolvedWaterBodies, ResolvedWaterBody, WaterBodiesResolved,
    WaterOptics,
};

mod render;

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;

/// Adds the underwater volume composite after the main 3D pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct AquaVolumePlugin;

impl Plugin for AquaVolumePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "volume.wgsl");
        app.init_resource::<ExtractedVolume>()
            .add_plugins(ExtractResourcePlugin::<ExtractedVolume>::default())
            .add_systems(PostUpdate, detect_underwater.after(WaterBodiesResolved));
        render::add(app);
    }
}

/// GPU payload extracted each frame. Inactive frames skip the composite.
#[derive(Resource, Clone, ExtractResource, Debug)]
struct ExtractedVolume {
    active: bool,
    surface_level: f32,
    sample_waves: bool,
    optics: WaterOptics,
}

impl Default for ExtractedVolume {
    fn default() -> Self {
        Self {
            active: false,
            surface_level: 0.0,
            sample_waves: false,
            optics: WaterOptics::DEEP_OCEAN,
        }
    }
}

/// How far above the mean plane the CPU still considers the camera possibly
/// inside a wave crest, so the shader can make the exact call.
const SURFACE_MARGIN: f32 = 12.0;

/// Surface level, optics, and whether ocean cascade displacement applies.
pub(crate) fn sample_medium(
    camera: Vec3,
    ocean: Option<&Ocean>,
    settings: &AquaSettings,
    bodies: &[ResolvedWaterBody],
) -> Option<(f32, WaterOptics, bool)> {
    let xz = camera.xz();
    let mut best: Option<(f32, WaterOptics)> = None;
    for body in bodies {
        if camera.y < body.level + SURFACE_MARGIN && body.contains(xz) {
            let optics = body.optics.unwrap_or(settings.water_optics);
            if best.is_none_or(|(level, _)| body.level > level) {
                best = Some((body.level, optics));
            }
        }
    }
    if let Some((level, optics)) = best {
        return Some((level, optics, false));
    }
    let ocean = ocean?;
    (camera.y < ocean.level + SURFACE_MARGIN).then_some((ocean.level, settings.water_optics, true))
}

fn detect_underwater(
    cameras: Query<&GlobalTransform, With<OceanView>>,
    ocean: Option<Res<Ocean>>,
    settings: Res<AquaSettings>,
    bodies: Res<ResolvedWaterBodies>,
    mut volume: ResMut<ExtractedVolume>,
) {
    let Some(transform) = cameras.iter().next() else {
        *volume = ExtractedVolume::default();
        return;
    };
    let Some((surface_level, optics, sample_waves)) = sample_medium(
        transform.translation(),
        ocean.as_deref(),
        &settings,
        &bodies.0,
    ) else {
        *volume = ExtractedVolume::default();
        return;
    };

    *volume = ExtractedVolume {
        active: true,
        surface_level,
        sample_waves,
        optics,
    };
}

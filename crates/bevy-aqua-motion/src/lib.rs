#![warn(unreachable_pub)]

//! Motion-vector integration for Aqua's animated wave surfaces.
//!
//! Adding [`AquaMotionPlugin`] enables Bevy's `MotionVectorPrepass` on the
//! camera marked [`bevy_aqua_core::OceanView`]. For a temporary runtime spike gate,
//! run `cargo run --features motion --example ocean`.
//! Set `RUST_LOG=bevy_aqua_motion=trace,wgpu_core=warn` to see the queue/draw spans.
//! Add or remove `DepthPrepass` on that camera to compare the depth-feature
//! variants; Aqua's motion item always keeps depth writes disabled.

use bevy::{
    prelude::*,
    render::extract_resource::{ExtractResource, ExtractResourcePlugin},
};

mod history;
mod prepass;

/// Adds motion-vector support for Aqua's animated wave surfaces.
#[derive(Debug, Default, Clone, Copy)]
pub struct AquaMotionPlugin;

/// Render schedule sets exposed for ordering work around Aqua motion output.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum AquaMotionSystems {
    /// Draws Aqua motion vectors after Bevy's prepass and before the main pass.
    Draw,
}

#[derive(Resource, Clone, Copy, Debug, Default, ExtractResource)]
pub(crate) struct MotionEpoch(pub(crate) u64);

impl Plugin for AquaMotionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MotionEpoch>()
            .add_plugins(ExtractResourcePlugin::<MotionEpoch>::default())
            .add_systems(
                PostUpdate,
                (enable_motion_prepass, advance_motion_epoch)
                    .after(bevy_aqua_core::CascadeMaterialsUpdated)
                    .after(bevy_aqua_core::WaterBodiesResolved),
            );
        history::add_render_systems(app);
        prepass::add_render_systems(app);
    }
}

fn enable_motion_prepass(
    mut commands: Commands,
    views: Query<
        Entity,
        (
            With<bevy_aqua_core::OceanView>,
            Without<bevy::core_pipeline::prepass::MotionVectorPrepass>,
        ),
    >,
) {
    for entity in &views {
        commands
            .entity(entity)
            .insert(bevy::core_pipeline::prepass::MotionVectorPrepass);
    }
}

fn advance_motion_epoch(
    mut epoch: ResMut<MotionEpoch>,
    waves: Res<bevy_aqua_core::OceanWaves>,
    bodies: Res<bevy_aqua_core::ResolvedWaterBodies>,
    ocean: Option<Res<bevy_aqua_core::Ocean>>,
    mut previous_ocean: Local<Option<bevy_aqua_core::Ocean>>,
) {
    let ocean = ocean.as_deref().copied();
    let ocean_changed = *previous_ocean != ocean;
    *previous_ocean = ocean;
    if waves.is_changed() || bodies.is_changed() || ocean_changed {
        epoch.0 = epoch.0.wrapping_add(1);
    }
}

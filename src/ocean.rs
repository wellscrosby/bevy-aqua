//! Ocean tile entities and camera following.

use bevy::{
    camera::{
        primitives::{Aabb, MeshAabb},
        visibility::{NoAutoAabb, NoFrustumCulling},
    },
    light::NotShadowCaster,
    mesh::MeshTag,
    prelude::*,
};

use bevy_aqua_core::cascade as lod;
use bevy_aqua_core::rings as geometry;
use bevy_aqua_core::{
    LOD_COUNT, Ocean, ResolvedWaterBodies, ResolvedWaterBody, TILE_RESOLUTION, ViewPos, WaterBody,
    WaterOptics, WaterShape, lod_scale,
};
use bevy_aqua_core::{OceanWaves, WaveModel};
use bevy_aqua_waves as waves;

#[derive(Component, Debug)]
pub(crate) struct Tile {
    relative: Vec2,
    base_bounds: Aabb,
    lod: usize,
    bounds_linear: Mat3A,
}

#[derive(Component, Debug)]
pub(crate) struct Root;

const TILE_NAMES: [&str; LOD_COUNT] = [
    "Ocean tile L0",
    "Ocean tile L1",
    "Ocean tile L2",
    "Ocean tile L3",
    "Ocean tile L4",
];

#[derive(Resource)]
pub(crate) struct OceanGeometry {
    meshes: [Handle<Mesh>; geometry::Patch::ALL.len()],
    bounds: [Aabb; geometry::Patch::ALL.len()],
    layouts: [Vec<geometry::Tile>; LOD_COUNT],
}

impl OceanGeometry {
    fn mesh(&self, patch: geometry::Patch) -> Handle<Mesh> {
        self.meshes[patch as usize].clone()
    }

    fn bounds(&self, patch: geometry::Patch) -> Aabb {
        self.bounds[patch as usize]
    }
}

pub(crate) fn init(mut commands: Commands, mut mesh_assets: ResMut<Assets<Mesh>>) {
    let built = geometry::Patch::ALL.map(geometry::build_patch);
    let bounds = built.each_ref().map(|mesh| {
        mesh.compute_aabb()
            .expect("ocean patches must have position bounds")
    });
    let meshes = built.map(|mesh| mesh_assets.add(mesh));
    let layouts = std::array::from_fn(geometry::tile_layout);
    commands.insert_resource(OceanGeometry {
        meshes,
        bounds,
        layouts,
    });
}

#[expect(
    clippy::type_complexity,
    reason = "on-demand hierarchy resolution and root mutation must be disjoint"
)]
pub(crate) fn sync(
    mut commands: Commands,
    ocean: Option<Res<Ocean>>,
    bodies: Query<(Entity, &WaterShape, Option<&WaterOptics>), With<WaterBody>>,
    mut transforms_and_roots: ParamSet<(
        TransformHelper,
        Query<(Entity, &mut Transform), With<Root>>,
    )>,
    resources: (
        Res<lod::Data>,
        Res<OceanGeometry>,
        Res<OceanWaves>,
        Res<waves::StartupAmplitude>,
    ),
) {
    let (data, geometry, settings, startup_amplitude) = resources;
    let needed = ocean.is_some() || {
        let transforms = transforms_and_roots.p0();
        bodies.iter().any(|(entity, shape, optics)| {
            transforms
                .compute_global_transform(entity)
                .ok()
                .and_then(|transform| {
                    ResolvedWaterBody::resolve(entity, shape, optics.copied(), &transform).ok()
                })
                .is_some()
        })
    };
    let level = ocean.as_deref().map_or(0.0, |ocean| ocean.level);
    if let Some((entity, mut transform)) = transforms_and_roots.p1().iter_mut().next() {
        if !needed {
            commands.entity(entity).despawn();
        } else if transform.translation.y != level {
            transform.translation.y = level;
        }
        return;
    }
    if !needed {
        return;
    }

    let material = data.material();
    let displacement = waves::displacement_bounds(&settings, data.layout(), startup_amplitude.0);
    commands
        .spawn((
            Name::new("Aqua water surface"),
            Root,
            Transform::from_xyz(0.0, level, 0.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            for (lod, (layouts, name)) in geometry.layouts.iter().zip(TILE_NAMES).enumerate() {
                let scale = lod_scale(lod);
                for layout in layouts {
                    let relative = layout.offset * scale;
                    let base_bounds = geometry.bounds(layout.patch);
                    parent.spawn((
                        Name::new(name),
                        Tile {
                            relative,
                            base_bounds,
                            lod,
                            bounds_linear: Mat3A::ZERO,
                        },
                        expanded_bounds(
                            base_bounds,
                            identity_parent_padding(lod, displacement[lod]),
                        ),
                        NoAutoAabb,
                        MeshTag(lod as u32),
                        Mesh3d(geometry.mesh(layout.patch)),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(relative.x, 0.0, relative.y)
                            .with_rotation(Quat::from_rotation_y(layout.rotation))
                            .with_scale(Vec3::new(scale, 1.0, scale)),
                        NotShadowCaster,
                    ));
                }
            }
        });
}

/// Removes a bounded-only root as soon as the canonical snapshot becomes
/// empty. Root creation and level mutation stay before transform propagation.
pub(crate) fn prune(
    mut commands: Commands,
    ocean: Option<Res<Ocean>>,
    bodies: Res<ResolvedWaterBodies>,
    roots: Query<Entity, With<Root>>,
) {
    if ocean.is_none() && bodies.0.is_empty() {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
}

// `snap_and_transition` can move a vertex by less than two grid cells for
// camera snapping and another 1.5 cells for geomorphing.
const SNAP_AND_MORPH_CELLS: f32 = 3.5;

fn expanded_bounds(base: Aabb, local_padding: Vec3A) -> Aabb {
    Aabb {
        center: base.center,
        half_extents: base.half_extents + local_padding,
    }
}

fn identity_parent_padding(lod: usize, displacement: waves::DisplacementBounds) -> Vec3A {
    let scale = lod_scale(lod);
    let snap_world = SNAP_AND_MORPH_CELLS * scale / TILE_RESOLUTION as f32;
    Vec3A::new(
        (snap_world + displacement.horizontal) / scale,
        displacement.vertical,
        (snap_world + displacement.horizontal) / scale,
    )
}

fn world_to_local_padding(transform: &GlobalTransform, world_padding: Vec3A) -> Option<Vec3A> {
    let linear = transform.affine().matrix3;
    let determinant = linear.determinant();
    if !linear.is_finite() || !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        return None;
    }
    let inverse = linear.inverse();
    let absolute_inverse = Mat3A::from_cols(
        inverse.x_axis.abs(),
        inverse.y_axis.abs(),
        inverse.z_axis.abs(),
    );
    Some(absolute_inverse * world_padding)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoundsKey {
    model: WaveModel,
}

#[derive(Default)]
pub(crate) struct BoundsCache {
    key: Option<BoundsKey>,
    displacement: [waves::DisplacementBounds; LOD_COUNT],
}

pub(crate) fn update_bounds(
    mut commands: Commands,
    settings: Res<OceanWaves>,
    startup_amplitude: Res<waves::StartupAmplitude>,
    data: Res<lod::Data>,
    mut cached: Local<BoundsCache>,
    mut tiles: Query<(
        Entity,
        &mut Tile,
        &GlobalTransform,
        &mut Aabb,
        Has<NoFrustumCulling>,
    )>,
) {
    let key = BoundsKey {
        model: settings.model,
    };
    let settings_changed = cached.key != Some(key);
    if settings_changed {
        cached.key = Some(key);
        cached.displacement =
            waves::displacement_bounds(&settings, data.layout(), startup_amplitude.0);
    }
    let displacement = cached.displacement;
    for (entity, mut tile, transform, mut bounds, no_frustum_culling) in &mut tiles {
        let linear = transform.affine().matrix3;
        if !settings_changed && tile.bounds_linear == linear {
            continue;
        }
        tile.bounds_linear = linear;
        let scale = lod_scale(tile.lod);
        let snap_world = SNAP_AND_MORPH_CELLS * scale / TILE_RESOLUTION as f32;
        let world_padding = Vec3A::new(
            snap_world + displacement[tile.lod].horizontal,
            displacement[tile.lod].vertical,
            snap_world + displacement[tile.lod].horizontal,
        );
        let Some(local_padding) = world_to_local_padding(transform, world_padding) else {
            if !no_frustum_culling {
                commands.entity(entity).insert(NoFrustumCulling);
            }
            continue;
        };
        if no_frustum_culling {
            commands.entity(entity).remove::<NoFrustumCulling>();
        }
        *bounds = expanded_bounds(tile.base_bounds, local_padding);
    }
}

/// Moves the coherent tile layout with the continuous Crest ocean centre.
pub(crate) fn follow(view: Res<ViewPos>, mut tiles: Query<(&Tile, &mut Transform)>) {
    if !view.is_changed() {
        return;
    }
    for (tile, mut transform) in &mut tiles {
        transform.translation.x = view.0.x + tile.relative.x;
        transform.translation.z = view.0.y + tile.relative.y;
    }
}

#[cfg(test)]
#[path = "ocean_tests.rs"]
mod tests;

//! ECS-authored bounded water and its canonical world-space snapshot.

use bevy::prelude::*;

#[cfg(test)]
use crate::RiverPath;
use crate::{FlowSample, WaterOptics, WaterShape};

/// Marks an entity as one bounded water surface.
///
/// Add a sibling [`WaterShape`] authored in entity-local XZ space. The
/// entity's [`GlobalTransform`] supplies world placement and surface level;
/// add [`WaterOptics`] for a per-body override.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[require(Transform)]
pub struct WaterBody;

/// Why a body transform cannot map to Aqua's horizontal scalar-width ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterBodyTransformError {
    /// Local XZ maps partly into world Y, creating a tilted surface.
    Tilted,
    /// The horizontal transform is singular or non-finite.
    Degenerate,
}

/// Canonical world snapshot consumed by every Aqua subsystem.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedWaterBody {
    pub entity: Entity,
    pub level: f32,
    pub optics: Option<WaterOptics>,
    pub shape: WaterShape,
    origin: Vec2,
    linear: Mat2,
    inverse: Mat2,
}

impl ResolvedWaterBody {
    /// Resolves one authored body. Aqua supports translation, parenting, yaw,
    /// reflection, nonuniform horizontal scale, and planar shear. Tilt cannot
    /// preserve a scalar-height water surface.
    pub fn resolve(
        entity: Entity,
        shape: &WaterShape,
        optics: Option<WaterOptics>,
        transform: &GlobalTransform,
    ) -> Result<Self, WaterBodyTransformError> {
        const EPSILON: f32 = 1.0e-4;
        let affine = transform.affine();
        let x = affine.matrix3.x_axis;
        let z = affine.matrix3.z_axis;
        let translation = affine.translation;
        if !x.is_finite() || !z.is_finite() || !translation.is_finite() {
            return Err(WaterBodyTransformError::Degenerate);
        }
        if x.y.abs() > EPSILON || z.y.abs() > EPSILON {
            return Err(WaterBodyTransformError::Tilted);
        }
        let world_x = Vec2::new(x.x, x.z);
        let world_z = Vec2::new(z.x, z.z);
        let linear = Mat2::from_cols(world_x, world_z);
        let determinant = linear.determinant();
        if !determinant.is_finite() || determinant.abs() <= EPSILON {
            return Err(WaterBodyTransformError::Degenerate);
        }
        Ok(Self {
            entity,
            level: translation.y,
            optics,
            shape: shape.clone(),
            origin: Vec2::new(translation.x, translation.z),
            linear,
            inverse: linear.inverse(),
        })
    }

    /// Maps a local body point to world XZ.
    pub fn world_point(&self, local: Vec2) -> Vec2 {
        self.origin + self.linear * local
    }

    /// Tests a world XZ point against the local authored shape.
    pub fn contains(&self, world: Vec2) -> bool {
        self.shape.contains(self.inverse * (world - self.origin))
    }

    /// Samples local river flow and rotates/reflects it into world XZ while
    /// preserving authored speed in metres per second.
    pub fn flow_at(&self, world: Vec2) -> Option<FlowSample> {
        let mut sample = self.shape.flow_at(self.inverse * (world - self.origin))?;
        if self.linear == Mat2::IDENTITY {
            return Some(sample);
        }
        let local_normal = Vec2::new(-sample.direction.y, sample.direction.x);
        let normal_scale = 1.0 / (self.inverse.transpose() * local_normal).length();
        sample.direction = (self.linear * sample.direction).normalize_or_zero();
        sample.flow = self.linear * sample.flow;
        sample.margin *= normal_scale;
        sample.half_width *= normal_scale;
        sample.speed = sample.flow.length();
        Some(sample)
    }

    /// Tight conservative world AABB plus a two-metre world-space field pad.
    pub fn aabb(&self) -> (Vec2, Vec2) {
        const PAD: f32 = 2.0;
        let mut minimum = Vec2::splat(f32::MAX);
        let mut maximum = Vec2::splat(f32::MIN);
        let mut include = |local: Vec2, local_radius: f32| {
            let center = self.world_point(local);
            let row_radius =
                Vec2::new(self.linear.row(0).length(), self.linear.row(1).length()) * local_radius;
            minimum = minimum.min(center - row_radius);
            maximum = maximum.max(center + row_radius);
        };
        match &self.shape {
            WaterShape::Circle { radius } => include(Vec2::ZERO, *radius),
            WaterShape::Polygon { points } => {
                for &point in points {
                    include(point, 0.0);
                }
            }
            WaterShape::River { path } => {
                for point in &path.points {
                    include(point.position, 0.5 * point.width);
                }
            }
            WaterShape::Corridor { path, width } => {
                for point in &path.points {
                    include(point.position, 0.5 * *width);
                }
            }
        }
        if minimum.x == f32::MAX {
            minimum = self.origin;
            maximum = self.origin;
        }
        (minimum - Vec2::splat(PAD), maximum + Vec2::splat(PAD))
    }

    /// Conservative world-space square extent used by the fixed body ABI.
    pub fn extent(&self) -> (Vec2, f32) {
        let (minimum, maximum) = self.aabb();
        let center = 0.5 * (minimum + maximum);
        (center, 0.5 * (maximum - minimum).max_element())
    }
}

/// Stable resolved body set shared by rendering, queries, reflections, spray,
/// and motion invalidation.
#[doc(hidden)]
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ResolvedWaterBodies(pub Vec<ResolvedWaterBody>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonuniform_circle_transform_resolves_as_an_ellipse() {
        let shape = WaterShape::Circle { radius: 1.0 };
        let transform = GlobalTransform::from(
            Transform::from_xyz(10.0, 3.0, -4.0).with_scale(Vec3::new(2.0, 1.0, 0.5)),
        );
        let resolved = ResolvedWaterBody::resolve(
            Entity::from_bits(1),
            &shape,
            Some(WaterOptics::CLEAR_FRESH),
            &transform,
        )
        .unwrap();
        assert_eq!(resolved.level, 3.0);
        assert!(resolved.contains(Vec2::new(11.9, -4.0)));
        assert!(!resolved.contains(Vec2::new(10.0, -4.6)));
        assert_eq!(resolved.optics, Some(WaterOptics::CLEAR_FRESH));
    }

    #[test]
    fn yaw_rotates_river_flow_without_losing_transform_scale() {
        let shape = WaterShape::River {
            path: RiverPath {
                points: vec![
                    crate::RiverPoint::new(Vec2::new(-5.0, 0.0), 4.0, 2.0),
                    crate::RiverPoint::new(Vec2::new(5.0, 0.0), 4.0, 2.0),
                ],
            },
        };
        let transform = GlobalTransform::from(
            Transform::from_rotation(Quat::from_rotation_y(0.5 * std::f32::consts::PI))
                .with_scale(Vec3::new(2.0, 1.0, 0.5)),
        );
        let resolved =
            ResolvedWaterBody::resolve(Entity::from_bits(2), &shape, None, &transform).unwrap();
        let flowed = resolved.flow_at(Vec2::ZERO).unwrap();
        assert!(flowed.flow.x.abs() < 1.0e-4);
        assert!((flowed.flow.y + 4.0).abs() < 1.0e-4);
        assert!((flowed.half_width - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn zero_speed_river_keeps_width_under_shear() {
        let shape = WaterShape::Corridor {
            path: RiverPath {
                points: vec![
                    crate::RiverPoint::new(Vec2::new(-2.0, 0.0), 4.0, 0.0),
                    crate::RiverPoint::new(Vec2::new(2.0, 0.0), 4.0, 0.0),
                ],
            },
            width: 4.0,
        };
        let shear = GlobalTransform::from(bevy::math::Affine3A::from_mat3_translation(
            Mat3::from_cols(Vec3::X, Vec3::Y, Vec3::new(1.0, 0.0, 1.0)),
            Vec3::ZERO,
        ));
        let resolved =
            ResolvedWaterBody::resolve(Entity::from_bits(5), &shape, None, &shear).unwrap();
        let flowed = resolved.flow_at(Vec2::ZERO).unwrap();
        assert_eq!(flowed.flow, Vec2::ZERO);
        assert!((flowed.half_width - 2.0).abs() < 1.0e-4);
        assert!((flowed.margin - 2.0).abs() < 1.0e-4);
    }

    #[test]
    fn tilted_and_singular_surfaces_are_rejected() {
        let shape = WaterShape::Circle { radius: 1.0 };
        let tilted = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_x(0.2)));
        assert_eq!(
            ResolvedWaterBody::resolve(Entity::from_bits(3), &shape, None, &tilted),
            Err(WaterBodyTransformError::Tilted)
        );
        let singular = GlobalTransform::from(Transform::from_scale(Vec3::new(0.0, 1.0, 1.0)));
        assert_eq!(
            ResolvedWaterBody::resolve(Entity::from_bits(4), &shape, None, &singular),
            Err(WaterBodyTransformError::Degenerate)
        );
    }
}

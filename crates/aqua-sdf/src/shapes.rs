use bevy_ecs::prelude::Component;
use glam::Vec2;

use crate::river::RiverPath;

/// Extent of a localized body; everything outside is discarded.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum WaterShape {
    /// Filled circle centred on the body's local origin, measured in metres.
    Circle { radius: f32 },
    /// Polygon outline in body-local XZ metres, using even-odd containment.
    Polygon {
        /// Closed outline vertices in local XZ (the last vertex implicitly
        /// closes onto the first).
        points: Vec<Vec2>,
    },
    /// Flowing channel along an authored path; per-point widths size it.
    River { path: RiverPath },
    /// Constant-width corridor along a path; the path's own widths are
    /// ignored for the extent and mask.
    Corridor {
        path: RiverPath,
        /// Full channel width in metres, constant along the run.
        width: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowSample {
    /// Unit centerline tangent in the sampled coordinate space.
    pub direction: Vec2,
    /// Current in m/s in the sampled coordinate space.
    pub flow: Vec2,
    /// `half_width - distance`: positive inside the channel, zero at the
    /// waterline.
    pub margin: f32,
    /// Channel half width at the projected point in metres.
    pub half_width: f32,
    /// Surface speed in m/s.
    pub speed: f32,
}

pub(super) fn point_in_polygon(points: &[Vec2], local_xz: Vec2) -> bool {
    let mut inside = false;
    let mut previous = points.last().copied().unwrap_or_default();
    for &current in points {
        let crosses = (previous.y > local_xz.y) != (current.y > local_xz.y);
        if crosses {
            let intersect_x = (current.x - previous.x) * (local_xz.y - previous.y)
                / (current.y - previous.y)
                + previous.x;
            if local_xz.x < intersect_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

impl WaterShape {
    /// Flow-field sample under one local position. Signed margins go
    /// negative outside the channel; containment is [`Self::contains`].
    pub fn flow_at(&self, local_xz: Vec2) -> Option<FlowSample> {
        let sampled = self.river_path()?.sample(local_xz)?;
        let half_width = match self {
            WaterShape::Corridor { width, .. } => width * 0.5,
            _ => sampled.half_width,
        };
        Some(FlowSample {
            direction: sampled.direction,
            flow: sampled.flow,
            margin: half_width - sampled.distance,
            half_width,
            speed: sampled.speed,
        })
    }

    pub(crate) fn river_path(&self) -> Option<&RiverPath> {
        match self {
            WaterShape::River { path } | WaterShape::Corridor { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn contains(&self, local_xz: Vec2) -> bool {
        match self {
            WaterShape::Circle { radius } => local_xz.length() <= *radius,
            WaterShape::Polygon { points } => point_in_polygon(points, local_xz),
            // `sample` projects onto the nearest segment for ANY point, so
            // containment needs the signed bank margin, not just Some.
            WaterShape::River { .. } => {
                matches!(self.flow_at(local_xz), Some(sampled) if sampled.margin >= 0.0)
            }
            WaterShape::Corridor { path, width } => {
                let half_width = width * 0.5;
                matches!(path.sample(local_xz), Some(sampled) if sampled.distance <= half_width)
            }
        }
    }
}

// Include the widest bank plus two metres for the field-texture margin.
fn points_bounds_river(path: &RiverPath) -> (Vec2, Vec2) {
    const MARGIN: f32 = 2.0;
    if path.points.is_empty() {
        return (Vec2::ZERO, Vec2::ZERO);
    }
    let mut minimum = Vec2::splat(f32::MAX);
    let mut maximum = Vec2::splat(f32::MIN);
    let mut max_half_width = 0.0_f32;
    for point in &path.points {
        minimum = minimum.min(point.position);
        maximum = maximum.max(point.position);
        max_half_width = max_half_width.max(point.width * 0.5);
    }
    let inflate = max_half_width + MARGIN;
    (
        minimum - Vec2::splat(inflate),
        maximum + Vec2::splat(inflate),
    )
}

fn points_bounds(points: &[Vec2]) -> (Vec2, Vec2) {
    let mut minimum = Vec2::splat(f32::MAX);
    let mut maximum = Vec2::splat(f32::MIN);
    for point in points {
        minimum = minimum.min(*point);
        maximum = maximum.max(*point);
    }
    (minimum, maximum)
}

pub(super) fn resolve_extent(shape: &WaterShape) -> (Vec2, f32, Vec2, Vec2) {
    if let WaterShape::Polygon { points } = shape
        && !points.is_empty()
    {
        const MARGIN: f32 = 2.0;
        let (minimum, maximum) = points_bounds(points);
        let minimum = minimum - Vec2::splat(MARGIN);
        let maximum = maximum + Vec2::splat(MARGIN);
        let size = maximum - minimum;
        let center = (minimum + maximum) * 0.5;
        let half = size.max_element() * 0.5;
        return (center, half, minimum, Vec2::splat(half * 2.0));
    }
    match shape {
        WaterShape::Circle { radius } => (
            Vec2::ZERO,
            *radius,
            Vec2::splat(-*radius),
            Vec2::splat(2.0 * radius),
        ),
        WaterShape::Polygon { .. } => {
            // Empty polygons fall back to a degenerate extent at origin.
            (Vec2::ZERO, 1.0, Vec2::splat(-1.0), Vec2::splat(2.0))
        }
        WaterShape::River { path } => {
            let (minimum, maximum) = points_bounds_river(path);
            finish_corridor_extent(minimum, maximum)
        }
        WaterShape::Corridor { path, width } => {
            const MARGIN: f32 = 2.0;
            let inflate = width * 0.5 + MARGIN;
            let mut minimum = Vec2::splat(f32::MAX);
            let mut maximum = Vec2::splat(f32::MIN);
            for point in &path.points {
                minimum = minimum.min(point.position);
                maximum = maximum.max(point.position);
            }
            minimum -= Vec2::splat(inflate);
            maximum += Vec2::splat(inflate);
            finish_corridor_extent(minimum, maximum)
        }
    }
}

fn finish_corridor_extent(minimum: Vec2, maximum: Vec2) -> (Vec2, f32, Vec2, Vec2) {
    let size = maximum - minimum;
    let center = (minimum + maximum) * 0.5;
    let half = size.max_element() * 0.5;
    (center, half, minimum, Vec2::splat(half * 2.0))
}

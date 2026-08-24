use glam::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiverPoint {
    /// Body-local XZ position in metres.
    pub position: Vec2,
    /// Full channel width at this point in metres.
    pub width: f32,
    /// Surface current speed along the centerline in m/s.
    pub speed: f32,
}

impl RiverPoint {
    pub const fn new(position: Vec2, width: f32, speed: f32) -> Self {
        Self {
            position,
            width,
            speed,
        }
    }
}

/// Authored river centerline: polyline with per-point width and speed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RiverPath {
    /// Control points in order along the channel.
    pub points: Vec<RiverPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiverSample {
    /// Unit centerline tangent, retained even when speed is zero.
    pub direction: Vec2,
    /// Flow vector in m/s (direction times speed).
    pub flow: Vec2,
    /// Channel half width at the projected point in metres.
    pub half_width: f32,
    /// Distance to the centerline in metres.
    pub distance: f32,
    /// Centerline speed at the projected point in m/s.
    pub speed: f32,
}

impl RiverSample {
    pub fn within_bank(&self) -> bool {
        self.distance <= self.half_width
    }
}

impl RiverPath {
    /// Returns `None` only for paths with fewer than two points or
    /// degenerate segments everywhere; sampling far outside the channel
    /// still returns the nearest-centerline sample so callers can fade with
    /// distance.
    pub fn sample(&self, xz: Vec2) -> Option<RiverSample> {
        let mut best: Option<(f32, RiverSample)> = None;
        for pair in self.points.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            let segment = b.position - a.position;
            let length_squared = segment.length_squared();
            if length_squared <= 1e-10 {
                continue;
            }
            let t = ((xz - a.position).dot(segment) / length_squared).clamp(0.0, 1.0);
            let projected = a.position + segment * t;
            let delta = xz - projected;
            let distance = delta.length();
            let direction = segment.normalize();
            let candidate = RiverSample {
                direction,
                flow: direction * (a.speed + (b.speed - a.speed) * t),
                half_width: (a.width + (b.width - a.width) * t) * 0.5,
                distance,
                speed: a.speed + (b.speed - a.speed) * t,
            };
            let better = match &best {
                Some((best_distance, _)) => distance < *best_distance,
                None => true,
            };
            if better {
                best = Some((distance, candidate));
            }
        }
        best.map(|(_, nearest)| nearest)
    }
}

#[cfg(test)]
#[path = "river_tests.rs"]
mod tests;

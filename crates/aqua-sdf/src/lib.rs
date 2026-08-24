#![warn(unreachable_pub)]

//! Signed-shape and river-flow math for Aqua's global water fields.
//!
//! [`WaterShape`] provides containment, current, and render extents in the
//! body-local XZ plane. [`RiverPath`] projects points onto authored river
//! centerlines.

mod river;
mod shapes;

pub use river::{RiverPath, RiverPoint, RiverSample};
pub use shapes::{FlowSample, WaterShape};

/// Even-odd containment test against a closed polygon outline.
pub fn point_in_polygon(points: &[glam::Vec2], local_xz: glam::Vec2) -> bool {
    shapes::point_in_polygon(points, local_xz)
}

/// Bounding-square centre/half-extent of a body's renderable area plus the
/// local-XZ AABB min/size used to map the baked field textures.
pub fn resolve_extent(shape: &WaterShape) -> (glam::Vec2, f32, glam::Vec2, glam::Vec2) {
    shapes::resolve_extent(shape)
}

#[cfg(test)]
mod tests;

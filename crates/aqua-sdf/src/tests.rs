use super::shapes::{point_in_polygon, resolve_extent};
use super::*;
use glam::Vec2;

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
}

#[test]
fn point_in_polygon_even_odd_containment() {
    let square = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 10.0),
    ];
    assert!(point_in_polygon(&square, Vec2::new(5.0, 5.0)));
    assert!(point_in_polygon(&square, Vec2::new(0.5, 9.5)));
    assert!(!point_in_polygon(&square, Vec2::new(-1.0, 5.0)));
    assert!(!point_in_polygon(&square, Vec2::new(10.5, 10.5)));
    // Concave L shape: the notch is outside.
    let notch = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 4.0),
        Vec2::new(4.0, 4.0),
        Vec2::new(4.0, 10.0),
        Vec2::new(0.0, 10.0),
    ];
    assert!(point_in_polygon(&notch, Vec2::new(2.0, 8.0)));
    assert!(!point_in_polygon(&notch, Vec2::new(7.0, 8.0)));
}

#[test]
fn polygon_extent_fits_the_point_aabb_with_margin() {
    let shape = WaterShape::Polygon {
        points: vec![
            Vec2::new(-20.0, -10.0),
            Vec2::new(30.0, -10.0),
            Vec2::new(10.0, 25.0),
        ],
    };
    // Half of the longest side; resolve_extent adds the margin below.
    let (center, half, aabb_min, aabb_size) = resolve_extent(&shape);
    assert_eq!(center, Vec2::new(5.0, 7.5));
    assert!((half - 27.0).abs() < 1e-4);
    assert!((aabb_min.x - -22.0).abs() < 1e-4);
    assert_eq!(aabb_size, Vec2::splat(54.0));
}

#[test]
fn resolve_extent_fits_the_river_corridor_with_margin() {
    let shape = WaterShape::River {
        path: RiverPath {
            points: vec![
                RiverPoint::new(Vec2::new(0.0, 0.0), 10.0, 2.0),
                RiverPoint::new(Vec2::new(100.0, 30.0), 14.0, 2.0),
            ],
        },
    };
    let (center, half, aabb_min, aabb_size) = resolve_extent(&shape);
    // Domain spans x 0-100, y 0-30 plus half width (7) + margin (2).
    assert_close(aabb_min.x, -9.0);
    assert_close(aabb_min.y, -9.0);
    // Square texture domain covering the longer side.
    assert!((aabb_size.x - 118.0).abs() < 1e-4);
    assert!((aabb_size.y - 118.0).abs() < 1e-4);
    assert!((half - 59.0).abs() < 1e-4);
    assert_close(center.x, 50.0);
    assert_close(center.y, 15.0);
}

#[test]
fn corridor_extent_uses_the_constant_width_and_flow_matches() {
    let path = RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(0.0, 0.0), 99.0, 99.0),
            RiverPoint::new(Vec2::new(100.0, 0.0), 99.0, 99.0),
        ],
    };
    let shape = WaterShape::Corridor { path, width: 12.0 };
    let (center, half, aabb_min, _aabb_size) = resolve_extent(&shape);
    assert_close(center.x, 50.0);
    assert_close(center.y, 0.0);
    // Half width (6) + margin (2) inflate the straight run.
    assert_close(aabb_min.x, -8.0);
    assert_close(aabb_min.y, -8.0);
    assert!((half - 58.0).abs() < 1e-4);
    // Flow rides the path but banks come from the corridor width.
    let mid = shape.flow_at(Vec2::new(50.0, 5.5)).expect("mid channel");
    assert_close(mid.margin, 0.5);
    assert!((mid.half_width - 6.0).abs() < 1e-4);
    assert!(shape.contains(Vec2::new(50.0, 5.5)));
    assert!(!shape.contains(Vec2::new(50.0, 7.5)));
    // The wide per-point widths never leak into containment.
    assert!(!shape.contains(Vec2::new(50.0, 20.0)));
}

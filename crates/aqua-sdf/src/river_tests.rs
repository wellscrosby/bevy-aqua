use super::*;

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
}

fn straight() -> RiverPath {
    RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(0.0, 0.0), 10.0, 2.0),
            RiverPoint::new(Vec2::new(0.0, 100.0), 10.0, 2.0),
        ],
    }
}

#[test]
fn straight_river_flows_along_its_centerline() {
    let path = straight();
    let sample = path.sample(Vec2::new(3.0, 50.0)).expect("non-empty path");
    assert_close(sample.flow.x, 0.0);
    assert_close(sample.flow.y, 2.0);
    assert_close(sample.distance, 3.0);
    assert_close(sample.half_width, 5.0);
    assert!(sample.within_bank());
}

#[test]
fn outside_the_bank_still_samples_with_distance() {
    let path = straight();
    let sample = path.sample(Vec2::new(9.0, 40.0)).expect("non-empty path");
    assert!(!sample.within_bank());
    assert_close(sample.distance, 9.0);
    assert_close(sample.flow.y, 2.0);
}

#[test]
fn width_and_speed_interpolate_between_points() {
    let path = RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(0.0, 0.0), 8.0, 1.0),
            RiverPoint::new(Vec2::new(0.0, 100.0), 12.0, 3.0),
        ],
    };
    let at_start = path.sample(Vec2::ZERO).expect("non-empty path");
    assert_close(at_start.half_width, 4.0);
    assert_close(at_start.speed, 1.0);
    let mid = path.sample(Vec2::new(0.0, 50.0)).expect("non-empty path");
    assert_close(mid.half_width, 5.0);
    assert_close(mid.speed, 2.0);
}

#[test]
fn nearest_segment_wins_on_bends() {
    let path = RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(-50.0, 0.0), 10.0, 1.0),
            RiverPoint::new(Vec2::new(0.0, 0.0), 10.0, 1.0),
            RiverPoint::new(Vec2::new(0.0, 50.0), 10.0, 4.0),
        ],
    };
    // Near the second segment's start, not the first segment's end line.
    let at_bend = path.sample(Vec2::new(4.0, 6.0)).expect("non-empty path");
    // Projected at t=0.12 along the second segment: speed 1 + 3*0.12.
    assert_close(at_bend.speed, 1.36);
    assert!(at_bend.flow.x.abs() < 1e-4);
}

#[test]
fn empty_and_degenerate_paths_return_none() {
    assert!(RiverPath::default().sample(Vec2::ZERO).is_none());
    let degenerate = RiverPath {
        points: vec![
            RiverPoint::new(Vec2::new(1.0, 1.0), 10.0, 1.0),
            RiverPoint::new(Vec2::new(1.0, 1.0), 10.0, 1.0),
        ],
    };
    assert!(degenerate.sample(Vec2::ZERO).is_none());
}

use bevy::prelude::*;
use bevy_aqua_core::{AquaSettings, Ocean, ResolvedWaterBody, WaterOptics, WaterShape};

use super::sample_medium;

fn circle_body(
    level: f32,
    center: Vec2,
    radius: f32,
    optics: Option<WaterOptics>,
) -> ResolvedWaterBody {
    ResolvedWaterBody::resolve(
        Entity::from_bits(1),
        &WaterShape::Circle { radius },
        optics,
        &GlobalTransform::from(Transform::from_xyz(center.x, level, center.y)),
    )
    .expect("test body must resolve")
}

#[test]
fn ocean_camera_switches_on_mean_plane() {
    let ocean = Ocean { level: -2.0 };
    let settings = AquaSettings {
        water_optics: WaterOptics::DEEP_OCEAN,
        ..default()
    };
    assert!(sample_medium(Vec3::new(0.0, 0.0, 0.0), Some(&ocean), &settings, &[], 0.0).is_none());
    let under =
        sample_medium(Vec3::new(4.0, -3.0, 1.0), Some(&ocean), &settings, &[], 0.0).unwrap();
    assert_eq!(under.0, -2.0);
    assert_eq!(under.1, WaterOptics::DEEP_OCEAN);
    assert!(under.2);
}

#[test]
fn ocean_camera_follows_local_wave_height() {
    let ocean = Ocean { level: 0.0 };
    let settings = AquaSettings::default();
    let under_crest = sample_medium(Vec3::new(0.0, 1.0, 0.0), Some(&ocean), &settings, &[], 2.0);
    assert!(under_crest.is_some());

    let in_trough = sample_medium(
        Vec3::new(0.0, -1.0, 0.0),
        Some(&ocean),
        &settings,
        &[],
        -2.0,
    );
    assert!(in_trough.is_none());
}

#[test]
fn pond_wins_over_ocean_when_the_camera_is_inside() {
    let ocean = Ocean { level: -2.0 };
    let settings = AquaSettings {
        water_optics: WaterOptics::DEEP_OCEAN,
        ..default()
    };
    let pond = circle_body(
        3.0,
        Vec2::new(10.0, 0.0),
        4.0,
        Some(WaterOptics::CLEAR_FRESH),
    );
    let inside = sample_medium(
        Vec3::new(10.0, 1.0, 0.0),
        Some(&ocean),
        &settings,
        &[pond.clone()],
        0.0,
    )
    .unwrap();
    assert_eq!(inside.0, 3.0);
    assert_eq!(inside.1, WaterOptics::CLEAR_FRESH);
    assert!(!inside.2);

    let beside = sample_medium(
        Vec3::new(0.0, 1.0, 0.0),
        Some(&ocean),
        &settings,
        &[pond],
        0.0,
    );
    assert!(beside.is_none());
}

#[test]
fn camera_below_ocean_outside_a_raised_pond_uses_the_ocean() {
    let ocean = Ocean { level: 0.0 };
    let settings = AquaSettings::default();
    let pond = circle_body(8.0, Vec2::ZERO, 3.0, Some(WaterOptics::CLEAR_FRESH));
    let under = sample_medium(
        Vec3::new(20.0, -1.0, 0.0),
        Some(&ocean),
        &settings,
        &[pond],
        0.0,
    )
    .unwrap();
    assert_eq!(under.0, 0.0);
    assert_eq!(under.1, WaterOptics::DEEP_OCEAN);
    assert!(under.2);
}

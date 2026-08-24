use super::*;

#[test]
fn body_requires_transform_and_resolves_from_global_space() {
    let mut app = App::new();
    app.init_resource::<ResolvedWaterBodies>()
        .add_plugins(bevy::transform::TransformPlugin)
        .add_systems(
            PostUpdate,
            resolve_bodies.after(TransformSystems::Propagate),
        );

    let parent = app
        .world_mut()
        .spawn(Transform::from_xyz(10.0, 1.0, -5.0))
        .id();
    let body = app
        .world_mut()
        .spawn((
            WaterBody,
            WaterShape::Circle { radius: 4.0 },
            WaterOptics::CLEAR_FRESH,
            Transform::from_xyz(2.0, 2.0, -2.0),
            ChildOf(parent),
        ))
        .id();
    let optics_only = app.world_mut().spawn(WaterOptics::COASTAL).id();
    app.update();

    assert!(app.world().get::<Transform>(body).is_some());
    assert!(app.world().get::<GlobalTransform>(body).is_some());
    assert!(app.world().get::<WaterBody>(optics_only).is_none());
    let resolved = &app.world().resource::<ResolvedWaterBodies>().0;
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].entity, body);
    assert_eq!(resolved[0].level, 3.0);
    assert_eq!(resolved[0].optics, Some(WaterOptics::CLEAR_FRESH));
    assert!(resolved[0].contains(Vec2::new(12.0, -7.0)));
    assert!(!resolved[0].contains(Vec2::new(17.0, -7.0)));

    app.world_mut()
        .get_mut::<Transform>(parent)
        .unwrap()
        .translation
        .x = 20.0;
    app.update();
    assert!(app.world().resource::<ResolvedWaterBodies>().0[0].contains(Vec2::new(22.0, -7.0)));

    app.world_mut().entity_mut(body).remove::<WaterOptics>();
    app.update();
    assert_eq!(
        app.world().resource::<ResolvedWaterBodies>().0[0].optics,
        None
    );

    app.world_mut().entity_mut(body).remove::<WaterShape>();
    app.update();
    assert!(app.world().resource::<ResolvedWaterBodies>().0.is_empty());
    assert!(app.world().get::<WaterBody>(body).is_some());

    app.world_mut().despawn(body);
    app.update();
    assert!(app.world().resource::<ResolvedWaterBodies>().0.is_empty());
}

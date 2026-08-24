use super::*;

#[test]
fn view_position_uses_camera_rig_world_transform() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<AquaDebug>()
        .init_resource::<AquaSettings>()
        .init_resource::<OceanWaves>()
        .init_resource::<ViewPos>()
        .init_resource::<ViewDetail>()
        .init_resource::<ViewSeaLevel>()
        .init_resource::<ViewOrder>()
        .insert_resource(Ocean { level: 6.5 })
        .add_systems(Update, update_view);
    let rig = app
        .world_mut()
        .spawn(Transform::from_xyz(100.0, 0.0, -40.0))
        .id();
    app.world_mut().spawn((
        Camera3d::default(),
        Transform::from_xyz(7.0, 3.0, 11.0),
        ChildOf(rig),
    ));

    app.update();

    assert_eq!(app.world().resource::<ViewPos>().0, Vec2::new(107.0, -29.0));
    assert_eq!(app.world().resource::<ViewSeaLevel>().0, 6.5);
}

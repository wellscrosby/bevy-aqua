//! Runs a pond and river fields bake without waves or foam.
//!
//! ```none
//! cargo run -p aqua-shore --example shore_ponds
//! ```

use aqua_core::{Ocean, WaterBody, WaterShape};
use aqua_sdf::{RiverPath, RiverPoint};
use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    // AssetPlugin first: WaterFields seeds its handles from Assets<Image>.
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::log::LogPlugin::default(),
    ));
    // Headless stand-ins for the render/asset plugins the full app gets.
    app.init_resource::<Assets<Image>>();
    app.init_asset::<bevy::shader::Shader>();
    app.register_asset_loader(bevy::shader::ShaderLoader);
    app.init_resource::<Assets<aqua_core::CascadeMaterial>>();
    app.init_resource::<aqua_core::WaterFields>();
    app.add_plugins(aqua_shore::AquaShorePlugin);
    app.add_systems(PreStartup, assemble_data);
    app.add_systems(Startup, spawn_bodies);

    app.update();
    app.update();

    let fields = app.world().resource::<aqua_core::WaterFields>();
    assert_eq!(fields.params.meta.x, 2.0, "two bounded bodies baked");
    assert_eq!(
        fields.params.meta.y, 1.0,
        "the Ocean resource enabled the unbounded plane"
    );
    println!("shore isolation OK: pond + river baked into the fields");
}

fn assemble_data(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Stand-in for the umbrella's cascade assembly.
    let cascades = aqua_core::cascade::layout(Vec2::ZERO);
    commands.insert_resource(aqua_core::Data::new(
        Handle::default(),
        images.add(aqua_core::cascade::make_texture()),
        images.add(aqua_core::cascade::make_fft_surface_texture()),
        aqua_core::GpuLayout::new(&cascades, Vec2::ZERO, 0.0),
    ));
}

fn spawn_bodies(mut commands: Commands) {
    commands.insert_resource(Ocean::default());
    commands.spawn((
        WaterBody,
        WaterShape::Circle { radius: 19.0 },
        Transform::from_xyz(45.0, 3.0, 30.0),
    ));
    commands.spawn((
        WaterBody,
        WaterShape::River {
            path: RiverPath {
                points: vec![
                    RiverPoint::new(Vec2::new(-50.0, 0.0), 10.0, 1.0),
                    RiverPoint::new(Vec2::new(50.0, 0.0), 10.0, 3.0),
                ],
            },
        },
        Transform::default(),
    ));
}

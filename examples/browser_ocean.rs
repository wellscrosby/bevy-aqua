//! Small visual WebGPU demo: analytic ocean waves and a reflected buoy.
//!
//! Run in a browser with:
//! ```none
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-server-runner \
//! cargo run --target wasm32-unknown-unknown --example browser_ocean
//! ```

use bevy::{
    core_pipeline::prepass::DepthPrepass,
    prelude::*,
    render::{
        RenderPlugin,
        settings::{PowerPreference, WgpuSettings},
    },
};
use bevy_aqua::{
    AquaPlugin, AquaSettings, Ocean, OceanWaves, ReflectedInWater, ReflectionMode, WaveModel,
    WaveQuery, WaveSurface,
};

// Places the asset's -0.81 m keel about 0.6 m below the mean water plane.
const BUOY_MEAN_HEIGHT: f32 = 0.2;
// A 4 s^-1 exponential response damps one-frame GPU readback changes.
const BUOY_FOLLOW_RATE: f32 = 4.0;

#[derive(Component)]
struct Buoy {
    mean_height: f32,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.48, 0.72, 0.92)))
        .insert_resource(Ocean::default())
        .insert_resource(OceanWaves {
            model: WaveModel::Analytic,
            ..default()
        })
        .insert_resource(AquaSettings {
            reflections: ReflectionMode::Planar {
                scale: 0.25,
                distortion: 0.02,
            },
            caustics: None,
            ..default()
        })
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "bevy-aqua WebGPU ocean".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        power_preference: PowerPreference::HighPerformance,
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
            AquaPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, follow_surface)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        DepthPrepass,
        Transform::from_xyz(18.0, 9.0, 22.0).looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.85, -0.55, 0.0)),
    ));

    commands
        .spawn((
            Buoy {
                mean_height: BUOY_MEAN_HEIGHT,
            },
            WaveQuery,
            ReflectedInWater,
            Visibility::default(),
            Transform::from_xyz(0.0, BUOY_MEAN_HEIGHT, 0.0),
        ))
        .with_child(WorldAssetRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("examples/ocean_buoy.glb"),
        )));
}

fn follow_surface(time: Res<Time>, mut buoy: Query<(&Buoy, &WaveSurface, &mut Transform)>) {
    let response = 1.0 - (-BUOY_FOLLOW_RATE * time.delta_secs()).exp();
    for (buoy, surface, mut transform) in &mut buoy {
        if !surface.valid {
            continue;
        }
        let target_height = buoy.mean_height + surface.displacement.y;
        transform.translation.y = transform.translation.y.lerp(target_height, response);
        let target_rotation = Quat::from_rotation_arc(Vec3::Y, surface.normal);
        transform.rotation = transform.rotation.slerp(target_rotation, response);
    }
}

//! Minimal setup for adding water-surface sampling to a buoy.
//!
//! A GPU device is required for readback; this example only checks that
//! [`WaveSurface`] is inserted with [`WaveQuery`].
//!
//! ```none
//! cargo run -p aqua-query --example query_buoy
//! ```

use aqua_query::{WaveQuery, WaveSurface};
use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.init_resource::<Assets<bevy::render::storage::ShaderBuffer>>();
    app.add_plugins(aqua_query::AquaQueryPlugin);
    app.add_systems(Startup, spawn_buoy);
    app.update();

    let mut q = app.world_mut().query::<(&WaveQuery, &WaveSurface)>();
    let count = q.iter(app.world()).count();
    assert_eq!(count, 1, "buoy carries both components");
    println!("query isolation OK: buoy registered for GPU sampling");
}

fn spawn_buoy(mut commands: Commands) {
    commands.spawn((WaveQuery, Transform::from_xyz(4.0, 0.0, -2.0)));
}

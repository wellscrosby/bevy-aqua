# bevy-aqua

[![crates.io](https://img.shields.io/crates/v/bevy-aqua.svg)](https://crates.io/crates/bevy-aqua)

Camera-centred ocean rendering for Bevy 0.19 with analytic and FFT waves,
depth-aware transmission, reflections, persistent foam, localized water
bodies, and GPU surface queries.

![FFT ocean at sunset with planar buoy reflection](docs/images/sunset-fft.jpg)

## Features

- Five camera-centred displacement cascades with smooth LOD blending.
- Crest-style analytic waves or Tessendorf spectral waves.
- Beer-Lambert transmission, refraction, reflections, scene lighting, and a fullscreen underwater volume composite.
- Persistent whitecaps and shoreline foam.
- Static terrain heightfields for shoaling and shallow-water optics.
- Bounded ponds, lakes, and river corridors with per-body optics.
- GPU `WaveQuery` samples for buoyancy and gameplay.
- Optional budgeted Hanabi spray.

## Highlights

| FFT open ocean | Coastal foam and shallow-water optics |
|---|---|
| ![Close FFT wave detail](docs/images/fft-open-ocean.jpg) | ![Foam and coastal transmission at an island shore](docs/images/coastal-foam.jpg) |

| Bounded river corridor | Planar reflection |
|---|---|
| ![Localized river water following a curved corridor](docs/images/bounded-river.jpg) | ![Planar reflection of a buoy in calm water](docs/images/planar-reflection.jpg) |

## Compatibility

| bevy-aqua | Bevy | Rust | Verified target |
|---|---|---|---|
| 0.1 | 0.19 | 1.95+ | Desktop Vulkan and in browser WebGPU |

The default `query` feature re-exports `WaveQuery` and `WaveSurface` from the
facade. GPU probes always run because the underwater composite follows local
wave height at the camera. The default `reflect` feature enables planar
reflections. The optional `spray` feature adds `bevy-aqua-spray` and `bevy_hanabi`,
implies `query`, and defaults to `Off` at runtime. Web, mobile, and other
desktop APIs are not yet verified.

## Quick start

```rust
use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*};
use bevy_aqua::{AquaPlugin, Ocean};

fn main() {
    App::new()
        .insert_resource(Ocean::default())
        .add_plugins((DefaultPlugins, AquaPlugin))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((
                Camera3d::default(),
                DepthPrepass,
                Transform::from_xyz(24.0, 12.0, 32.0)
                    .looking_at(Vec3::ZERO, Vec3::Y),
            ));
            commands.spawn((
                DirectionalLight::default(),
                Transform::from_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    -0.8,
                    -0.6,
                    0.0,
                )),
            ));
        })
        .run();
}
```

Insert one `Ocean` resource. `Ocean::level` sets the global sea level. Remove
the resource for bounded-water-only worlds. One active `Camera3d` is the
supported view path.

## Configuration

Insert `OceanWaves` and `AquaSettings` before `AquaPlugin` to replace their
defaults.

`OceanWaves` selects `WaveModel::Analytic` or `WaveModel::Spectral`. It also
sets sea state, shallow-water attenuation, wind direction and speed, fetch,
and world-XZ flow. `sea_state`, wind, and fetch determine startup spectrum
data; set them before the plugin starts.

`AquaSettings` selects a `WaterOptics` preset and a `detail_strength` in
`0..=2`. `WaterOptics::DEEP_OCEAN` is the default. Coastal, tropical, and
clear-fresh presets are also provided. When the camera is below the local wave
surface, Aqua composites the same extinction through a fullscreen volume pass
and shades the mesh underside as a water-to-air window. Fog only follows the
underwater segment of each view ray. Crossing the surface is a hard cut.

`far_tier_start` and `far_tier_end`
bound the reduced-cost shading transition in metres. Far shading keeps sun
and reflections while omitting depth, foam, and sampled subsurface detail.
`reflections` selects the default planar mirror views or the byte-compatible cubemap-only path. Mark terrain or a
scene root with `ReflectedInWater` to include it and its descendants in planar
views. `caustics` controls the default procedural shallow-bed lighting; set it
to `None` to skip both texture samples. Hosts can update
`CausticsSunVisibility` to fold cloud-shadow coverage into the direct sun.

### Terrain bed

Insert a `BedHeightMap` before `AquaPlugin`. Its single-channel image stores
normalized height. `origin` is the world-XZ centre of texel `(0, 0)`, `size`
is the distance from the first to last texel centres, and `height_range`
decodes normalized values to metres.

```rust,ignore
commands.insert_resource(BedHeightMap {
    image: terrain_heightmap,
    origin: Vec2::splat(-10_000.0),
    size: Vec2::splat(20_000.0),
    height_range: [0.0, 1_808.0],
});
```

Without a bed map, Aqua uses deep-water attenuation everywhere.

### Localized water and queries

`WaterBody` marks a bounded surface. Add a sibling `WaterShape` for circles,
polygons, corridors, or rivers. Shape coordinates are local to the entity. Its
propagated `Transform` supplies world
XZ placement and surface Y, so parenting, yaw, reflection, nonuniform scale,
and planar shear work naturally. Tilted water is rejected because the renderer
uses one horizontal level per body. Add `WaterOptics` as a sibling component to
override the global ocean optics.

```rust,ignore
commands.spawn((
    WaterBody,
    WaterShape::Circle { radius: 24.0 },
    WaterOptics::CLEAR_FRESH,
    Transform::from_xyz(40.0, 3.0, -20.0),
));
```

Keep bodies inside the bed-map region when they need shoreline foam or
shallow-water attenuation. Moving a body or a transformed ancestor rebuilds
the shared shoreline fields, so body transforms are intended to change
infrequently.

With the `query` feature, add `WaveQuery` to an entity whose
transform sits on the owning surface's mean plane. Aqua refreshes its
`WaveSurface` with relative displacement. `WaveSurface::valid` is false when
no ocean or bounded body owns the point. GPU samples arrive with about one
frame of readback latency. Rivers use the matching analytic path; other bounded
shapes remain flat, matching their rendered geometry. The per-frame limit is
256 probes. `WaveSurface::crest` exposes the same
horizontal-compression source used to seed persistent whitecaps.

### Spray

Enable Cargo feature `spray`, then insert `SpraySettings` before `AquaPlugin`.
`SprayQuality::Off` does no wave-query or particle work. Low and High use fixed
probe, emitter, particle-rate, distance, and projected-screen-coverage budgets.
They reuse `WaveSurface::crest` and bed depth rather than adding a spray fluid
simulation.

## Repository showcase

From a repository checkout:

```sh
cargo run --release --example showcase
cargo run --release --example showcase -- --wave-backend fft --sea-state moderate
cargo run --release --example showcase -- --scene river
cargo run --release --example showcase -- \
  --headless --time 12 --screenshot /tmp/aqua-island.png
cargo run --release --features spray --example showcase -- \
  --scene island --near-shore --sea-state rough --spray high
```

Scene recipes provide useful defaults. Explicit CLI values override them:

| `--scene` | Purpose and built-in recipe |
|---|---|
| `island` | Default terrain and ocean |
| `lake` | Calm water, 1.2 m/s current, 25 degree wind |
| `reflection-lake` | Calm planar-reflection view with a large buoy (local test asset required) |
| `ponds` / `ponds-many` | Two presentation ponds or ten profiling ponds, using clear-fresh optics |
| `river` | Clear-fresh river and basin, calm water, 20 degree wind |
| `anim-waves` | Open-ocean flight with optional buoy and reflection-probe views |

`--profile-pose` selects its matching scene as well as its camera. Non-default
values under the `Open-ocean scene` help heading require `--scene anim-waves`
or an open-ocean profile pose. Run the showcase with `--help` for grouped
presentation, water, diagnostic, capture, and profiling flags.

## AI disclosure

This project was developed with assistance from AI coding agents.

## Attribution

Aqua adapts established ocean-rendering techniques and includes attributed
third-party assets. See [ATTRIBUTION.md](ATTRIBUTION.md) for sources and
licenses.

//! Underwater volume composite for Aqua.
//!
//! When the active camera is below the local wave surface (mean plane plus
//! sampled displacement, or the mean plane when no wave sample is ready),
//! cascade tiles are hidden and a fullscreen Core3d pass applies the same
//! Beer-Lambert mix as the surface transmission path. Fog uses only the
//! underwater segment of each view ray. Above water the pass is skipped.

#![warn(unreachable_pub)]

use bevy::{
    asset::embedded_asset,
    camera::Exposure,
    prelude::*,
    render::extract_resource::{ExtractResource, ExtractResourcePlugin},
};
use bevy_aqua_core::{
    AquaSettings, CascadeMaterial, CausticsSunVisibility, Data, Ocean, OceanView,
    ResolvedWaterBodies, ResolvedWaterBody, WaterBodiesResolved, WaterOptics, bed,
};
use bevy_aqua_query::{AquaQueryPlugin, WaveQuery, WaveSurface};

mod render;

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;

/// Adds the underwater volume composite after the main 3D pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct AquaVolumePlugin;

impl Plugin for AquaVolumePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AquaQueryPlugin>() {
            app.add_plugins(AquaQueryPlugin);
        }
        embedded_asset!(app, "volume.wgsl");
        app.init_resource::<ExtractedVolume>()
            .add_plugins(ExtractResourcePlugin::<ExtractedVolume>::default())
            .add_systems(
                PostUpdate,
                (ensure_camera_probe, detect_underwater, hide_surface)
                    .chain()
                    .after(WaterBodiesResolved),
            );
        render::add(app);
    }
}

/// GPU payload extracted each frame. Inactive frames skip the composite.
#[derive(Resource, Clone, ExtractResource, Debug)]
struct ExtractedVolume {
    active: bool,
    surface_level: f32,
    sample_waves: bool,
    optics: WaterOptics,
    caustics: Vec4,
    sun_visibility: f32,
    sun_direction: Vec3,
    sun_color: Vec3,
    environment_map: Handle<Image>,
    environment_intensity: f32,
    environment_rotation: Quat,
    time: f32,
    caustics_image: Handle<Image>,
}

impl Default for ExtractedVolume {
    fn default() -> Self {
        Self {
            active: false,
            surface_level: 0.0,
            sample_waves: false,
            optics: WaterOptics::DEEP_OCEAN,
            caustics: Vec4::ZERO,
            sun_visibility: 1.0,
            sun_direction: Vec3::Y,
            sun_color: Vec3::ZERO,
            environment_map: Handle::default(),
            environment_intensity: 0.0,
            environment_rotation: Quat::IDENTITY,
            time: 0.0,
            caustics_image: Handle::default(),
        }
    }
}

/// Surface level, optics, and whether ocean cascade displacement applies.
pub(crate) fn sample_medium(
    camera: Vec3,
    ocean: Option<&Ocean>,
    settings: &AquaSettings,
    bodies: &[ResolvedWaterBody],
    wave_height: f32,
) -> Option<(f32, WaterOptics, bool)> {
    let xz = camera.xz();
    let mut best: Option<(f32, WaterOptics)> = None;
    for body in bodies {
        if camera.y < body.level + wave_height && body.contains(xz) {
            let optics = body.optics.unwrap_or(settings.water_optics);
            if best.is_none_or(|(level, _)| body.level > level) {
                best = Some((body.level, optics));
            }
        }
    }
    if let Some((level, optics)) = best {
        return Some((level, optics, false));
    }
    let ocean = ocean?;
    (camera.y < ocean.level + wave_height).then_some((ocean.level, settings.water_optics, true))
}

fn linear_rgb(color: Color) -> Vec3 {
    color.to_linear().to_vec3()
}

fn ensure_camera_probe(
    cameras: Query<Entity, (With<OceanView>, Without<WaveQuery>)>,
    mut commands: Commands,
) {
    for entity in &cameras {
        commands.entity(entity).insert(WaveQuery);
    }
}

fn detect_underwater(
    cameras: Query<
        (
            &GlobalTransform,
            Option<&Exposure>,
            Option<&EnvironmentMapLight>,
            Option<&WaveSurface>,
        ),
        With<OceanView>,
    >,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    ocean: Option<Res<Ocean>>,
    settings: Res<AquaSettings>,
    bodies: Res<ResolvedWaterBodies>,
    sun_visibility: Res<CausticsSunVisibility>,
    time: Res<Time>,
    data: Option<Res<Data>>,
    materials: Res<Assets<CascadeMaterial>>,
    fallback: Res<bed::GpuFallback>,
    mut volume: ResMut<ExtractedVolume>,
) {
    let Some((transform, exposure, environment, wave_surface)) = cameras.iter().next() else {
        volume.active = false;
        return;
    };
    let wave_height = wave_surface
        .filter(|surface| surface.valid)
        .map(|surface| surface.displacement.y)
        .unwrap_or(0.0);
    let Some((surface_level, optics, sample_waves)) = sample_medium(
        transform.translation(),
        ocean.as_deref(),
        &settings,
        &bodies.0,
        wave_height,
    ) else {
        volume.active = false;
        return;
    };

    let exposure = exposure.copied().unwrap_or_default().exposure();
    let mut sun_direction = Vec3::Y;
    let mut sun_color = Vec3::ZERO;
    let mut best_lux = 0.0;
    for (light, light_transform) in &lights {
        if light.illuminance > best_lux {
            best_lux = light.illuminance;
            sun_direction = -Vec3::from(*light_transform.forward());
            sun_color = linear_rgb(light.color) * light.illuminance * exposure;
        }
    }
    let (environment_map, environment_intensity, environment_rotation) = environment
        .map(|light| {
            (
                light.diffuse_map.clone(),
                light.intensity * exposure,
                light.rotation.inverse(),
            )
        })
        .unwrap_or((Handle::default(), 0.0, Quat::IDENTITY));
    let caustics = settings.caustics.map_or(Vec4::ZERO, |caustics| {
        Vec4::new(
            caustics.strength.max(0.0),
            caustics.scale.max(0.01),
            caustics.speed,
            caustics.depth_max.max(0.0),
        )
    });
    let caustics_image = data
        .as_ref()
        .and_then(|data| materials.get(&data.material()))
        .map(|material| material.caustics.clone())
        .unwrap_or_else(|| fallback.0.clone());

    *volume = ExtractedVolume {
        active: true,
        surface_level,
        sample_waves,
        optics,
        caustics,
        sun_visibility: sun_visibility.0.clamp(0.0, 1.0),
        sun_direction,
        sun_color,
        environment_map,
        environment_intensity,
        environment_rotation,
        time: time.elapsed_secs(),
        caustics_image,
    };
}

fn hide_surface(
    volume: Res<ExtractedVolume>,
    mut tiles: Query<&mut Visibility, With<MeshMaterial3d<CascadeMaterial>>>,
) {
    let next = if volume.active {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut visibility in &mut tiles {
        if *visibility != next {
            *visibility = next;
        }
    }
}

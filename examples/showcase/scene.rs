//! Scene construction for the showcase.

use super::*;

type SetupAssets<'w> = (
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<ScatteringMedium>>,
    // Only present in the anim-waves scene, which registers the material.
    Option<ResMut<'w, Assets<CubemapProbeMaterial>>>,
);

pub(super) fn setup(
    mut commands: Commands,
    debug: Res<AquaDebug>,
    settings: Res<AquaSettings>,
    waves: Res<OceanWaves>,
    demo: Res<Demo>,
    asset_server: Res<AssetServer>,
    assets: SetupAssets,
) {
    let (mut meshes, mut materials, mut images, mut media, mut probe_materials) = assets;
    let anim = demo.scene == Scene::AnimWaves;
    let beauty = uses_beauty_presentation(&demo, *debug, anim);
    let lighting = showcase_lighting(&demo, anim);
    if anim {
        spawn_open_ocean_content(&mut commands, &demo, lighting, &mut media);
    } else {
        spawn_terrain_content(
            &mut commands,
            &demo,
            beauty,
            TerrainAssets {
                meshes: &mut meshes,
                materials: &mut materials,
                images: &mut images,
                media: &mut media,
            },
        );
    }
    let camera_id = spawn_showcase_camera(
        &mut commands,
        &demo,
        *debug,
        beauty,
        lighting,
        &mut meshes,
        probe_materials.as_deref_mut(),
    );
    spawn_scene_lighting(&mut commands, anim, lighting);
    spawn_showcase_buoy(
        &mut commands,
        &demo,
        anim,
        &asset_server,
        &mut meshes,
        &mut materials,
    );
    let base_label = showcase_base_label(&demo, *debug, &settings, &waves, anim);
    spawn_showcase_ui(&mut commands, camera_id, &demo, &settings, base_label, anim);
}

fn uses_beauty_presentation(demo: &Demo, debug: AquaDebug, anim: bool) -> bool {
    if anim {
        demo.bloom
            && !matches!(
                debug,
                AquaDebug::WaveHeight
                    | AquaDebug::LightRadiance
                    | AquaDebug::ReflectionFraction
                    | AquaDebug::ReflectionSanity
            )
            && demo.open.cubemap_probe.is_none()
            && demo.open.sky_only.is_none()
    } else {
        matches!(
            debug,
            AquaDebug::ShallowComposite | AquaDebug::FoamDensity | AquaDebug::FoamDensityBilinear
        ) && !demo.checker
    }
}

fn showcase_lighting(demo: &Demo, anim: bool) -> LightingSettings {
    let mut lighting = demo.lighting.settings();
    if anim {
        lighting.illuminance *= demo.open.light_scale;
        if demo.open.buoy && demo.lighting == Lighting::Night {
            lighting.exposure = 2.0;
        }
        lighting.exposure += demo.open.exposure_offset;
    } else if demo.lighting == Lighting::Night {
        lighting.exposure = 2.0;
    }
    lighting
}

fn spawn_open_ocean_content(
    commands: &mut Commands,
    demo: &Demo,
    lighting: LightingSettings,
    media: &mut Assets<ScatteringMedium>,
) {
    let open = demo.open;
    if open.cubemap_probe.is_none() && open.sky_only.is_none() && demo.water_enabled {
        commands.insert_resource(Ocean::default());
    }
    commands.spawn(Atmosphere::earth(
        media.add(ScatteringMedium::earth(256, 256)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: lighting.color,
        brightness: match demo.lighting {
            Lighting::Day => 80.0,
            Lighting::Sunset => 3.0,
            Lighting::Night => 0.002,
        },
        ..default()
    });
}

struct TerrainAssets<'a> {
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
    images: &'a mut Assets<Image>,
    media: &'a mut Assets<ScatteringMedium>,
}

fn spawn_terrain_content(
    commands: &mut Commands,
    demo: &Demo,
    beauty: bool,
    assets: TerrainAssets<'_>,
) {
    if demo.water_enabled
        && matches!(
            demo.scene,
            Scene::Island | Scene::Lake | Scene::ReflectionLake
        )
    {
        commands.insert_resource(Ocean::default());
    }
    let terrain_texture = assets.images.add(match (beauty, demo.scene) {
        (true, Scene::Island) => island_texture(),
        (true, _) => plateau_texture(),
        (false, _) => sand_texture(),
    });
    commands.spawn((
        Mesh3d(assets.meshes.add(terrain_mesh(demo.scene, !beauty))),
        MeshMaterial3d(assets.materials.add(StandardMaterial {
            base_color_texture: Some(terrain_texture),
            perceptual_roughness: 0.92,
            unlit: !beauty,
            ..default()
        })),
        #[cfg(feature = "reflect")]
        ReflectedInWater,
        #[cfg(not(feature = "reflect"))]
        (),
    ));
    commands.insert_resource(BedHeightMap::from_height_fn(
        assets.images,
        scene_height_fn(demo.scene),
        TERRAIN_RESOLUTION,
        Vec2::splat(-0.5 * TERRAIN_SIZE),
        TERRAIN_STEP,
    ));
    spawn_terrain_water_bodies(commands, demo);
    if !beauty {
        spawn_diagnostic_reference(commands, assets.meshes, assets.materials);
    }
    if beauty {
        commands.spawn(Atmosphere::earth(
            assets.media.add(ScatteringMedium::earth(256, 256)),
        ));
    }
}

fn spawn_diagnostic_reference(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(7.0, 10.0, 7.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.12, 0.07),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(30.0, 3.0, -12.0),
    ));
}

fn spawn_terrain_water_bodies(commands: &mut Commands, demo: &Demo) {
    match demo.scene {
        Scene::River => spawn_river_bodies(commands, demo.body_optics),
        Scene::PondsMany => spawn_many_ponds(commands, demo.body_optics),
        Scene::Ponds => spawn_ponds(commands, demo.body_optics),
        Scene::Island | Scene::Lake | Scene::ReflectionLake | Scene::AnimWaves => {}
    }
}

fn spawn_body(
    commands: &mut Commands,
    shape: WaterShape,
    transform: Transform,
    optics: Option<WaterOptics>,
) {
    let mut entity = commands.spawn((WaterBody, shape, transform));
    if let Some(optics) = optics {
        entity.insert(optics);
    }
}

fn spawn_river_bodies(commands: &mut Commands, optics: Option<WaterOptics>) {
    for (level, path) in [(5.0, showcase_river_upper()), (0.0, showcase_river_lower())] {
        spawn_body(
            commands,
            WaterShape::River { path },
            Transform::from_xyz(0.0, level, 0.0),
            optics,
        );
    }
    spawn_body(
        commands,
        WaterShape::Circle { radius: 56.0 },
        Transform::from_xyz(235.0, 0.0, 20.0),
        optics,
    );
}

fn spawn_many_ponds(commands: &mut Commands, optics: Option<WaterOptics>) {
    for row in 0..2 {
        for column in 0..5 {
            spawn_body(
                commands,
                WaterShape::Circle { radius: 18.0 },
                Transform::from_xyz(
                    -120.0 + 60.0 * column as f32,
                    pond_many_level(column, row),
                    -45.0 + 90.0 * row as f32,
                ),
                optics,
            );
        }
    }
}

fn spawn_ponds(commands: &mut Commands, optics: Option<WaterOptics>) {
    for (level, center, radius) in [
        (0.0, Vec2::new(-40.0, -20.0), 26.0),
        (3.0, Vec2::new(45.0, 30.0), 19.0),
    ] {
        spawn_body(
            commands,
            WaterShape::Circle { radius },
            Transform::from_xyz(center.x, level, center.y),
            optics,
        );
    }
}

fn spawn_scene_lighting(commands: &mut Commands, anim: bool, lighting: LightingSettings) {
    commands.spawn((
        DirectionalLight {
            color: lighting.color,
            illuminance: lighting.illuminance,
            shadow_maps_enabled: anim,
            ..default()
        },
        sun_transform(lighting),
        VolumetricLight,
    ));
    if anim {
        commands.spawn((
            FogVolume {
                density_factor: 0.000_1,
                absorption: 0.08,
                scattering: 0.4,
                scattering_asymmetry: 0.9,
                ..default()
            },
            Transform::from_xyz(0.0, 250.0, 0.0).with_scale(Vec3::new(20_000.0, 500.0, 20_000.0)),
        ));
    }
}

fn buoy_options(demo: &Demo, anim: bool) -> Option<BuoyOptions> {
    let open = demo.open;
    if anim && open.buoy {
        Some(BuoyOptions {
            lamp_active: open.buoy_lamp,
            spot: open.buoy_spot,
            underwater_light: open.buoy_underwater_light,
            scale: 1.0,
        })
    } else if demo.scene == Scene::ReflectionLake {
        Some(BuoyOptions {
            lamp_active: false,
            spot: false,
            underwater_light: false,
            scale: REFLECTION_LAKE_BUOY_SCALE,
        })
    } else {
        None
    }
}

fn spawn_showcase_buoy(
    commands: &mut Commands,
    demo: &Demo,
    anim: bool,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    if let Some(options) = buoy_options(demo, anim) {
        spawn_test_buoy(commands, asset_server, meshes, materials, options);
    }
}

fn showcase_camera(demo: &Demo, debug: AquaDebug, anim: bool) -> (Transform, Projection) {
    let open = demo.open;
    let camera = if let Some(step) = demo.far_dolly_step {
        far_dolly_camera(step)
    } else if let Some(pose) = demo.profile_pose {
        profile_camera(pose)
    } else if anim {
        if open.cubemap_probe.is_some() || open.sky_only.is_some() {
            Transform::from_xyz(0.0, 2.0, 32.0).looking_at(Vec3::new(0.0, 2.0, -32.0), Vec3::Y)
        } else {
            open_ocean_camera(&open, open.camera_offset)
        }
    } else {
        terrain_camera(demo, debug)
    };
    let projection = if anim {
        Projection::Perspective(PerspectiveProjection {
            far: FAR_PLANE,
            fov: open
                .cubemap_probe
                .or(open.sky_only)
                .map_or(PerspectiveProjection::default().fov, ProbeFraming::fov),
            ..default()
        })
    } else {
        Projection::default()
    };
    (camera, projection)
}

fn terrain_camera(demo: &Demo, debug: AquaDebug) -> Transform {
    if demo.scene == Scene::ReflectionLake {
        return Transform::from_translation(REFLECTION_LAKE_CAMERA)
            .looking_at(REFLECTION_LAKE_TARGET, Vec3::Y);
    }
    match debug {
        AquaDebug::RefractionValidity => {
            Transform::from_xyz(45.0, 7.0, 20.0).looking_at(Vec3::new(30.0, 2.0, -12.0), Vec3::Y)
        }
        AquaDebug::WaterPath => {
            Transform::from_xyz(54.0, 20.0, 62.0).looking_at(Vec3::ZERO, Vec3::Y)
        }
        AquaDebug::Transmission | AquaDebug::TransmissionUnrefracted => {
            Transform::from_xyz(48.0, 4.0, 35.0).looking_at(Vec3::new(15.0, -1.0, 0.0), Vec3::Y)
        }
        AquaDebug::BeerLambert => {
            Transform::from_xyz(52.0, 10.0, 58.0).looking_at(Vec3::new(0.0, -1.0, 0.0), Vec3::Y)
        }
        AquaDebug::FoamDensity | AquaDebug::FoamDensityBilinear if demo.close_up => {
            close_surface_camera()
        }
        AquaDebug::SeaFloorDepth => overview_camera(),
        AquaDebug::ShallowComposite if demo.close_up => close_surface_camera(),
        AquaDebug::ShallowComposite if demo.near_shore => {
            Transform::from_xyz(44.0, 2.0, 16.0).looking_at(Vec3::new(28.0, -1.0, 0.0), Vec3::Y)
        }
        AquaDebug::ShallowComposite
        | AquaDebug::ReflectionSanity
        | AquaDebug::FarTier
        | AquaDebug::FoamDensity
        | AquaDebug::FoamDensityBilinear
        | AquaDebug::WaveHeight => overview_camera(),
        AquaDebug::Shaded | AquaDebug::LightRadiance | AquaDebug::ReflectionFraction => {
            unreachable!()
        }
    }
}

fn close_surface_camera() -> Transform {
    let centre = Vec3::new(60.0, 0.0, 0.0);
    Transform::from_translation(centre + Vec3::new(0.0, 2.0, 8.0)).looking_at(centre, Vec3::Y)
}

fn overview_camera() -> Transform {
    Transform::from_xyz(68.0, 34.0, 82.0).looking_at(Vec3::ZERO, Vec3::Y)
}

fn spawn_showcase_camera(
    commands: &mut Commands,
    demo: &Demo,
    debug: AquaDebug,
    beauty: bool,
    lighting: LightingSettings,
    meshes: &mut Assets<Mesh>,
    probe_materials: Option<&mut Assets<CubemapProbeMaterial>>,
) -> Entity {
    let anim = demo.scene == Scene::AnimWaves;
    let (camera, projection) = showcase_camera(demo, debug, anim);
    let camera_id = commands
        .spawn((Camera3d::default(), CaptureCamera, projection, camera))
        .id();
    if anim {
        configure_anim_camera(commands, camera_id, demo, debug, beauty, lighting);
        spawn_cubemap_probe(commands, camera_id, demo, meshes, probe_materials);
    } else {
        configure_terrain_camera(commands, camera_id, beauty, lighting);
    }
    camera_id
}

fn configure_anim_camera(
    commands: &mut Commands,
    camera_id: Entity,
    demo: &Demo,
    debug: AquaDebug,
    beauty: bool,
    lighting: LightingSettings,
) {
    commands.entity(camera_id).insert((
        ShadowLodOrigin,
        AtmosphereSettings {
            rendering_method: AtmosphereMode::Raymarched,
            ..default()
        },
        AtmosphereEnvironmentMapLight {
            intensity: lighting.environment_intensity,
            size: UVec2::splat(256),
            ..default()
        },
        Exposure {
            ev100: lighting.exposure,
        },
        if matches!(
            debug,
            AquaDebug::WaveHeight | AquaDebug::LightRadiance | AquaDebug::ReflectionFraction
        ) {
            Tonemapping::None
        } else {
            Tonemapping::AcesFitted
        },
        VolumetricFog {
            ambient_intensity: 0.0,
            step_count: 96,
            ..default()
        },
    ));
    if beauty {
        commands
            .entity(camera_id)
            .insert(if demo.lighting == Lighting::Sunset {
                Bloom::NATURAL
            } else {
                common::beauty_bloom()
            });
    }
}

fn spawn_cubemap_probe(
    commands: &mut Commands,
    camera_id: Entity,
    demo: &Demo,
    meshes: &mut Assets<Mesh>,
    probe_materials: Option<&mut Assets<CubemapProbeMaterial>>,
) {
    if let (Some(_), Some(materials)) = (demo.open.cubemap_probe, probe_materials) {
        let probe = commands
            .spawn((
                Mesh3d(meshes.add(Rectangle::new(2.0, 2.0))),
                MeshMaterial3d(materials.add(CubemapProbeMaterial {})),
                Transform::from_xyz(0.0, 0.0, -1.0),
                NoFrustumCulling,
            ))
            .id();
        commands.entity(camera_id).add_child(probe);
    }
}

fn configure_terrain_camera(
    commands: &mut Commands,
    camera_id: Entity,
    beauty: bool,
    lighting: LightingSettings,
) {
    commands.entity(camera_id).insert(DepthPrepass);
    if beauty {
        commands.entity(camera_id).insert((
            AtmosphereSettings::default(),
            AtmosphereEnvironmentMapLight {
                intensity: lighting.environment_intensity,
                size: UVec2::splat(256),
                ..default()
            },
            Exposure {
                ev100: lighting.exposure,
            },
            Tonemapping::AcesFitted,
            common::beauty_bloom(),
        ));
    } else {
        commands.entity(camera_id).insert(Tonemapping::None);
    }
}

fn showcase_base_label(
    demo: &Demo,
    debug: AquaDebug,
    settings: &AquaSettings,
    waves: &OceanWaves,
    anim: bool,
) -> String {
    if anim {
        open_ocean_base_label(demo, debug, settings, waves)
    } else {
        format!(
            "{}  |  {}",
            terrain_base_label(demo, debug, waves),
            demo.lighting.label()
        )
    }
}

fn open_ocean_base_label(
    demo: &Demo,
    debug: AquaDebug,
    settings: &AquaSettings,
    waves: &OceanWaves,
) -> String {
    let open = demo.open;
    let capture_time = if demo.fixed_time {
        demo.capture_time
    } else {
        0.0
    };
    if let Some(framing) = open.cubemap_probe {
        return format!(
            "Cubemap probe {framing:?} {}  |  mip 0 x intensity x view.exposure",
            demo.lighting.label()
        );
    }
    if let Some(framing) = open.sky_only {
        return format!("Direct atmosphere {framing:?} {}", demo.lighting.label());
    }
    if open.buoy {
        return open_ocean_buoy_label(demo);
    }
    match debug {
        AquaDebug::ReflectionFraction => format!(
            "Reflection fraction  |  {}  |  white = 1",
            demo.lighting.label(),
        ),
        AquaDebug::LightRadiance => format!(
            "Direct light radiance / 16  |  {}  |  EV {:.0}",
            demo.lighting.label(),
            demo.lighting.settings().exposure + open.exposure_offset,
        ),
        AquaDebug::WaveHeight => format!(
            "{:?} signed wave height  |  {:.0} m pose  |  t={capture_time:.2}s",
            waves.model, open.height,
        ),
        _ => open_ocean_surface_label(demo, debug, settings, waves, capture_time),
    }
}

fn open_ocean_buoy_label(demo: &Demo) -> String {
    let open = demo.open;
    let lamp = if !open.buoy_lamp {
        "OFF"
    } else if open.buoy_spot {
        "ROTATING BEACON"
    } else {
        "POINT ON"
    };
    format!(
        "Buoy test {lamp}{}  |  {}  |  approximate motion",
        if open.buoy_underwater_light {
            " + UNDERLIGHT"
        } else {
            ""
        },
        demo.lighting.label(),
    )
}

fn open_ocean_surface_label(
    demo: &Demo,
    debug: AquaDebug,
    settings: &AquaSettings,
    waves: &OceanWaves,
    capture_time: f32,
) -> String {
    let backend = if waves.model == WaveModel::Spectral {
        "FFT | "
    } else {
        ""
    };
    let open = demo.open;
    if debug == AquaDebug::ReflectionSanity {
        format!(
            "Reflection sanity {}  |  FLAT WATER  |  t={capture_time:.1}s",
            demo.lighting.label(),
        )
    } else if open.boundary {
        format!(
            "Surface v2 {backend}{}  |  LOD 0/1 BOUNDARY AT +  |  t={capture_time:.1}s",
            demo.lighting.label(),
        )
    } else if open.detail_close {
        format!(
            "Ocean surface {backend}{}  |  DETAIL CLOSE  |  ripples {}  |  bloom {}  |  t={capture_time:.1}s",
            demo.lighting.label(),
            on_off(settings.detail_strength > 0.0),
            on_off(demo.bloom),
        )
    } else {
        format!(
            "Ocean surface {backend}{}  |  {:.0} m  |  ripples {}  |  bloom {}  |  t={capture_time:.1}s",
            demo.lighting.label(),
            open.height,
            on_off(settings.detail_strength > 0.0),
            on_off(demo.bloom),
        )
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "ON" } else { "OFF" }
}

fn terrain_base_label(demo: &Demo, debug: AquaDebug, waves: &OceanWaves) -> &'static str {
    match debug {
        AquaDebug::WaterPath => "Water path  |  camera depth  |  black 0 m / white 32 m",
        AquaDebug::RefractionValidity => "Refraction validity  |  green use / red cancel",
        AquaDebug::Transmission => "Transmission  |  full resolution  |  refraction ON",
        AquaDebug::TransmissionUnrefracted => "Transmission  |  full resolution  |  refraction OFF",
        AquaDebug::BeerLambert => "Beer-Lambert fog  |  per-channel preset extinction",
        AquaDebug::SeaFloorDepth => "SeaFloorDepth  |  red shallow / blue deep",
        AquaDebug::ShallowComposite => terrain_composite_label(demo, waves),
        AquaDebug::ReflectionSanity => "Reflection sanity  |  flat water",
        AquaDebug::FarTier => "Far tier  |  black near / white far",
        AquaDebug::FoamDensity | AquaDebug::FoamDensityBilinear => {
            terrain_foam_label(demo, debug, waves)
        }
        AquaDebug::WaveHeight => "Signed displacement height",
        AquaDebug::Shaded | AquaDebug::LightRadiance | AquaDebug::ReflectionFraction => {
            unreachable!()
        }
    }
}

fn terrain_composite_label(demo: &Demo, waves: &OceanWaves) -> &'static str {
    if demo.close_up {
        "Island close-up  |  transmission + lit foam  |  2 m"
    } else if !demo.checker {
        terrain_scene_label(demo.scene, waves.model)
    } else if waves.shallow_water_attenuation == 0.0 {
        "Shallow composite  |  wave attenuation OFF"
    } else if demo.near_shore {
        "Near shore  |  transmission + reflection  |  2 m"
    } else if demo.checker {
        "Island checker diagnostic  |  shaded surface"
    } else {
        "Island overview  |  SSS + detail normals + environment light"
    }
}

fn terrain_scene_label(scene: Scene, model: WaveModel) -> &'static str {
    match (scene, model == WaveModel::Spectral) {
        (Scene::Island, true) => "Island showcase  |  FFT foam  |  Jacobian whitecaps + shoreline",
        (Scene::Lake, true) => "Lake showcase  |  FFT foam  |  Jacobian whitecaps + shoreline",
        (Scene::ReflectionLake, true) => "Reflection lake  |  calm water + enlarged buoy",
        (Scene::Ponds | Scene::PondsMany, true) => {
            "Ponds showcase  |  FFT foam  |  Jacobian whitecaps + shoreline"
        }
        (Scene::River, true) => "River showcase  |  FFT foam  |  Jacobian whitecaps + shoreline",
        (Scene::Island, false) => {
            "Island showcase  |  Gerstner foam  |  persistent whitecaps + shoreline"
        }
        (Scene::Lake, false) => {
            "Lake showcase  |  Gerstner foam  |  persistent whitecaps + shoreline"
        }
        (Scene::ReflectionLake, false) => "Reflection lake  |  calm water + enlarged buoy",
        (Scene::Ponds | Scene::PondsMany, false) => {
            "Ponds showcase  |  Gerstner foam  |  persistent whitecaps + shoreline"
        }
        (Scene::River, false) => {
            "River showcase  |  Gerstner foam  |  persistent whitecaps + shoreline"
        }
        (Scene::AnimWaves, _) => unreachable!(),
    }
}

fn terrain_foam_label(demo: &Demo, debug: AquaDebug, waves: &OceanWaves) -> &'static str {
    match (debug, demo.close_up, waves.model == WaveModel::Spectral) {
        (AquaDebug::FoamDensityBilinear, true, true) => {
            "Foam density  |  FFT bilinear baseline  |  2 m"
        }
        (AquaDebug::FoamDensityBilinear, true, false) => {
            "Foam density  |  Gerstner close-up  |  2 m"
        }
        (AquaDebug::FoamDensity, true, true) => "Foam density  |  FFT close-up  |  2 m",
        (AquaDebug::FoamDensity, true, false) => "Foam density  |  Gerstner close-up  |  2 m",
        (AquaDebug::FoamDensity, false, true) => "Foam density  |  FFT persistent foam",
        (AquaDebug::FoamDensity, false, false) => "Foam density  |  Gerstner whitecaps + shoreline",
        (AquaDebug::FoamDensityBilinear, false, _) => "Foam density  |  bilinear baseline",
        _ => unreachable!(),
    }
}

fn spawn_showcase_ui(
    commands: &mut Commands,
    camera_id: Entity,
    demo: &Demo,
    settings: &AquaSettings,
    base_label: String,
    anim: bool,
) {
    if demo.ui {
        let label = look_label(&base_label, &settings.water_optics);
        commands.spawn((
            Text::new(label),
            WaterOpticsLabel(base_label),
            TextFont {
                font_size: FontSize::Px(UI_FONT_SIZE),
                ..default()
            },
            TextColor(Color::WHITE),
            UiTargetCamera(camera_id),
            Node {
                position_type: PositionType::Absolute,
                top: px(UI_MARGIN),
                left: px(UI_MARGIN),
                ..default()
            },
        ));
        if anim && demo.open.boundary {
            commands.spawn((
                Text::new("+"),
                TextFont {
                    font_size: FontSize::Px(28.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.15, 0.1)),
                UiTargetCamera(camera_id),
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(47.0),
                    left: percent(49.3),
                    ..default()
                },
            ));
        }
    }
}

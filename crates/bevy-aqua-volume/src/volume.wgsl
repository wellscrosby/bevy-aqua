#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_aqua_core::waves_sample::{
    AnimWavesUniform,
    resolve_lod,
    sample_displacement,
}

#ifdef MULTISAMPLED
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var depth_texture: texture_multisampled_2d<f32>;
#else
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var depth_texture: texture_2d<f32>;
#endif
@group(0) @binding(2) var screen_sampler: sampler;

struct VolumeUniform {
    world_from_clip: mat4x4<f32>,
    camera_position: vec4<f32>,
    extinction: vec4<f32>,
    deep_color: vec4<f32>,
    grazing_color: vec4<f32>,
    shallow_color: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_radiance: vec4<f32>,
    environment: vec4<f32>,
    env_rotation: vec4<f32>,
    caustics: vec4<f32>,
    sea: vec4<f32>,
}

@group(0) @binding(3) var<uniform> volume: VolumeUniform;
@group(0) @binding(4) var caustics_texture: texture_2d<f32>;
@group(0) @binding(5) var caustics_sampler: sampler;
@group(0) @binding(6) var lod_data: texture_2d_array<f32>;
@group(0) @binding(7) var lod_sampler: sampler;
@group(0) @binding(8) var<uniform> waves: AnimWavesUniform;
@group(0) @binding(9) var environment_map: texture_cube<f32>;
@group(0) @binding(10) var environment_sampler: sampler;
@group(0) @binding(11) var bed_height: texture_2d<f32>;

const CAUSTIC_CELLS_PER_TILE: f32 = 16.0;
const CAUSTIC_FOCUS_GAIN: f32 = 12.0;
const CAUSTIC_DAYLIGHT_MIN_LUX: f32 = 64.0;
const CAUSTIC_REFERENCE_SUN_LUX: f32 = 120000.0;
const CAUSTIC_LUMINANCE_WEIGHTS: vec3<f32> = vec3(0.2126, 0.7152, 0.0722);
const PATH_LENGTH_MAX: f32 = 512.0;
const NO_BED_DEPTH: f32 = 256.0;

fn quat_rotate(rotation: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
    return value + 2.0 * cross(rotation.xyz, cross(rotation.xyz, value) + rotation.w * value);
}

fn beer_lambert_mix(
    scene_colour: vec3<f32>,
    scatter_colour: vec3<f32>,
    extinction: vec3<f32>,
    path_length: f32,
) -> vec3<f32> {
    return mix(scene_colour, scatter_colour, 1.0 - exp(-extinction * path_length));
}

fn deep_water_weight(water_depth: f32) -> f32 {
    return smoothstep(0.35, volume.shallow_color.a, water_depth);
}

fn depth_scaled_extinction(extinction: vec3<f32>, water_depth: f32) -> vec3<f32> {
    let shallow_extinction_scale = mix(vec3(0.52, 0.42, 0.62), vec3(1.0), deep_water_weight(water_depth));
    return extinction * shallow_extinction_scale;
}

fn reconstruct_world(uv: vec2<f32>, raw_depth: f32) -> vec3<f32> {
    let clip = vec4(uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0), raw_depth, 1.0);
    let world = volume.world_from_clip * clip;
    return world.xyz / max(world.w, 1e-8);
}

fn displacement_y(world_xz: vec2<f32>) -> f32 {
    if volume.sea.w < 0.5 {
        return 0.0;
    }
    let sampled_xz = world_xz - waves.flow.xy * waves.time.x;
    let lod = resolve_lod(waves.cascade_layout, sampled_xz);
    return sample_displacement(
        lod_data,
        lod_sampler,
        waves.cascade_layout,
        sampled_xz,
        lod.lod,
        lod.alpha,
    ).y;
}

fn surface_y(world_xz: vec2<f32>) -> f32 {
    return volume.sea.x + displacement_y(world_xz);
}

fn underwater_path_end(camera: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let ray = world - camera;
    let ray_len = length(ray);
    if ray_len < 1e-4 {
        return world;
    }
    var sample_xz = camera.xz;
    if abs(ray.y) > 1e-5 {
        let t_plane = (volume.sea.x - camera.y) / ray.y;
        if t_plane > 0.0 {
            sample_xz = (camera + t_plane * ray).xz;
        }
    }
    let water_y = surface_y(sample_xz);
    var t_end = 1.0;
    if abs(ray.y) > 1e-5 {
        let t_surf = (water_y - camera.y) / ray.y;
        if t_surf > 0.0 {
            t_end = min(t_end, t_surf);
        }
    } else if camera.y >= water_y {
        t_end = 0.0;
    }
    return camera + t_end * ray;
}

fn bed_water_depth(world_xz: vec2<f32>) -> f32 {
    let range = waves.cascade_layout.bed_range;
    if range.y < 0.0 {
        return NO_BED_DEPTH;
    }
    let uv = (world_xz - waves.cascade_layout.bed_transform.xy)
        * waves.cascade_layout.bed_transform.zw;
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return NO_BED_DEPTH;
    }
    let dimensions = vec2<f32>(textureDimensions(bed_height));
    let texel_position = uv * (dimensions - vec2(1.0));
    let base = vec2<i32>(floor(texel_position));
    let fraction = fract(texel_position);
    let maximum = i32(dimensions.x) - 1;
    let maximum_y = i32(dimensions.y) - 1;
    let p00 = clamp(base, vec2(0), vec2(maximum, maximum_y));
    let p10 = clamp(base + vec2(1, 0), vec2(0), vec2(maximum, maximum_y));
    let p01 = clamp(base + vec2(0, 1), vec2(0), vec2(maximum, maximum_y));
    let p11 = clamp(base + vec2(1), vec2(0), vec2(maximum, maximum_y));
    let row_0 = mix(
        textureLoad(bed_height, p00, 0).r,
        textureLoad(bed_height, p10, 0).r,
        fraction.x,
    );
    let row_1 = mix(
        textureLoad(bed_height, p01, 0).r,
        textureLoad(bed_height, p11, 0).r,
        fraction.x,
    );
    let height = mix(row_0, row_1, fraction.y) * range.y + range.x;
    return max(range.z - height, 0.0);
}

fn environment_along(direction: vec3<f32>) -> vec3<f32> {
    var sample_direction = quat_rotate(volume.env_rotation, direction);
    sample_direction.y = max(sample_direction.y, 0.0);
    let length_squared = dot(sample_direction, sample_direction);
    sample_direction = select(
        vec3(0.0, 1.0, 0.0),
        sample_direction * inverseSqrt(length_squared),
        length_squared > 1e-8,
    );
    sample_direction.z = -sample_direction.z;
    return textureSampleLevel(
        environment_map,
        environment_sampler,
        sample_direction,
        0.0,
    ).rgb * volume.environment.x;
}

fn environment_irradiance() -> vec3<f32> {
    return environment_along(vec3(0.0, 1.0, 0.0));
}

fn scatter_colour(to_view: vec3<f32>, water_depth: f32) -> vec3<f32> {
    let view_vertical = abs(to_view.y);
    let deep_body_albedo = mix(
        volume.grazing_color.rgb,
        volume.deep_color.rgb,
        view_vertical,
    );
    let body_albedo = mix(
        volume.shallow_color.rgb,
        deep_body_albedo,
        deep_water_weight(water_depth),
    );
    return body_albedo * environment_irradiance() * volume.extinction.w;
}

fn apply_caustics(
    scene_colour: vec3<f32>,
    world_xz: vec2<f32>,
    water_depth: f32,
    extinction: vec3<f32>,
) -> vec3<f32> {
    let strength = volume.caustics.x * volume.sea.y;
    let sun_direction = volume.sun_direction.xyz;
    if strength <= 0.0 || water_depth >= volume.caustics.w || sun_direction.y <= 0.0 {
        return scene_colour;
    }
    let scale = volume.caustics.y * CAUSTIC_CELLS_PER_TILE;
    let scroll = volume.caustics.z * volume.sea.z / scale;
    let uv_a = world_xz / scale + vec2(scroll, 0.63 * scroll);
    let uv_b = world_xz * 1.37 / scale + vec2(-0.71 * scroll, 0.43 * scroll);
    let a = textureSampleLevel(caustics_texture, caustics_sampler, uv_a, 0.0).r;
    let b = textureSampleLevel(caustics_texture, caustics_sampler, uv_b, 0.0).r;
    let pattern = CAUSTIC_FOCUS_GAIN * a * b;
    let depth_gate = 1.0 - smoothstep(0.0, volume.caustics.w, water_depth);
    let direct_lux = dot(max(volume.sun_radiance.rgb, vec3(0.0)), CAUSTIC_LUMINANCE_WEIGHTS);
    let daylight = clamp(
        (direct_lux - CAUSTIC_DAYLIGHT_MIN_LUX)
            / (CAUSTIC_REFERENCE_SUN_LUX - CAUSTIC_DAYLIGHT_MIN_LUX),
        0.0,
        1.0,
    );
    let bed_incidence = max(sun_direction.y, 0.0);
    let incoming_path = water_depth / max(bed_incidence, 0.02);
    let incoming_transmission = exp(-extinction * incoming_path);
    let sun_chroma = max(volume.sun_radiance.rgb, vec3(0.0))
        / max(direct_lux, CAUSTIC_DAYLIGHT_MIN_LUX);
    let focus = strength * depth_gate * pattern * daylight * bed_incidence;
    return scene_colour + scene_colour * sun_chroma * incoming_transmission * focus;
}

@fragment
fn fragment(
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
    in: FullscreenVertexOutput,
) -> @location(0) vec4<f32> {
    let texture_size = vec2<f32>(textureDimensions(screen_texture));
    let frag_coords = vec2<i32>(in.uv * texture_size);
#ifdef MULTISAMPLED
    let scene = textureLoad(screen_texture, frag_coords, 0).rgb;
    let raw_depth = textureLoad(depth_texture, frag_coords, i32(sample_index)).x;
#else
    let scene = textureSample(screen_texture, screen_sampler, in.uv).rgb;
    let raw_depth = textureLoad(depth_texture, frag_coords, 0).x;
#endif

    let world = reconstruct_world(in.uv, raw_depth);
    let camera = volume.camera_position.xyz;
    let look = camera - world;
    let look_len = max(length(look), 1e-4);
    let to_view = look / look_len;
    let path_end = underwater_path_end(camera, world);
    let path_length = min(length(path_end - camera), PATH_LENGTH_MAX);
    let clipped = length(world - path_end) > 0.05;
    let water_depth = select(max(volume.sea.x - world.y, 0.0), 0.0, clipped);
    let albedo_depth = bed_water_depth(select(world.xz, camera.xz, clipped));
    let extinction = depth_scaled_extinction(volume.extinction.rgb, water_depth);
    // The opaque pass still contains the unrefracted above-water draw. The
    // underside window is the copy that should remain. Drop that draw when
    // the hit is in air, including holes the sheet did not cover.
    let above_sheet = world.y > surface_y(world.xz) + 0.05;
    let look_out = select(
        world - camera,
        vec3(0.0, 1.0, 0.0),
        length(world - camera) < 1e-4,
    );
    let lit_scene = select(
        apply_caustics(scene, world.xz, water_depth, extinction),
        environment_along(look_out),
        clipped && above_sheet,
    );
    let colour = beer_lambert_mix(
        lit_scene,
        scatter_colour(to_view, albedo_depth),
        extinction,
        path_length,
    );
    return vec4(colour, 1.0);
}

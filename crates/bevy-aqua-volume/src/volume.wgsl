#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_aqua_core::waves_sample::{
    AnimWavesUniform,
    resolve_lod,
    sample_displacement,
}
#import bevy_pbr::mesh_view_bindings::{lights, view}
#import bevy_pbr::view_transformations::{
    frag_coord_to_ndc,
    position_ndc_to_world,
}

struct VolumeUniform {
    extinction: vec4<f32>,
    environment: vec4<f32>,
    sea: vec4<f32>,
}

#ifdef MULTISAMPLED
@group(1) @binding(0) var screen_texture: texture_2d<f32>;
@group(1) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(1) @binding(0) var screen_texture: texture_2d<f32>;
@group(1) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(1) @binding(2) var screen_sampler: sampler;
@group(1) @binding(3) var<uniform> volume: VolumeUniform;
@group(1) @binding(4) var lod_data: texture_2d_array<f32>;
@group(1) @binding(5) var lod_sampler: sampler;
@group(1) @binding(6) var<uniform> waves: AnimWavesUniform;

const FRAC_4_PI: f32 = 0.07957747154594767;
const PATH_LENGTH_MAX: f32 = 256.0;
const SURFACE_HIT_SLACK: f32 = 1.0;
const SKY_FRACTION: f32 = 0.4;
const SUN_BODY: f32 = 0.5;
const N_WATER: f32 = 1.333;
const MIN_L_Y: f32 = 0.02;
const KAPPA_EPS: f32 = 1e-5;
const OPTICAL_CLAMP: f32 = 80.0;
// Particle scatter at 550 nm for scatter_scale = 1, in 1/m. Red-heavy σt is
// absorption; this is the particle load, weakly blue (~λ^{-1}).
const PARTICLE_SCATTER: f32 = 0.02;
const SCATTER_SPECTRUM: vec3<f32> = vec3(0.85, 1.0, 1.22);

fn henyey_greenstein(l_dot_rd: f32, g: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * l_dot_rd;
    return FRAC_4_PI * (1.0 - g * g) / (denom * sqrt(denom));
}

fn displacement_y(world_xz: vec2<f32>) -> f32 {
    if volume.environment.w < 0.5 {
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

fn view_ray_direction(frag_xy: vec2<f32>) -> vec3<f32> {
    // Near plane (NDC z = 1). Reverse-Z infinite perspective puts the far
    // plane at infinity, so a depth-0 reconstruct is Inf/NaN.
    let near_world = position_ndc_to_world(frag_coord_to_ndc(vec4(frag_xy, 1.0, 1.0)));
    let dir = near_world - view.world_position;
    return dir / max(length(dir), 1e-4);
}

fn intersect_surface_metres(origin: vec3<f32>, rd: vec3<f32>, t_max: f32, surface: f32) -> f32 {
    if rd.y <= 1e-5 {
        return t_max;
    }
    return clamp((surface - origin.y) / rd.y, 0.0, t_max);
}

fn fresnel_air_to_water(cos_air: f32) -> f32 {
    let n1 = 1.0;
    let n2 = N_WATER;
    let eta = n1 / n2;
    let sin2_t = eta * eta * (1.0 - cos_air * cos_air);
    if sin2_t >= 1.0 {
        return 1.0;
    }
    let cos_t = sqrt(1.0 - sin2_t);
    let rs = (n1 * cos_air - n2 * cos_t) / (n1 * cos_air + n2 * cos_t);
    let rp = (n2 * cos_air - n1 * cos_t) / (n2 * cos_air + n1 * cos_t);
    return 0.5 * (rs * rs + rp * rp);
}

fn refract_air_to_water(l_air: vec3<f32>) -> vec3<f32> {
    let sin2_air = max(1.0 - l_air.y * l_air.y, 0.0);
    let sin2_water = sin2_air / (N_WATER * N_WATER);
    let cos_water = sqrt(max(1.0 - sin2_water, 0.0));
    let horiz_len = length(l_air.xz);
    var xz = vec2(0.0);
    if horiz_len > 1e-8 {
        xz = l_air.xz / horiz_len * sqrt(sin2_water);
    }
    return vec3(xz.x, cos_water, xz.y);
}

// ∫_0^t I0 exp(-σ (d0 - s rd.y) / L.y) exp(-σ s) ds
// with I0 the irradiance just under the surface along L.
fn downwelling_integral(
    sigma: vec3<f32>,
    t: f32,
    rd_y: f32,
    l_y: f32,
    d0: f32,
    irradiance: vec3<f32>,
) -> vec3<f32> {
    let ly = max(l_y, MIN_L_Y);
    let i0 = irradiance * exp(-sigma * (d0 / ly));
    let kappa = sigma * (1.0 - rd_y / ly);
    let optical = clamp(kappa * t, vec3(-OPTICAL_CLAMP), vec3(OPTICAL_CLAMP));
    let use_series = abs(kappa) <= vec3(KAPPA_EPS);
    let kappa_safe = select(kappa, vec3(1.0), use_series);
    let integral = select(
        (vec3(1.0) - exp(-optical)) / kappa_safe,
        vec3(t),
        use_series,
    );
    return i0 * integral;
}

@fragment
fn fragment(
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
    in: FullscreenVertexOutput,
) -> @location(0) vec4<f32> {
    var scene = textureSample(screen_texture, screen_sampler, in.uv).rgb;
#ifdef MULTISAMPLED
    let raw_depth = textureLoad(depth_texture, vec2<i32>(in.position.xy), i32(sample_index));
#else
    let raw_depth = textureLoad(depth_texture, vec2<i32>(in.position.xy), 0);
#endif

    let camera = view.world_position;
    let plane = volume.sea.x;
    var surface = plane;
    if camera.y >= plane {
        let camera_surface = plane + displacement_y(camera.xz);
        if camera.y >= camera_surface {
            return vec4(scene, 1.0);
        }
        surface = camera_surface;
    }

    let rd_world = view_ray_direction(in.position.xy);
    var t_scene = PATH_LENGTH_MAX;
    if raw_depth > 0.0 {
        let world = position_ndc_to_world(frag_coord_to_ndc(vec4(in.position.xy, raw_depth, 1.0)));
        t_scene = min(length(world - camera), PATH_LENGTH_MAX);
    }
    let t_surface = intersect_surface_metres(camera, rd_world, PATH_LENGTH_MAX, surface);
    var t_end = min(t_scene, PATH_LENGTH_MAX);
    if rd_world.y > 0.0 {
        t_end = min(t_end, t_surface);
        if raw_depth > 0.0 && abs(t_scene - t_surface) < SURFACE_HIT_SLACK {
            scene = vec3(0.0);
        }
    }
    if t_end < 1e-4 {
        return vec4(scene, 1.0);
    }

    let sigma_t = volume.extinction.rgb;
    let sigma_s = min(
        sigma_t,
        PARTICLE_SCATTER * max(volume.extinction.w, 0.0) * SCATTER_SPECTRUM,
    );
    let g = volume.environment.z;
    let exposure = view.exposure;
    let d0 = max(surface - camera.y, 0.0);
    let weighted = sigma_s * volume.environment.y;

    var sun_surface = vec3(0.0);
    var inscatter = vec3<f32>(0.0);
    let directional_light_count = lights.n_directional_lights;
    for (var light_index = 0u; light_index < directional_light_count; light_index += 1u) {
        let light = &lights.directional_lights[light_index];
        let l_air = (*light).direction_to_light.xyz;
        if l_air.y <= 0.0 {
            continue;
        }
        let e_air = (*light).color.rgb * exposure;
        sun_surface += e_air * l_air.y;
        let l_water = refract_air_to_water(l_air);
        let e_water = e_air * (1.0 - fresnel_air_to_water(l_air.y));
        let phase = henyey_greenstein(dot(l_water, rd_world), g);
        inscatter += weighted * downwelling_integral(
            sigma_t,
            t_end,
            rd_world.y,
            l_water.y,
            d0,
            e_water * (SUN_BODY + phase),
        );
    }

    let sky = select(
        vec3(volume.environment.x),
        sun_surface * SKY_FRACTION,
        any(sun_surface > vec3(1e-6)),
    );
    inscatter += weighted * downwelling_integral(
        sigma_t,
        t_end,
        rd_world.y,
        1.0,
        d0,
        sky,
    );

    let transmittance = exp(-sigma_t * t_end);
    return vec4(scene * transmittance + inscatter, 1.0);
}

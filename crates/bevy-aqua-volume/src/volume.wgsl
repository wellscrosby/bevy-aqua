#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_aqua_core::waves_sample::{
    AnimWavesUniform,
    resolve_lod,
    sample_displacement,
}
#import bevy_pbr::mesh_view_bindings::{globals, lights, view, clustered_lights}
#import bevy_pbr::mesh_view_types::{
    DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT,
    POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    POINT_LIGHT_FLAGS_VOLUMETRIC_BIT,
    POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
}
#import bevy_pbr::shadow_sampling::{
    sample_shadow_map_hardware,
    sample_shadow_cubemap,
    sample_shadow_map,
    SPOT_SHADOW_TEXEL_SIZE,
}
#import bevy_pbr::shadows::{get_cascade_index, world_to_directional_light_local}
#import bevy_pbr::utils::interleaved_gradient_noise
#import bevy_pbr::view_transformations::{
    frag_coord_to_ndc,
    position_ndc_to_world,
}
#import bevy_render::maths::orthonormalize
#import bevy_pbr::clustered_forward as clustering
#import bevy_pbr::lighting::getDistanceAttenuation

struct VolumeUniform {
    extinction: vec4<f32>,
    deep_color: vec4<f32>,
    grazing_color: vec4<f32>,
    shallow_color: vec4<f32>,
    sss_tint: vec4<f32>,
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
@group(1) @binding(7) var bed_height: texture_2d<f32>;

const FRAC_4_PI: f32 = 0.07957747154594767;
const PATH_LENGTH_MAX: f32 = 256.0;
const NO_BED_DEPTH: f32 = 256.0;
const SCATTER_FRACTION: f32 = 0.45;
const SURFACE_REFINE: u32 = 4u;
const MIN_TRANSMITTANCE: f32 = 0.001;
const SKY_FRACTION: f32 = 0.4;
const SURFACE_HIT_SLACK: f32 = 1.0;
const BODY_SCATTER: f32 = 0.2;
const SSS_SCATTER: f32 = 0.05;
const GODOT_WATER_ALBEDO: vec3<f32> = vec3(0.010022826, 0.01960665, 0.02721178);

fn henyey_greenstein(neg_LdotV: f32, g: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * neg_LdotV;
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

fn surface_y(world_xz: vec2<f32>) -> f32 {
    return volume.sea.x + displacement_y(world_xz);
}

fn deep_water_weight(water_depth: f32) -> f32 {
    return smoothstep(0.35, volume.shallow_color.a, water_depth);
}

fn scatter_albedo(to_view: vec3<f32>, water_depth: f32) -> vec3<f32> {
    let view_vertical = abs(to_view.y);
    let deep_body = mix(volume.grazing_color.rgb, volume.deep_color.rgb, view_vertical);
    return mix(volume.shallow_color.rgb, deep_body, deep_water_weight(water_depth));
}

fn volume_scatter_color(to_view: vec3<f32>, water_depth: f32) -> vec3<f32> {
    let body = scatter_albedo(to_view, water_depth);
    return GODOT_WATER_ALBEDO + volume.sss_tint.rgb * SSS_SCATTER + body * BODY_SCATTER;
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

fn view_ray_direction(frag_xy: vec2<f32>) -> vec3<f32> {
    // Near plane (NDC z = 1). Reverse-Z infinite perspective puts the far
    // plane at infinity, so a depth-0 reconstruct is Inf/NaN.
    let near_world = position_ndc_to_world(frag_coord_to_ndc(vec4(frag_xy, 1.0, 1.0)));
    let dir = near_world - view.world_position;
    return dir / max(length(dir), 1e-4);
}

fn intersect_surface_metres(origin: vec3<f32>, rd: vec3<f32>, t_max: f32) -> f32 {
    if rd.y <= 1e-5 {
        return t_max;
    }
    var t = clamp((volume.sea.x - origin.y) / rd.y, 0.0, t_max);
    for (var i = 0u; i < SURFACE_REFINE; i += 1u) {
        let p = origin + t * rd;
        let err = p.y - surface_y(p.xz);
        t = clamp(t - err / max(rd.y, 0.05), 0.0, t_max);
    }
    return t;
}

fn directional_sun_irradiance(exposure: f32) -> vec3<f32> {
    var sun_irradiance = vec3(0.0);
    let directional_light_count = lights.n_directional_lights;
    for (var light_index = 0u; light_index < directional_light_count; light_index += 1u) {
        let light = &lights.directional_lights[light_index];
        let L = (*light).direction_to_light.xyz;
        sun_irradiance += (*light).color.rgb * exposure * max(L.y, 0.0);
    }
    return sun_irradiance;
}

fn column_to_surface(p: vec3<f32>, up: f32) -> f32 {
    return max(surface_y(p.xz) - p.y, 0.0) / max(up, 0.02);
}

fn light_water_path(light_pos: vec3<f32>, p: vec3<f32>) -> f32 {
    let light_surf = surface_y(light_pos.xz);
    if light_pos.y > light_surf {
        let to_light = light_pos - p;
        let to_light_len = length(to_light);
        if to_light_len < 1e-4 {
            return 0.0;
        }
        return column_to_surface(p, abs(to_light.y) / to_light_len);
    }
    return length(light_pos - p);
}

fn fetch_point_shadow_without_normal(
    light_id: u32,
    frag_position: vec4<f32>,
    frag_coord_xy: vec2<f32>,
) -> f32 {
    let light = &clustered_lights.data[light_id];
    let surface_to_light = (*light).position_radius.xyz - frag_position.xyz;
    let surface_to_light_abs = abs(surface_to_light);
    let distance_to_light = max(
        surface_to_light_abs.x,
        max(surface_to_light_abs.y, surface_to_light_abs.z),
    );
    let depth_offset = (*light).shadow_depth_bias * normalize(surface_to_light.xyz);
    let offset_position = frag_position.xyz + depth_offset;
    let frag_ls = offset_position.xyz - (*light).position_radius.xyz;
    let abs_position_ls = abs(frag_ls);
    let major_axis_magnitude = max(
        abs_position_ls.x,
        max(abs_position_ls.y, abs_position_ls.z),
    );
    let zw = -major_axis_magnitude * (*light).light_custom_data.xy
        + (*light).light_custom_data.zw;
    let depth = zw.x / zw.y;
    let flip_z = vec3(1.0, 1.0, -1.0);
    return sample_shadow_cubemap(
        frag_ls * flip_z,
        distance_to_light,
        depth,
        light_id,
        frag_coord_xy,
    );
}

fn fetch_spot_shadow_without_normal(
    light_id: u32,
    frag_position: vec4<f32>,
    frag_coord_xy: vec2<f32>,
) -> f32 {
    let light = &clustered_lights.data[light_id];
    let surface_to_light = (*light).position_radius.xyz - frag_position.xyz;
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
        spot_dir.y = -spot_dir.y;
    }
    let fwd = -spot_dir;
    let offset_position = -surface_to_light
        + ((*light).shadow_depth_bias * normalize(surface_to_light));
    let light_inv_rot = orthonormalize(fwd);
    let projected_position = offset_position * light_inv_rot;
    let f_div_minus_z = 1.0 / ((*light).spot_light_tan_angle * -projected_position.z);
    let shadow_xy_ndc = projected_position.xy * f_div_minus_z;
    let shadow_uv = shadow_xy_ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    let depth = 0.1 / -projected_position.z;
    return sample_shadow_map(
        shadow_uv,
        depth,
        i32(light_id) + lights.spot_light_shadowmap_offset,
        frag_coord_xy,
        SPOT_SHADOW_TEXEL_SIZE,
    );
}

fn view_z(p_world: vec3<f32>) -> f32 {
    return (view.view_from_world * vec4(p_world, 1.0)).z;
}

fn directional_shadow(light_index: u32, p_world: vec3<f32>, p_view_z: f32) -> f32 {
    let light = &lights.directional_lights[light_index];
    let L = (*light).direction_to_light.xyz;
    let depth_offset = (*light).shadow_depth_bias * L;
    let cascade_index = get_cascade_index(light_index, p_view_z);
    let light_local = world_to_directional_light_local(
        light_index,
        cascade_index,
        vec4(p_world + depth_offset, 1.0),
    );
    if light_local.w == 0.0 {
        return 1.0;
    }
    let array_index = i32((*light).depth_texture_base_index + cascade_index);
    return sample_shadow_map_hardware(light_local.xy, light_local.z, array_index);
}

fn clustered_in_scatter(
    p_world: vec3<f32>,
    rd_world: vec3<f32>,
    frag_coord: vec4<f32>,
    p_view_z: f32,
    sigma_t: vec3<f32>,
    g: f32,
    exposure: f32,
) -> vec3<f32> {
    var sample_color = vec3(0.0);
    let is_orthographic = view.clip_from_view[3].w == 1.0;
    let cluster_index = clustering::view_fragment_cluster_index(
        frag_coord.xy,
        p_view_z,
        is_orthographic,
    );
    var ranges = clustering::unpack_clusterable_object_index_ranges(cluster_index);
    for (
        var i: u32 = ranges.first_point_light_index_offset;
        i < ranges.first_reflection_probe_index_offset;
        i = i + 1u
    ) {
        let light_id = clustering::get_clusterable_object_id(i);
        let light = &clustered_lights.data[light_id];
        if (((*light).flags & POINT_LIGHT_FLAGS_VOLUMETRIC_BIT) == 0u) {
            continue;
        }
        let light_to_frag = (*light).position_radius.xyz - p_world;
        let distance_square = dot(light_to_frag, light_to_frag);
        let L = light_to_frag * inverseSqrt(max(distance_square, 1e-8));
        var local_light = getDistanceAttenuation(
            distance_square,
            (*light).color_inverse_square_range.w,
        );
        if i < ranges.first_spot_light_index_offset {
            if (((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
                local_light *= fetch_point_shadow_without_normal(
                    light_id,
                    vec4(p_world, 1.0),
                    frag_coord.xy,
                );
            }
        } else {
            var spot_dir = vec3<f32>(
                (*light).light_custom_data.x,
                0.0,
                (*light).light_custom_data.y,
            );
            spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
            if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
                spot_dir.y = -spot_dir.y;
            }
            let cd = dot(-spot_dir, L);
            let attenuation = clamp(
                cd * (*light).light_custom_data.z + (*light).light_custom_data.w,
                0.0,
                1.0,
            );
            local_light *= attenuation * attenuation;
            if (((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
                local_light *= fetch_spot_shadow_without_normal(
                    light_id,
                    vec4(p_world, 1.0),
                    frag_coord.xy,
                );
            }
        }
        if local_light <= 0.0 {
            continue;
        }
        let water_path = light_water_path((*light).position_radius.xyz, p_world);
        let phase = henyey_greenstein(dot(L, rd_world), g);
        sample_color += (*light).color_inverse_square_range.rgb
            * local_light
            * phase
            * exp(-sigma_t * water_path)
            * exposure;
    }
    return sample_color;
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
    if camera.y >= surface_y(camera.xz) {
        return vec4(scene, 1.0);
    }

    let rd_world = view_ray_direction(in.position.xy);
    let to_view = -rd_world;
    var t_scene = PATH_LENGTH_MAX;
    if raw_depth > 0.0 {
        let world = position_ndc_to_world(frag_coord_to_ndc(vec4(in.position.xy, raw_depth, 1.0)));
        t_scene = min(length(world - camera), PATH_LENGTH_MAX);
    }
    let t_surface = intersect_surface_metres(camera, rd_world, PATH_LENGTH_MAX);
    var t_end = min(t_scene, PATH_LENGTH_MAX);
    if rd_world.y > 0.0 {
        t_end = min(t_end, t_surface);
        if raw_depth > 0.0 && abs(t_scene - t_surface) < SURFACE_HIT_SLACK {
            scene = vec3(0.0);
        }
    }
    let ray_length = t_end;
    if ray_length < 1e-4 {
        return vec4(scene, 1.0);
    }

    let step_count = max(u32(volume.sea.y + 0.5), 1u);
    let step_size = ray_length / f32(step_count);
    let jitter = interleaved_gradient_noise(in.position.xy, globals.frame_count)
        * volume.environment.y;
    let sigma_t = volume.extinction.rgb;
    let sigma_s = sigma_t * volume.extinction.w * SCATTER_FRACTION;
    let sigma_s_body = vec3(0.5 * (sigma_t.g + sigma_t.b));
    let scatter_scale = volume.extinction.w;
    let g = volume.environment.z;
    let exposure = view.exposure;
    let sun_irradiance = directional_sun_irradiance(exposure);
    let sky_irradiance = select(
        vec3(volume.environment.x),
        sun_irradiance * SKY_FRACTION,
        any(sun_irradiance > vec3(1e-6)),
    );

    var transmittance = vec3(1.0);
    var inscatter = vec3(0.0);
    let directional_light_count = lights.n_directional_lights;

    for (var step = 0u; step < step_count; step += 1u) {
        if all(transmittance < vec3(MIN_TRANSMITTANCE)) {
            break;
        }
        let t = (f32(step) + 0.5 + jitter) * step_size;
        let p_world = camera + rd_world * t;
        if p_world.y >= surface_y(p_world.xz) {
            break;
        }
        let depth = max(surface_y(p_world.xz) - p_world.y, 0.0);
        let scatter = volume_scatter_color(to_view, bed_water_depth(p_world.xz));
        let fog = scatter
            * (sky_irradiance + sun_irradiance * 0.5)
            * scatter_scale
            * exp(-sigma_t * depth);

        var shafts = clustered_in_scatter(
            p_world,
            rd_world,
            in.position,
            view_z(p_world),
            sigma_t,
            g,
            exposure,
        );

        for (var light_index = 0u; light_index < directional_light_count; light_index += 1u) {
            let light = &lights.directional_lights[light_index];
            if (((*light).flags & DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT) == 0u) {
                break;
            }
            let L = (*light).direction_to_light.xyz;
            let shadow = directional_shadow(light_index, p_world, view_z(p_world));
            if shadow <= 0.0 {
                continue;
            }
            let phase = henyey_greenstein(dot(L, rd_world), g);
            shafts += (*light).color.rgb
                * shadow
                * phase
                * exp(-sigma_t * column_to_surface(p_world, max(L.y, 0.0)))
                * exposure;
        }

        inscatter += transmittance * (sigma_s_body * fog + sigma_s * shafts) * step_size;
        transmittance *= exp(-sigma_t * step_size);
    }

    return vec4(scene * transmittance + inscatter, 1.0);
}

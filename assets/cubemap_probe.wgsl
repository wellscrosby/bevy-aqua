// Direct Bevy environment-cubemap probe for the calm-water reflection audit.
#import bevy_pbr::{
    forward_io::Vertex,
    mesh_view_bindings::{light_probes, view},
}
#import bevy_pbr::mesh_view_bindings as view_bindings

struct ProbeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

fn quat_rotate(rotation: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
    return value + 2.0 * cross(rotation.xyz, cross(rotation.xyz, value) + rotation.w * value);
}

@vertex
fn vertex(vertex: Vertex) -> ProbeOutput {
    var out: ProbeOutput;
    out.ndc = 2.0 * vertex.position.xy;
    out.position = vec4(out.ndc, 0.5, 1.0);
    return out;
}

@fragment
fn fragment(in: ProbeOutput) -> @location(0) vec4<f32> {
    let world_position = view.world_from_clip * vec4(in.ndc, 0.5, 1.0);
    let direction = normalize(world_position.xyz / world_position.w - view.world_position);
    var radiance = vec3(0.0);
#ifdef ENVIRONMENT_MAP
    if light_probes.view_cubemap_index >= 0 {
        var sample_direction = quat_rotate(light_probes.view_rotation, direction);
        sample_direction.z = -sample_direction.z;
#ifdef MULTIPLE_LIGHT_PROBES_IN_ARRAY
        radiance = textureSampleLevel(
            view_bindings::specular_environment_maps[u32(light_probes.view_cubemap_index)],
            view_bindings::environment_map_sampler,
            sample_direction,
            0.0,
        ).rgb * light_probes.intensity_for_view * view.exposure;
#else
        radiance = textureSampleLevel(
            view_bindings::specular_environment_map,
            view_bindings::environment_map_sampler,
            sample_direction,
            0.0,
        ).rgb * light_probes.intensity_for_view * view.exposure;
#endif
    }
#endif
    return vec4(radiance, 1.0);
}

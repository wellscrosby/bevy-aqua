// Closed-form river-wave synthesis shared by every GPU consumer: the cascade
// material's vertex path, motion prepass, and wave-query compute pass.

#define_import_path aqua_core::river

/// Analytic sum-of-sines displacement for river bodies.
///
/// Small bodies use the closed-form path: three
/// components travelling along the baked local current, deep-water
/// dispersion omega = sqrt(g k), amplitude scaled by channel width and
/// bank proximity. Visible deformation, motion vectors, and WaveQuery import
/// this one implementation.
const RIVER_WAVELENGTHS: vec3<f32> = vec3(13.0, 5.5, 2.3);
const RIVER_BASE_AMPLITUDES: vec3<f32> = vec3(0.16, 0.085, 0.04);

fn river_analytic_displacement(world_xz: vec2<f32>, flow_sample: vec4<f32>, time: f32) -> vec3<f32> {
    let speed = length(flow_sample.xy);
    if speed < 1e-4 {
        return vec3(0.0);
    }
    let direction = flow_sample.xy / speed;
    let bank_fade = clamp(flow_sample.z / max(flow_sample.w, 1e-3), 0.0, 1.0);
    let width_scale = clamp(flow_sample.w * 0.0 + flow_sample.z * 0.06, 0.5, 1.4);
    let speed_scale = clamp(0.5 + 0.35 * speed, 0.5, 1.5);
    let amplitude_scale = bank_fade * width_scale * speed_scale;

    var height = 0.0;
    var slope_x = 0.0;
    var slope_z = 0.0;
    for (var index = 0u; index < 3u; index += 1u) {
        let wavelength = RIVER_WAVELENGTHS[index];
        let k = 6.283185307179586 / wavelength;
        let omega = sqrt(9.80665 * k);
        let amplitude = RIVER_BASE_AMPLITUDES[index] * amplitude_scale;
        // Advect the pattern with the current so waves travel downstream.
        let advected = world_xz - flow_sample.xy * time;
        let phase = k * dot(direction, advected) - omega * time;
        height += amplitude * sin(phase);
        slope_x += amplitude * k * direction.x * cos(phase);
        slope_z += amplitude * k * direction.y * cos(phase);
    }
    return vec3(height, slope_x, slope_z);
}

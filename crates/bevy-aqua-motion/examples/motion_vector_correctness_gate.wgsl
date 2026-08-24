#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> frame: vec4<f32>;
// current/previous ring origin X, stable anchor X, and view width.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> anchor: vec4<f32>;

@fragment
fn fragment(_in: VertexOutput) -> @location(0) vec4<f32> {
    // The physical point is anchored in world space. Camera-follow ring origin
    // changes in anchor.xy are deliberately absent from both positions.
    let current_world_x = anchor.z + frame.z;
    let previous_world_x = anchor.z + frame.w;
    let current_ndc_x = 2.0 * (current_world_x - frame.x) / anchor.w;
    let previous_ndc_x = 2.0 * (previous_world_x - frame.y) / anchor.w;
    let current_ndc = vec2<f32>(current_ndc_x, 0.0);
    let previous_ndc = vec2<f32>(previous_ndc_x, 0.0);

    // Terra and Bevy use current minus previous NDC, transformed to UV space.
    let raw_motion = (current_ndc - previous_ndc) * vec2<f32>(0.5, -0.5);
    let motion = select(raw_motion, vec2(0.0), raw_motion == vec2(0.0));
    return vec4<f32>(motion, 0.0, 1.0);
}

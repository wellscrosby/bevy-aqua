const FFT_RESOLUTION: u32 = 256u;
const FFT_STAGES: u32 = 8u;

@group(0) @binding(0) var source_field: texture_2d_array<f32>;
@group(0) @binding(1) var target_field: texture_storage_2d_array<rgba32float, write>;

var<workgroup> bank_a: array<vec4<f32>, FFT_RESOLUTION>;
var<workgroup> bank_b: array<vec4<f32>, FFT_RESOLUTION>;

fn multiply_pair(value: vec4<f32>, twiddle: vec2<f32>) -> vec4<f32> {
    return vec4(
        value.x * twiddle.x - value.y * twiddle.y,
        value.x * twiddle.y + value.y * twiddle.x,
        value.z * twiddle.x - value.w * twiddle.y,
        value.z * twiddle.y + value.w * twiddle.x,
    );
}

fn butterfly(index: u32, stage: u32, a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    let half_span = 1u << stage;
    let span = 2u * half_span;
    let j = index % half_span;
    let angle = 2.0 * 3.141592653589793 * f32(j) / f32(span);
    let rotated = multiply_pair(b, vec2(cos(angle), sin(angle)));
    return select(a + rotated, a - rotated, (index % span) >= half_span);
}

fn transform(local_index: u32) {
    for (var stage = 0u; stage < FFT_STAGES; stage += 1u) {
        let half_span = 1u << stage;
        let span = 2u * half_span;
        let j = local_index % half_span;
        let base = (local_index / span) * span + j;
        if stage % 2u == 0u {
            bank_b[local_index] = butterfly(local_index, stage, bank_a[base], bank_a[base + half_span]);
        } else {
            bank_a[local_index] = butterfly(local_index, stage, bank_b[base], bank_b[base + half_span]);
        }
        workgroupBarrier();
    }
}

#ifdef FFT_VERTICAL
@compute @workgroup_size(FFT_RESOLUTION, 1, 1)
fn main(
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
) {
    let index = local.x;
    let source = vec2<i32>(i32(group.y), i32(reverseBits(index) >> 24u));
    bank_a[index] = textureLoad(source_field, source, i32(group.z), 0);
    workgroupBarrier();
    transform(index);
    textureStore(target_field, vec2<i32>(i32(group.y), i32(index)), i32(group.z), bank_a[index]);
}
#endif

#ifndef FFT_VERTICAL
@compute @workgroup_size(FFT_RESOLUTION, 1, 1)
fn main(
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
) {
    let index = local.x;
    let source = vec2<i32>(i32(reverseBits(index) >> 24u), i32(group.y));
    bank_a[index] = textureLoad(source_field, source, i32(group.z), 0);
    workgroupBarrier();
    transform(index);
    textureStore(target_field, vec2<i32>(i32(index), i32(group.y)), i32(group.z), bank_a[index]);
}
#endif

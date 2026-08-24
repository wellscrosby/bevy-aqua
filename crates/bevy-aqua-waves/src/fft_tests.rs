use super::*;
use bevy_aqua_core::cascade as lod;

fn shoaling_weights(depth: f32, wave_number: f32) -> (f32, f32) {
    let relative_depth = (depth * wave_number / core::f32::consts::PI).clamp(0.0, 1.0);
    let vertical_base = smoothstep(0.0, 1.0, relative_depth);
    let breaker = 4.0 * vertical_base * (1.0 - vertical_base);
    let vertical = vertical_base * (1.0 + 0.18 * breaker);
    let chop_ratio = mix(0.55, 1.0, smoothstep(0.15, 0.85, vertical_base));
    (vertical * chop_ratio, vertical)
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(low: f32, high: f32, fraction: f32) -> f32 {
    low + (high - low) * fraction
}

#[test]
fn deep_water_shoaling_is_exactly_one_for_every_bin() {
    // The shaders decode "no bed data" (and any depth at or past this value)
    // as NO_BED_DEPTH. At that depth every representative bin wavelength
    // shoals to exactly vec2(1.0), which is what makes the single-bin fast
    // path bit-equivalent to the four-bin path in terrain-free scenes.
    let depth = bevy_aqua_core::bed::NO_BED_DEPTH;
    for lod in 0..LOD_COUNT {
        let texel_width = 4.0 * bevy_aqua_core::lod_scale(lod) / RESOLUTION as f32;
        let max_wavelength = 4.0 * texel_width;
        for bin in 0..ATTENUATION_BINS {
            let octave_fraction = (bin as f32 + 0.5) / ATTENUATION_BINS as f32;
            let representative_wavelength = 0.5 * max_wavelength * octave_fraction.exp2();
            let wave_number = core::f32::consts::TAU / representative_wavelength;
            assert_eq!(
                shoaling_weights(depth, wave_number),
                (1.0, 1.0),
                "lod {lod} bin {bin} wavelength {representative_wavelength}"
            );
        }
    }
}

#[test]
fn active_bins_collapse_only_when_shoaling_cannot_apply() {
    assert_eq!(active_bin_count(0.0, false), 1);
    assert_eq!(active_bin_count(0.95, true), 1);
    assert_eq!(active_bin_count(0.0, true), 1);
    assert_eq!(active_bin_count(0.95, false), ATTENUATION_BINS);
}

#[test]
fn spectrum_authoring_reshapes_h0_deterministically() {
    let layout = lod::GpuLayout::new(&lod::layout(Vec2::ZERO), Vec2::ZERO, 0.0);
    let short_fetch = SpectrumAuthoring {
        fetch: 50_000.0,
        ..SpectrumAuthoring::default()
    };
    // JONSWAP's dimensionless fetch halves, so the field must change
    // while identical inputs stay bit-deterministic.
    let default_h0 = make_h0(&layout, 1.0, &SpectrumAuthoring::default());
    let reshaped = make_h0(&layout, 1.0, &short_fetch);
    let again = make_h0(&layout, 1.0, &short_fetch);
    assert_ne!(default_h0.data, reshaped.data);
    assert_eq!(reshaped.data, again.data);
}

#[test]
fn fft_displacement_bounds_match_deterministic_h0() {
    let layout = lod::GpuLayout::new(&lod::layout(Vec2::ZERO), Vec2::ZERO, 0.0);
    let expected = [252.608_06, 244.313_5, 227.737_76, 194.751_74, 129.361_68];
    let actual = cumulative_height_bounds(&layout, 1.0, &SpectrumAuthoring::default());
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
    }
}

#[test]
fn fft_uniform_mode_flags_offset_is_stable() {
    // One Vec4 of mode flags after the layout and params: x carries the
    // active attenuation-bin count. Layout ends with the two bed vec4s.
    assert_eq!(Uniform::min_size().get(), 272);
    let uniform = Uniform {
        layout: lod::GpuLayout::new(&lod::layout(Vec2::ZERO), Vec2::ZERO, 0.0),
        params: Vec4::new(1.0, 2.0, 3.0, 4.0),
        mode: Vec4::new(5.0, 6.0, 7.0, 8.0),
    };
    let mut bytes = Vec::new();
    bevy::render::render_resource::encase::UniformBuffer::new(&mut bytes)
        .write(&uniform)
        .expect("uniform write");
    let mode_bytes: Vec<u8> = bytes[256..272].to_vec();
    let mode: [f32; 4] = std::array::from_fn(|index| {
        f32::from_le_bytes(mode_bytes[index * 4..index * 4 + 4].try_into().unwrap())
    });
    assert_eq!(mode, [5.0, 6.0, 7.0, 8.0]);
}

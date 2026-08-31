// Closed-form water medium: RGB Beer-Lambert transmittance and in-scatter
// from directional downwelling. Shared by the underwater fullscreen pass
// and the cascade surface body.

#define_import_path aqua::medium

#import bevy_pbr::mesh_view_bindings::{lights, view}

const FRAC_4_PI: f32 = 0.07957747154594767;
const PATH_LENGTH_MAX: f32 = 256.0;
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

fn medium_radiance(
    scene: vec3<f32>,
    rd: vec3<f32>,
    t_end: f32,
    d0: f32,
    sigma_t: vec3<f32>,
    scatter_scale: f32,
    inscatter_scale: f32,
    g: f32,
    sky_fallback: vec3<f32>,
) -> vec3<f32> {
    if t_end < 1e-4 {
        return scene;
    }

    let sigma_s = min(
        sigma_t,
        PARTICLE_SCATTER * max(scatter_scale, 0.0) * SCATTER_SPECTRUM,
    );
    let exposure = view.exposure;
    let weighted = sigma_s * inscatter_scale;

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
        let phase = henyey_greenstein(dot(l_water, rd), g);
        inscatter += weighted * downwelling_integral(
            sigma_t,
            t_end,
            rd.y,
            l_water.y,
            d0,
            e_water * (SUN_BODY + phase),
        );
    }

    let sky = select(
        sky_fallback,
        sun_surface * SKY_FRACTION,
        any(sun_surface > vec3(1e-6)),
    );
    inscatter += weighted * downwelling_integral(
        sigma_t,
        t_end,
        rd.y,
        1.0,
        d0,
        sky,
    );

    let transmittance = exp(-sigma_t * t_end);
    return scene * transmittance + inscatter;
}

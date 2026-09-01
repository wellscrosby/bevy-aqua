use bevy::prelude::*;
use bevy_aqua_core::{AquaSettings, Ocean, ResolvedWaterBody, WaterOptics, WaterShape};

use super::sample_medium;

const N_WATER: f32 = 1.333;

fn refract_air_to_water(l_air: Vec3) -> Option<Vec3> {
    if l_air.y <= 0.0 {
        return None;
    }
    let sin2_air = (1.0 - l_air.y * l_air.y).max(0.0);
    let sin2_water = sin2_air / (N_WATER * N_WATER);
    let cos_water = (1.0 - sin2_water).max(0.0).sqrt();
    let horiz = Vec2::new(l_air.x, l_air.z);
    let horiz_len = horiz.length();
    let xz = if horiz_len > 1e-8 {
        horiz / horiz_len * sin2_water.sqrt()
    } else {
        Vec2::ZERO
    };
    Some(Vec3::new(xz.x, cos_water, xz.y))
}

fn refract_view_into_water(rd_air: Vec3) -> Vec3 {
    let eta = 1.0 / N_WATER;
    let cos_i = (-rd_air.y).clamp(0.0, 1.0);
    let sin2_t = eta * eta * (1.0 - cos_i * cos_i).max(0.0);
    let cos_t = (1.0 - sin2_t).max(0.0).sqrt();
    Vec3::new(rd_air.x * eta, -cos_t, rd_air.z * eta)
}

fn air_ray_water_path(depth: f32, rd_air: Vec3) -> f32 {
    depth / (-rd_air.y).max(0.02)
}

fn downwelling_integral(sigma: f32, t: f32, rd_y: f32, l_y: f32, d0: f32) -> f32 {
    let ly = l_y.max(0.02);
    let i0 = (-sigma * (d0 / ly)).exp();
    let kappa = sigma * (1.0 - rd_y / ly);
    let integral = if kappa.abs() <= 1e-5 {
        t
    } else {
        let optical = (kappa * t).clamp(-80.0, 80.0);
        (1.0 - (-optical).exp()) / kappa
    };
    i0 * integral
}

fn circle_body(
    level: f32,
    center: Vec2,
    radius: f32,
    optics: Option<WaterOptics>,
) -> ResolvedWaterBody {
    ResolvedWaterBody::resolve(
        Entity::from_bits(1),
        &WaterShape::Circle { radius },
        optics,
        &GlobalTransform::from(Transform::from_xyz(center.x, level, center.y)),
    )
    .expect("test body must resolve")
}

#[test]
fn ocean_camera_switches_on_mean_plane() {
    let ocean = Ocean { level: -2.0 };
    let settings = AquaSettings {
        water_optics: WaterOptics::DEEP_OCEAN,
        ..default()
    };
    assert!(sample_medium(Vec3::new(0.0, 20.0, 0.0), Some(&ocean), &settings, &[]).is_none());
    let under = sample_medium(Vec3::new(4.0, -3.0, 1.0), Some(&ocean), &settings, &[]).unwrap();
    assert_eq!(under.0, -2.0);
    assert_eq!(under.1, WaterOptics::DEEP_OCEAN);
    assert!(under.2);
}

#[test]
fn ocean_camera_keeps_the_pass_alive_through_a_crest_margin() {
    let ocean = Ocean { level: 0.0 };
    let settings = AquaSettings::default();
    let under_crest = sample_medium(Vec3::new(0.0, 4.0, 0.0), Some(&ocean), &settings, &[]);
    assert!(under_crest.is_some());
    let well_above = sample_medium(Vec3::new(0.0, 20.0, 0.0), Some(&ocean), &settings, &[]);
    assert!(well_above.is_none());
}

#[test]
fn pond_wins_over_ocean_when_the_camera_is_inside() {
    let ocean = Ocean { level: -2.0 };
    let settings = AquaSettings {
        water_optics: WaterOptics::DEEP_OCEAN,
        ..default()
    };
    let pond = circle_body(
        3.0,
        Vec2::new(10.0, 0.0),
        4.0,
        Some(WaterOptics::CLEAR_FRESH),
    );
    let inside = sample_medium(
        Vec3::new(10.0, 1.0, 0.0),
        Some(&ocean),
        &settings,
        &[pond.clone()],
    )
    .unwrap();
    assert_eq!(inside.0, 3.0);
    assert_eq!(inside.1, WaterOptics::CLEAR_FRESH);
    assert!(!inside.2);

    let beside = sample_medium(Vec3::new(0.0, 20.0, 0.0), Some(&ocean), &settings, &[pond]);
    assert!(beside.is_none());
}

#[test]
fn camera_below_ocean_outside_a_raised_pond_uses_the_ocean() {
    let ocean = Ocean { level: 0.0 };
    let settings = AquaSettings::default();
    let pond = circle_body(8.0, Vec2::ZERO, 3.0, Some(WaterOptics::CLEAR_FRESH));
    let under =
        sample_medium(Vec3::new(20.0, -1.0, 0.0), Some(&ocean), &settings, &[pond]).unwrap();
    assert_eq!(under.0, 0.0);
    assert_eq!(under.1, WaterOptics::DEEP_OCEAN);
    assert!(under.2);
}

#[test]
fn overhead_sun_does_not_refract() {
    let water = refract_air_to_water(Vec3::Y).unwrap();
    assert!(water.distance(Vec3::Y) < 1e-5);
}

#[test]
fn grazing_sun_stays_steeper_than_snell_window() {
    let air = Vec3::new(1.0, 0.05, 0.0).normalize();
    let water = refract_air_to_water(air).unwrap();
    assert!(water.y > 0.65);
    assert!(water.y > air.y);
    assert!(refract_air_to_water(Vec3::new(1.0, -0.1, 0.0)).is_none());
}

#[test]
fn lower_sun_dies_faster_with_depth() {
    let sigma = 0.3;
    let t = 20.0;
    let d0 = 8.0;
    let overhead = downwelling_integral(sigma, t, 0.0, 1.0, d0);
    let low = downwelling_integral(sigma, t, 0.0, 0.66, d0);
    assert!(low < overhead);
}

const RAYLEIGH: Vec3 = Vec3::new(0.00095, 0.00193, 0.00456);

fn particle_scatter(scatter_scale: f32, scatter_tint: Vec3) -> Vec3 {
    scatter_tint.max(Vec3::ZERO) * 0.02 * scatter_scale.max(0.0)
}

fn total_scatter(scatter_scale: f32, scatter_tint: Vec3, extinction: Vec3) -> Vec3 {
    (particle_scatter(scatter_scale, scatter_tint) + RAYLEIGH).min(extinction)
}

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * cos_theta;
    std::f32::consts::FRAC_1_PI * 0.25 * (1.0 - g * g) / (denom * denom.sqrt())
}

fn phase_rayleigh(cos_theta: f32) -> f32 {
    3.0 / (16.0 * std::f32::consts::PI) * (1.0 + cos_theta * cos_theta)
}

fn mixed_phase(cos_theta: f32, scatter_scale: f32, scatter_tint: Vec3, g: f32) -> Vec3 {
    let sigma_p = particle_scatter(scatter_scale, scatter_tint);
    let denom = (sigma_p + RAYLEIGH).max(Vec3::splat(1e-8));
    (sigma_p * henyey_greenstein(cos_theta, g) + RAYLEIGH * phase_rayleigh(cos_theta)) / denom
}

#[test]
fn red_extinction_is_mostly_absorption() {
    let optics = WaterOptics::DEEP_OCEAN;
    let sigma_s = total_scatter(optics.scatter_scale, optics.scatter_tint, optics.extinction);
    let omega = sigma_s / optics.extinction;
    assert!(sigma_s.x < sigma_s.y);
    assert!(sigma_s.y < sigma_s.z);
    assert!(omega.x < 0.05);
    assert!(omega.x < omega.y);
}

#[test]
fn scatter_tint_recolors_particle_haze() {
    let ocean = WaterOptics::DEEP_OCEAN;
    let silt = WaterOptics {
        scatter_tint: Vec3::new(1.2, 1.0, 0.55),
        ..ocean
    };
    let blue = particle_scatter(ocean.scatter_scale, ocean.scatter_tint);
    let yellow = particle_scatter(silt.scatter_scale, silt.scatter_tint);
    assert!(blue.z > blue.x);
    assert!(yellow.x > yellow.z);
}

#[test]
fn backscatter_is_molecular_not_an_isotropic_gain() {
    let optics = WaterOptics::DEEP_OCEAN;
    let back = mixed_phase(
        -1.0,
        optics.scatter_scale,
        optics.scatter_tint,
        optics.scattering_asymmetry,
    );
    let hg_back = henyey_greenstein(-1.0, optics.scattering_asymmetry);
    assert!(back.z > hg_back);
    assert!(back.z < 0.15);
    assert!(back.max_element() < 0.5);
}

#[test]
fn forward_scatter_is_still_henyey_greenstein() {
    let optics = WaterOptics::DEEP_OCEAN;
    let forward = mixed_phase(
        1.0,
        optics.scatter_scale,
        optics.scatter_tint,
        optics.scattering_asymmetry,
    );
    let back = mixed_phase(
        -1.0,
        optics.scatter_scale,
        optics.scatter_tint,
        optics.scattering_asymmetry,
    );
    let hg_forward = henyey_greenstein(1.0, optics.scattering_asymmetry);
    assert!(forward.y > 1.0);
    assert!(forward.y > back.y * 10.0);
    assert!((forward.y - hg_forward).abs() < hg_forward * 0.5);
}

#[test]
fn looking_along_the_light_uses_path_length() {
    let sigma = 0.3;
    let t = 12.0;
    let l_y = 0.8;
    let d0 = 5.0;
    let along = downwelling_integral(sigma, t, l_y, l_y, d0);
    let expected = t * (-sigma * (d0 / l_y)).exp();
    assert!((along - expected).abs() < 1e-4);
}

#[test]
fn water_leaving_keeps_the_air_camera_path() {
    let depth = 2.0;
    let grazing = Vec3::new(0.8, -0.2, 0.0).normalize();
    let t_air = air_ray_water_path(depth, grazing);
    let water = refract_view_into_water(grazing);
    let t_snell = t_air * (-grazing.y) / (-water.y);
    assert!(t_air > 8.0);
    assert!(t_snell < 0.5 * t_air);
}

#[test]
fn water_leaving_divides_haze_by_n_squared() {
    let haze_water = Vec3::splat(1.78);
    let leaving = haze_water / (N_WATER * N_WATER);
    assert!((leaving.x - 1.0).abs() < 0.01);
}

fn fresnel_dielectric(n1: f32, n2: f32, cos_i: f32) -> f32 {
    let eta = n1 / n2;
    let sin2_t = eta * eta * (1.0 - cos_i * cos_i);
    if sin2_t >= 1.0 {
        return 1.0;
    }
    let cos_t = (1.0 - sin2_t).max(0.0).sqrt();
    let rs = (n1 * cos_i - n2 * cos_t) / (n1 * cos_i + n2 * cos_t);
    let rp = (n2 * cos_i - n1 * cos_t) / (n2 * cos_i + n1 * cos_t);
    0.5 * (rs * rs + rp * rp)
}

fn sun_to_water_transmittance(sigma: Vec3, depth: f32, l_air: Vec3) -> Vec3 {
    if l_air.y <= 0.0 || depth <= 0.0 {
        return Vec3::ONE;
    }
    let l_water = refract_air_to_water(l_air).expect("upward sun");
    let path = depth / l_water.y.max(0.02);
    Vec3::splat(1.0 - fresnel_dielectric(1.0, N_WATER, l_air.y)) * (-sigma * path).exp()
}

#[test]
fn above_water_keeps_full_incident_light() {
    let t = sun_to_water_transmittance(WaterOptics::DEEP_OCEAN.extinction, 0.0, Vec3::Y);
    assert_eq!(t, Vec3::ONE);
}

#[test]
fn fifty_metres_of_deep_ocean_is_barely_blue() {
    let t = sun_to_water_transmittance(WaterOptics::DEEP_OCEAN.extinction, 50.0, Vec3::Y);
    assert!(t.x < 1e-6);
    assert!(t.y > 0.005 && t.y < 0.01);
    assert!(t.z > 0.07 && t.z < 0.09);
    assert!(t.z > t.y && t.y > t.x);
}

#[test]
fn low_sun_dies_faster_than_overhead_sun() {
    let sigma = WaterOptics::DEEP_OCEAN.extinction;
    let high = sun_to_water_transmittance(sigma, 20.0, Vec3::Y);
    let low = sun_to_water_transmittance(sigma, 20.0, Vec3::new(0.8, 0.2, 0.0).normalize());
    assert!(low.x < high.x);
    assert!(low.y < high.y);
    assert!(low.z < high.z);
}

#[test]
fn water_to_air_normal_incidence_matches_f0() {
    let f0 = ((N_WATER - 1.0) / (N_WATER + 1.0)).powi(2);
    let fresnel = fresnel_dielectric(N_WATER, 1.0, 1.0);
    assert!((fresnel - f0).abs() < 1e-5);
}

#[test]
fn water_to_air_tirs_beyond_the_critical_angle() {
    let critical = (1.0 / N_WATER).asin();
    let inside = fresnel_dielectric(N_WATER, 1.0, (critical - 0.05).cos());
    let outside = fresnel_dielectric(N_WATER, 1.0, (critical + 0.05).cos());
    assert!(inside < 1.0);
    assert!((outside - 1.0).abs() < 1e-5);
}

#[test]
fn underside_window_multiplies_air_radiance_by_n_squared() {
    let air = Vec3::splat(1.0);
    let in_water = air * (N_WATER * N_WATER);
    let leaving = in_water / (N_WATER * N_WATER);
    assert!((in_water.x - 1.777).abs() < 0.01);
    assert!((leaving.x - 1.0).abs() < 1e-5);
}

# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- A focused native/WebGPU `debug_views` example that automatically cycles
  through every `AquaDebug` diagnostic mode.
- Cascade underside shading: water-to-air interface when looking up from below. Snell's window is air radiance times n² times Fresnel transmittance; TIR and the reflected lobe are the medium integral along the bounce, with a screen-space depth march for on-screen underwater geometry.
- Underwater volume pass: closed-form RGB Beer-Lambert transmittance and directional downwelling (Snell, Fresnel, particle Henyey-Greenstein plus molecular Rayleigh) when the camera is below the local surface. Particle scatter is a weak blue-tilted coefficient times `scatter_scale`, not a copy of extinction. Henyey-Greenstein `g` is `WaterOptics::scattering_asymmetry`. Opaque scene colour is scaled by the surface-to-hit sun path from the depth buffer, so a 50 m mesh in deep-ocean optics is barely lit and blue-green without a special material.

### Changed

- Medium in-scatter comes only from directional lights. The vertical sky downwelling lobe is gone.
- Medium in-scatter is a normalized mix of particle Henyey-Greenstein and molecular Rayleigh. The isotropic `SUN_BODY` phase gain is gone. `WaterOptics` presets use beam extinction near Pope-Fry absorption plus scatter.
- Cascade surface shading uses the same `aqua::medium` integral as the underwater pass, converted to water-leaving radiance along the camera ray that sampled transmission (in-water radiance / n²). The water body is no longer lit as Lambert plastic; foam, Fresnel, and crest SSS stay on the mesh.
- `WaterOptics` now includes Henyey-Greenstein `scattering_asymmetry`. `WaterVolume` and `AquaSettings::volume` are removed; the underwater pass is no longer optional, and the old `inscatter` gain is gone.
- `WaterOptics` no longer carries unused Crest body paints (`deep_color`, `grazing_color`, `shallow_color`). Far-tier water is treated as deep below 7 m.

### Fixed

- Surface body fog uses the Euclidean camera-ray path through the water, matching the unrefracted transmission sample. Snell no longer shortens that path, so a shallow object seen from above veils over distance the same way it does from just below.
- Underwater volume composite keeps cascade underside colour instead of zeroing hits near the mean plane.

## [0.1.3] - 2026-08-30

### Added

- Browser WebGPU/Wasm support across the Aqua render stack, contributed by
  [@wellscrosby](https://github.com/wellscrosby) in
  [#1](https://github.com/sayhisam1/bevy-aqua/pull/1).
- Nine focused, procedural examples that run unchanged on native and Wasm,
  with expected-look screenshots and a hosted WebGPU gallery.
- GitHub Actions workflows for native/Wasm CI, GitHub Pages deployment, and
  ordered crates.io publication of the full workspace.

### Changed

- Replace the configurable showcase and browser-only demo with small examples
  for oceans, spectral waves, foam, bounded water, rivers, terrain beds, water
  optics, planar reflections, and GPU wave queries.
- Centralize packed field-texture layout contracts and strengthen render-pass
  shader variant validation.

### Fixed

- Use a WebGPU-filterable foam storage format and Web-compatible diagnostic
  timestamps.
- Preserve authored mip filtering with derivative-free explicit shader LOD.

## [0.1.2] - 2026-08-24

### Fixed

- Remove the incompatible docs.rs Cargo job override so documentation builds can run with the service-provided job setting.

## [0.1.1] - 2026-08-24

### Fixed

- Limit docs.rs builds to one Cargo job to stay within the documentation sandbox memory limit.
- Document all facade features so optional re-exports appear on the umbrella API page.

## [0.1.0] - 2026-08-24

### Added

- Camera-centred concentric ocean geometry with smooth five-cascade LOD blending.
- Crest-style Gerstner waves and deterministic JONSWAP/Phillips FFT waves with authored sea state, wind, fetch, and world-XZ flow.
- Depth-aware refraction, Beer-Lambert transmission, scene and environment lighting, caustics, detail normals, and reduced-cost far-water shading.
- Persistent whitecap and shoreline foam, terrain bed-height capture, and shallow-water attenuation.
- An ECS-native authoring model: an authoritative optional `Ocean` resource plus bounded `WaterBody` entities with circle, polygon, corridor, and river `WaterShape` components.
- Affine world-XZ placement for bounded water, per-body optics, analytic river deformation, and a shared resolved-body snapshot consumed by rendering and optional integrations.
- Optional GPU `WaveQuery` probes, planar reflections, motion vectors, and budgeted Hanabi spray behind the `query`, `reflect`, `motion`, and `spray` Cargo features.
- Repository showcase scenes for islands, lakes, ponds, rivers, reflections, and open-ocean diagnostics.

### Known limitations

- One active 3D camera is supported.
- Water surfaces must remain horizontal; bounded bodies support affine world-XZ placement but reject tilt.
- No collision or CPU wave query; `WaveQuery` sampling uses GPU readback with about one frame of latency.
- Desktop Vulkan is the verified rendering target.
- The optional local buoy model is not distributed.

### Attribution

- Core ocean architecture and bundled detail/foam textures derive from Crest Ocean System under MIT.
- FFT foam reconstruction and selected lighting and Fresnel mechanisms derive from GodotOceanWaves under MIT.
- See `ATTRIBUTION.md` and the bundled third-party license files.

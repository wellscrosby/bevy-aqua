# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Fullscreen underwater volume composite when the camera is below the local wave surface. The water mesh is hidden and the same `WaterOptics` Beer-Lambert mix lights the scene, including caustics on reconstructed positions. Fog uses only the underwater segment of each view ray.

### Changed

- GPU wave probes always run so the underwater pass can follow local wave height. The `query` feature still re-exports `WaveQuery` and `WaveSurface` from the facade.
- Underwater scatter uses the view environment map and bed-depth body albedo, matching the surface mix endpoint without SSS.

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

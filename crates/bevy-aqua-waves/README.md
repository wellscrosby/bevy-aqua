# bevy-aqua-waves

Wave production for Aqua's AnimWaves cascades: the analytic Crest Gerstner
component bands and the spectral Tessendorf path (JONSWAP h0 via
`bevy-aqua-fft`, Stockham transform, phase evolution, resolve, cascade combine).

## Owns

- `generate_components` + `spectrum_amplitude`: the stratified Crest
  spectrum (deterministic, seed 0).
- The FFT pipeline glue: h0 texture creation from `bevy_aqua_fft::make_h0`,
  field textures, attenuation-bin gating, per-frame uniforms.
- Compute shaders (embedded): `anim_waves*.wgsl`, `fft_evolve/resolve/
  surface.wgsl`, and the surface module `displace.wgsl` published at
  import path `aqua::waves::displace`.
- `Frame` + render-world `Prepared` (the live AnimWaves uniform, shared
  with the query pass through `bevy_aqua_core::AnimWavesUniform`).

## Public API

`AquaWavesPlugin`, plus the umbrella-facing helpers `displacement_bounds`,
`DisplacementBounds`, `StartupAmplitude`, `RenderPrepared`, and the
startup/update systems. Settings live on `bevy_aqua_core::OceanWaves`.

## Test alone

```
cd crates/bevy-aqua-waves && cargo test
```

Isolation example: `cargo run --example waves-only` (core + waves on a
flat ocean).

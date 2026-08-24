# bevy-aqua-foam

Persistent foam for Aqua: a Crest-style simulation cascade (advect with the
current, fade, accumulate from wave crests + shoreline depth) plus the
foam terms the surface material shades through `aqua::foam::shade`.

## Owns

- The sim compute shader (`foam.wgsl`): reprojection across layout changes,
  wave-crest injection, shoreline/bank streaks, fixed-step catch-up.
- `shade.wgsl` (`aqua::foam::shade`): bicubic cascade sampling, breakup
  mask, bubble tint, foam lighting for the composed material.
- `Textures` (double-buffered state + published surface + pattern) and
  the render-world write node ordered after `bevy_aqua_core::AnimWavesWritten`.

## Public API

`AquaFoamPlugin`, `Textures`. Settings live on `bevy_aqua_core::OceanWaves`
(model gate) and the material uniform; this crate adds no config.

## Test alone

```
cd crates/bevy-aqua-foam && cargo test
```

Isolation example: `cargo run --example foam-only` (core + foam on a
synthetic wave input).

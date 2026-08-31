# bevy-aqua-volume

Fullscreen underwater volume for Aqua. When `AquaSettings::volume` is `Some`
and the camera is below the local water surface, a Core3d pass applies RGB
Beer-Lambert transmittance and closed-form in-scatter along the underwater
segment. Particle scatter is a weak, slightly blue coefficient times
`scatter_scale`, clamped below extinction, so red absorption is not
scattered back. Haze colour is the sun and sky after downwelling, not the
cascade body paints.

Ambient downwelling is vertical. Directional lights are refracted at the
surface, then fall off along `depth / L.y`, so sun elevation changes how
fast the water goes dark. Body in-scatter is that sun fill; looking toward
the sun is brighter via Henyey-Greenstein. `WaterVolume::inscatter` scales
the haze. `VolumetricLight` is not used.

The cascade mesh is not changed. The medium is the mean water plane. One
cascade sample at the camera keeps a crest underwater and rejects air.

Above water the pass is skipped. Crossing the surface is a hard cut.

## Owns

- Camera-below-surface detection (`Ocean` or a containing `WaterBody`).
- The fullscreen `volume.wgsl` composite after the main pass.

## Public API

`AquaVolumePlugin`. Hosts should not add this themselves: `AquaPlugin` already
does. Enable the pass with `AquaSettings::volume`.

```
cd crates/bevy-aqua-volume && cargo test
```

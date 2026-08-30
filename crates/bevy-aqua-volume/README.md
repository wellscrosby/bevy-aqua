# bevy-aqua-volume

Fullscreen underwater volume for Aqua. When `AquaSettings::volume` is `Some`
and the camera is below the local water surface, a Core3d pass raymarches the
underwater segment of each view ray.

The pass applies RGB Beer-Lambert transmittance to the scene, then adds
in-scatter from Bevy lights that carry `VolumetricLight` (directional, point,
and spot), sampling their shadow maps. Ambient downwelling falls with depth
so unlit water goes dark instead of tinting toward a constant fog colour.

The cascade mesh is not changed. The ray is clipped to the displaced wave
height sampled from the AnimWaves cascades.

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

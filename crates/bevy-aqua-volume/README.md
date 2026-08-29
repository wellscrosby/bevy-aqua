# bevy-aqua-volume

Fullscreen underwater composite for Aqua. When the active camera is below the
local wave surface, the water mesh is hidden and a Core3d pass applies the same
Beer-Lambert mix as the surface shader, plus caustics on reconstructed scene
positions. Scatter colour uses the view environment map and the same
bed-depth body albedo as the surface mix. Fog uses only the underwater
segment of each view ray, clipped at the mean plane plus a displacement sample
from the AnimWaves cascades.

Above water the pass is skipped and the surface path is unchanged. Crossing the
surface is a hard cut: there is no waterline or partial submersion. Camera
in/out follows `WaveQuery` height at the camera, and the mean plane until that
sample arrives.

## Owns

- Camera-below-surface detection (`Ocean` or a containing `WaterBody`, using
  local wave height when a camera probe is valid).
- Hiding cascade tiles while the camera is underwater.
- The fullscreen `volume.wgsl` composite after the main pass.

## Public API

`AquaVolumePlugin`. Hosts should not add this themselves: `AquaPlugin` already
does. Optics stay on `WaterOptics` / `AquaSettings`.

# aqua-reflect

Planar scene reflections for Aqua. `AquaReflectPlugin` maintains at most two
`Rgba16Float` mirror views for the nearest visible water
levels. Add `ReflectedInWater` to terrain, cloud, and large static-mesh
entities that should appear in the water. Directional lights are included
automatically. Aqua falls back to its environment cubemap outside a mirror
view.

Select `ReflectionMode::Cubemap` for the cubemap-only path, or
`ReflectionMode::Planar { scale, distortion }` through `AquaSettings`.

## GPU budget

Measured on an NVIDIA RTX 3070 at 2560x1440 with the island terrain and
atmosphere, 300 measured frames after 300 warmup frames. The command was
`cargo run --release --example showcase -- --scene island --reflections
<mode> --gpu-profile --resolution 2560x1440 --ui-off`. Values are medians of
three paired runs from Bevy's reported GPU span sum.

| mode | reported span sum (ms) | paired reflection delta (ms) |
| --- | ---: | ---: |
| Cubemap | 2.684 | — |
| Planar, scale 0.5 | 2.889 | **0.194** |

The paired delta includes the mirror camera and the added water-material
sample. It is below Aqua's 1.0 ms reflection budget.

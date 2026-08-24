# aqua-shore

Shoreline and bed support for Aqua. The crate registers water bodies and
bakes a level/slot map and per-texel flow map over their union.

`AquaShorePlugin` resolves each complete `WaterBody` + `WaterShape` entity,
optional sibling `WaterOptics`, and propagated `GlobalTransform` into one stable
world snapshot.
It rebakes only when that snapshot or the authoritative `Ocean` resource
changes. `bake` provides the pure fields-bake implementation.

Body and optics types live in `aqua-core`. The shared
`WaterBodiesResolved` system set lets queries, reflections, spray, and motion
consume the same resolved state.

```sh
cargo test -p aqua-shore
cargo run -p aqua-shore --example shore_ponds
```

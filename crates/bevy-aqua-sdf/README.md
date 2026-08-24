# bevy-aqua-sdf

Signed-shape and river-flow math for Aqua's water fields. `WaterShape` is a
Bevy ECS component; its containment, flow, and extent math is CPU-only and
closed-form.

## Owns

- `WaterShape`: local-space circle / polygon / river / corridor,
  with `contains` and `flow_at` (a `FlowSample`: current, signed bank
  margin, half width, speed).
- `point_in_polygon` (even-odd) and `resolve_extent` (bounding square +
  field-texture AABB with margins).
- `RiverPath`/`RiverPoint`/`RiverSample`: nearest-segment polyline flow
  sampling anywhere in the local XZ plane.

## Public API

`WaterShape`, `FlowSample`, `RiverPath`, `RiverPoint`, `RiverSample`,
`point_in_polygon`, `resolve_extent`.

## Test alone

```
cd crates/bevy-aqua-sdf && cargo test
```

Consumers: the fields bake (`bevy-aqua-shore`) samples these per texel;
wave-query probe resolution matches bodies through them.

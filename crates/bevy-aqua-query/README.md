# bevy-aqua-query

GPU-sampled water surface queries for gameplay such as buoyancy and floating
props. Ocean, pond, and river probes share one compute dispatch and async
buffer readback. Results arrive on each entity's `WaveSurface` with roughly
one frame of latency.

At most 256 probes are submitted each frame. Extra entities retain their
previous sample.

## Public API

`AquaQueryPlugin`, `WaveQuery`, `WaveSurface`.

## Example

```sh
cargo run -p bevy-aqua-query --example query_buoy
```

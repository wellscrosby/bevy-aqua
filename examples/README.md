# Examples

Every example is a fixed visual scene with no command-line options or external
assets. Change only the example name in the commands below.

| Name | Visual focus |
|---|---|
| `ocean` | Analytic waves over a visible seabed |
| `spectral_waves` | FFT waves moving past scale-marker posts |
| `foam` | Whitecaps on a rough ocean |
| `bounded_water` | A circular pool and polygonal pool without an ocean |
| `river` | A curved fresh-water corridor with varying flow |
| `terrain_bed` | Waves and shallow-water shading around an island |
| `water_optics` | Three water-optics presets under identical lighting |
| `planar_reflection` | A bright procedural marker mirrored in calm water |
| `wave_query` | A procedural buoy following GPU wave samples |

## Native

```sh
cargo run --example ocean
```

## Browser WebGPU

Install the target and runner once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-server-runner
```

Then run the same example source in a browser:

```sh
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-server-runner \
  cargo run --target wasm32-unknown-unknown --example ocean
```

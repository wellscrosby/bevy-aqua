# aqua-fft

Pure ocean-spectrum math for Aqua's FFT wave backend. No Bevy, no GPU:
`spectrum models -> h0 generation -> normalisation -> displacement bounds`,
plus the Stockham transform WGSL the render layer submits verbatim.

## Owns

- `SpectralBin`/`spectral_bin`: JONSWAP bins per cascade texel.
- `make_h0`: deterministic Gaussian-seeded h0 coefficient bytes
  (`H0Field`, RGBA32-float layout) normalised to a target RMS height.
- `cumulative_height_bounds`: phase-independent Fourier L1 envelope used to
  expand render bounds.
- `STOCKHAM_WGSL`: the Stockham compute shader source (`horizontal` /
  `vertical`).
- `inverse_radix_two`: the CPU reference inverse transform.

## Test alone

```
cd crates/aqua-fft && cargo test
```

The energy-validation test compares generated h0 with analytic energy per cascade.

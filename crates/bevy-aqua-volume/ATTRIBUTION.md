# Third-party attribution

## Crest Ocean System

Aqua's underwater volume pass reimplements the fullscreen Beer-Lambert composite
from Crest's underwater renderer for Bevy. The medium coefficients and scatter
endpoint are the same `WaterOptics` values the surface shader already uses.

Crest references include `Shaders/Underwater/UnderwaterEffect.hlsl`,
`Shaders/Underwater/UnderwaterEffectShared.hlsl`, and
`Shaders/OceanEmission.hlsl`.

Crest is used under the following MIT License:

```text
MIT License

Copyright (c) 2019 Wave Harmonic and contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Package license

Aqua packages are distributed under `MIT OR Apache-2.0`; see `LICENSE-MIT` and
`LICENSE-APACHE`.

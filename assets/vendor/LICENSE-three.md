# Vendored three.js

The STL preview loads three.js from this directory; there is no CDN and no
runtime network access (ADR-0002).

| File | Source | sha256 |
| --- | --- | --- |
| `three.module.min.js` | https://cdn.jsdelivr.net/npm/three@0.185.0/build/three.module.min.js | `86bcee248b64f44bcfc23c331ae74619061957d59cab040171dcb6fb5900beb6` |
| `three.core.min.js` | https://cdn.jsdelivr.net/npm/three@0.185.0/build/three.core.min.js | `0e9dd2793e01d0d9eb4f2ab00b4ffcdd4488275ebebee5c31fa8d347bc29f0bf` |
| `STLLoader.js` | https://cdn.jsdelivr.net/npm/three@0.185.0/examples/jsm/loaders/STLLoader.js | `023ed97f848b633d8bcd53d4db3b996d29d0c644088700691297c552257d480b` |

- Release: three.js **0.185.0** (npm `three@0.185.0`), fetched 2026-10-18.
- `three.module.min.js` is jsDelivr's pinned Terser minification of
  `build/three.module.js` from that release; it imports `./three.core.min.js`
  (the release's module split), which is vendored beside it.
- `STLLoader.js` is the release's `examples/jsm/loaders/STLLoader.js`; it
  imports the bare specifier `three`, which `index.html` resolves through its
  import map.
- License: MIT, as shipped with the release
  (https://cdn.jsdelivr.net/npm/three@0.185.0/LICENSE). Text below.

```text
The MIT License

Copyright © 2010-2026 three.js authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

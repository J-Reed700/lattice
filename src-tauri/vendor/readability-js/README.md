# Vendored `readability-js`

This is a minimal vendored copy of [`readability-js` 0.1.5](https://crates.io/crates/readability-js/0.1.5), from upstream commit [`868010b737188060edda6dfbcfb7b89c7d8b41c1`](https://github.com/egemengol/readability-js/tree/868010b737188060edda6dfbcfb7b89c7d8b41c1/core). The crate's Rust wrapper, bundled JavaScript, and Mozilla `Readability.js` source are retained unchanged.

The sole manifest dependency adjustment pins `rquickjs` to `=0.14.0`, matching the desktop application's direct QuickJS dependency. This avoids linking a second `rquickjs-sys`/QuickJS static library version into the app. Other dependency declarations are equivalent to upstream's published 0.1.5 manifest.

`LICENSE` contains the upstream crate license. The bundled JavaScript also incorporates `linkedom` 0.18.12; its ISC license is in `LICENSE-LINKEDOM`. Mozilla's original `Readability.js` Apache-2.0 license is in `LICENSE-MOZILLA-READABILITY` and is preserved in that file's source header.

No upstream behavior or algorithm changes are intended.

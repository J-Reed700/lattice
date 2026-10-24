# Lattice Website (`lattice.lambdaaudio.com`)

Static marketing, documentation, and installer download site for **Lattice**, hosted under Lambda Audio LLC at <https://lattice.lambdaaudio.com/>.

## Structure

- `public/index.html` — Single-page site (overview, interactive 4-screen showcase, capabilities, privacy boundary, macOS `.dmg` pre-release download, and source build instructions).
- `public/styles.css` — Editorial layout with Lattice's warm paper, ink, and copper (`#9b4431` / `#dfb091`) palette and self-hosted variable WOFF2 fonts (`Inter`, `Source Serif 4 Italic`, `JetBrains Mono`).
- `public/script.js` — Progressive-enhancement view switcher and `<dialog>` screenshot lightbox.
- `downloads.json` — Explicit installer allowlist with verified byte count and SHA-256 digest.
- `deploy/` — Read-only Alpine Nginx container (`lattice-site:8080`), Compose definition on the `pangolin` network, deterministic release packager (`package_site.py`), and IONOS/Pangolin/Cloudflare runbook (`deploy/README.md`).

## Local preview

```sh
python3 -m http.server 8090 --directory website/public
```

Then open <http://127.0.0.1:8090/>.

## Packaging & deployment

See [`deploy/README.md`](deploy/README.md) for packaging with `website/deploy/package_site.py` and deploying to the IONOS server behind Pangolin.

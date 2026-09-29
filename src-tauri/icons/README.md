# App Icons

The Lattice icon is a 4x4 node lattice whose left column and bottom row are
lit, tracing an L. It is drawn procedurally by
`scripts/generate-app-icon.mjs`. Do not edit the PNG/ICNS/ICO files by hand.

To change the icon, edit the script, then regenerate:

```bash
node scripts/generate-app-icon.mjs          # writes icons/icon.svg + public/favicon.svg
npx tauri icon src-tauri/icons/icon.svg -o src-tauri/icons
rm -rf src-tauri/icons/android src-tauri/icons/ios   # desktop-only app
sips -z 512 512 src-tauri/icons/icon.png --out src-tauri/icons/512x512.png
```

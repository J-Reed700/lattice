#!/usr/bin/env node
// Generates the Lattice app icon: a 4x4 node lattice whose left column and
// bottom row are lit, tracing an L. Pure geometry, no raster inputs. Writes
// src-tauri/icons/icon.svg (bundle source) and public/favicon.svg; see
// src-tauri/icons/README.md for the rest of the pipeline.
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const S = 1024;
const C = S / 2;
const N = 4;
const ACCENT = 'hsl(12 52% 40%)';
const ACCENT_LIGHT = 'hsl(22 58% 72%)';
const INK = 'hsl(30 8% 9%)';

// `gap` is the node pitch; every other size is a fraction of it so the
// favicon variant scales as one piece.
function litL({ gap, tile }) {
  const o = C - (gap * (N - 1)) / 2;
  const at = (i, j) => [o + i * gap, o + j * gap];
  const lit = (i, j) => i === 0 || j === N - 1;
  const parts = [tile];

  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const [x, y] = at(i, j);
      const stroke = `stroke="${ACCENT}" stroke-opacity=".35" stroke-width="${gap * 0.063}"`;
      if (i < N - 1) parts.push(`<line x1="${x}" y1="${y}" x2="${at(i + 1, j)[0]}" y2="${y}" ${stroke}/>`);
      if (j < N - 1) parts.push(`<line x1="${x}" y1="${y}" x2="${x}" y2="${at(i, j + 1)[1]}" ${stroke}/>`);
    }
  }

  const [ax, ay] = at(0, 0);
  const [bx, by] = at(0, N - 1);
  const [cx] = at(N - 1, N - 1);
  parts.push(
    `<polyline points="${ax},${ay} ${bx},${by} ${cx},${by}" fill="none" stroke="${ACCENT_LIGHT}" stroke-width="${gap * 0.23}" stroke-linecap="round" stroke-linejoin="round"/>`,
  );

  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const [x, y] = at(i, j);
      parts.push(
        lit(i, j)
          ? `<circle cx="${x}" cy="${y}" r="${gap * 0.26}" fill="${ACCENT_LIGHT}"/>`
          : `<circle cx="${x}" cy="${y}" r="${gap * 0.14}" fill="${ACCENT}"/>`,
      );
    }
  }
  return parts.join('');
}

const svg = (body) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${S} ${S}">${body}</svg>\n`;

// App icon: macOS grid (824px body inside 1024, ~22% corner radius).
const appIcon = svg(litL({ gap: 150, tile: `<rect x="100" y="100" width="824" height="824" rx="185" fill="${INK}"/>` }));
// Favicon: full-bleed tile, lattice scaled up to stay legible at 16px.
const favicon = svg(litL({ gap: 250, tile: `<rect width="${S}" height="${S}" rx="200" fill="${INK}"/>` }));

fs.writeFileSync(path.join(ROOT, 'src-tauri/icons/icon.svg'), appIcon);
fs.writeFileSync(path.join(ROOT, 'public/favicon.svg'), favicon);
console.log('wrote src-tauri/icons/icon.svg, public/favicon.svg');

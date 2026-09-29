#!/usr/bin/env node
import { gzipSync } from 'node:zlib';
import { readFileSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import process from 'node:process';

const outDir = resolve(process.argv[2] ?? 'dist');
const maxBytes = Number(process.env.INITIAL_JS_BUDGET_BYTES ?? 1_050_000);
const manifest = JSON.parse(readFileSync(join(outDir, '.vite/manifest.json'), 'utf8'));
if (!manifest['index.html']) throw new Error(`No index.html entry in ${outDir}/.vite/manifest.json`);

const visited = new Set();
function visit(key) {
  if (visited.has(key)) return;
  const entry = manifest[key];
  if (!entry) throw new Error(`Missing manifest entry: ${key}`);
  visited.add(key);
  for (const imported of entry.imports ?? []) visit(imported);
}
visit('index.html');

const jsFiles = [...visited]
  .map((key) => manifest[key].file)
  .filter((file) => file.endsWith('.js'));
const totalBytes = jsFiles.reduce((total, file) => total + statSync(join(outDir, file)).size, 0);
const gzipBytes = jsFiles.reduce((total, file) => total + gzipSync(readFileSync(join(outDir, file))).byteLength, 0);
process.stdout.write(`Initial route JavaScript: ${totalBytes.toLocaleString()} bytes (${gzipBytes.toLocaleString()} gzip), ${jsFiles.length} files\n`);
if (totalBytes > maxBytes) {
  process.stderr.write(`Initial JavaScript exceeds the ${maxBytes.toLocaleString()} byte budget.\n`);
  process.exitCode = 1;
}

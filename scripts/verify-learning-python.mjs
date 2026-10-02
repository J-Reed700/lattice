import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const bundle = path.resolve(process.argv[2] || path.join(repository, 'src-tauri/resources/learning-python'));
const lock = JSON.parse(await readFile(path.join(repository, 'scripts/learning-python.lock.json'), 'utf8'));
const manifest = JSON.parse(await readFile(path.join(bundle, 'runtime-manifest.json'), 'utf8'));
assert.equal(manifest.pythonVersion, lock.python.version, 'Bundled Python differs from the pinned source release');
for (const [file, expected] of [
  ['python.wasm', manifest.wasmSha256],
  ['lib/python314.zip', manifest.stdlibSha256],
]) {
  assert.match(expected, /^[0-9a-f]{64}$/, `Invalid checksum for ${file}`);
  const content = await readFile(path.join(bundle, file));
  assert.equal(createHash('sha256').update(content).digest('hex'), expected, `Bundled ${file} failed verification`);
  if (file.endsWith('.wasm')) {
    assert.deepEqual([...content.subarray(0, 8)], [0, 97, 115, 109, 1, 0, 0, 0], 'Python is not a WebAssembly module');
  }
}
await readFile(path.join(bundle, 'LICENSE'));
console.log(`Verified bundled CPython ${manifest.pythonVersion}: interpreter, standard library and license.`);

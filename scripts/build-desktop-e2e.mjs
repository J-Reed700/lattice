import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { access, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'e2e-results', 'desktop');
const release = process.argv.includes('--release');
const platform = process.platform;
if (!['darwin', 'linux', 'win32'].includes(platform)) throw new Error('Unsupported desktop platform');
await mkdir(output, { recursive: true });
const config = JSON.parse(await readFile(path.join(root, 'src-tauri/tauri.e2e.conf.json'), 'utf8'));
const candidateToken = `t${randomUUID().replaceAll('-', '')}`;
config.identifier = `tech.lattice.compatibility.${candidateToken}`;
// Tauri's NSIS bundle chooses its own current-user install location. Give the
// candidate a unique product name so that default location is isolated and we
// test a real Unicode-and-spaces installation path on Windows.
if (platform === 'win32') config.productName = `Lattice Compatibility café ${candidateToken}`;
const configPath = path.join(output, 'config.json');
await writeFile(configPath, JSON.stringify(config, null, 2));
const env = { ...process.env };
// Never replace the ordinary development executable with a test-identity
// binary. A caller may explicitly select a dedicated target directory.
env.CARGO_TARGET_DIR = path.resolve(root, process.env.CARGO_TARGET_DIR || 'src-tauri/target/desktop-e2e');
const pathKey = Object.keys(env).find(key => key.toUpperCase() === 'PATH') || 'PATH';
env[pathKey] = `${path.join(root, 'node_modules', '.bin')}${path.delimiter}${env[pathKey] || ''}`;
// Limit local debug build disk use; CI can select the normal release profile.
if (!release) {
  env.CARGO_PROFILE_DEV_DEBUG = '0';
  env.CARGO_PROFILE_DEV_SPLIT_DEBUGINFO = 'off';
  env.CARGO_PROFILE_DEV_INCREMENTAL = 'false';
  if (platform === 'darwin' && process.arch === 'x64') env.LATTICE_ALLOW_UNPINNED_SIDECAR = '1';
}
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}`);
}
function capture(command, args) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}: ${result.stderr}`);
  return result.stdout;
}
async function assertMissing(file) {
  try {
    await access(file);
  } catch (error) {
    if (error?.code === 'ENOENT') return;
    throw error;
  }
  throw new Error(`Developer-only binary was bundled: ${file}`);
}

// Tauri discovers every Cargo binary and puts each eligible target in the
// installer. Keep the binding generator behind an opt-in feature so it cannot
// be copied into a production or compatibility bundle.
const cargoMetadata = JSON.parse(capture('cargo', [
  'metadata', '--no-deps', '--format-version', '1', '--manifest-path',
  path.join(root, 'src-tauri', 'Cargo.toml'),
]));
const desktopPackage = cargoMetadata.packages.find(pkg => pkg.name === 'lattice-desktop');
const bindingsTarget = desktopPackage?.targets.find(target => target.name === 'export_bindings');
if (!bindingsTarget?.['required-features']?.includes('bindings-export')
    || desktopPackage.features.default?.includes('bindings-export')) {
  throw new Error('export_bindings must require the non-default bindings-export feature');
}
const bundle = { darwin: 'app', linux: 'deb', win32: 'nsis' }[platform];
run(process.execPath, [path.join(root, 'scripts/verify-learning-python.mjs')]);
run(process.execPath, [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'build',
  ...(!release ? ['--debug'] : []), '--ci', '--features', 'desktop-e2e', '--config', configPath,
  '--bundles', bundle]);
const target = env.CARGO_TARGET_DIR;
const bundles = path.join(target, release ? 'release' : 'debug', 'bundle');
// Keep each installed candidate separate, including spaces and Unicode in its path.
let installed = path.join(output, `Installed apps café ${candidateToken}`);
if (platform === 'win32') {
  if (!process.env.LOCALAPPDATA) throw new Error('LOCALAPPDATA is required for a current-user NSIS install');
  installed = path.join(process.env.LOCALAPPDATA, config.productName);
}
await mkdir(installed, { recursive: true });
let binary;
if (platform === 'darwin') {
  const name = `${config.productName}.app`;
  const app = path.join(installed, name);
  run('ditto', [path.join(bundles, 'macos', name), app]);
  run('codesign', ['--verify', '--deep', '--strict', app]);
  binary = path.join(app, 'Contents', 'MacOS', 'lattice-desktop');
} else {
  const directory = path.join(bundles, bundle);
  const extension = platform === 'win32' ? '.exe' : '.deb';
  const packages = (await readdir(directory)).filter(name => name.endsWith(extension));
  if (packages.length !== 1) throw new Error(`Expected one ${bundle} package, found ${packages.length}`);
  const installer = path.join(directory, packages[0]);
  if (platform === 'win32') {
    run(installer, ['/S']);
    binary = path.join(installed, 'lattice-desktop.exe');
  } else {
    // Extract exactly the Debian package contents without changing the host's installed apps.
    run('dpkg-deb', ['--extract', installer, installed]);
    binary = path.join(installed, 'usr', 'bin', 'lattice-desktop');
  }
}
const fixture = path.join(installed, 'sample notes café.md');
const sidecar = path.join(path.dirname(binary), platform === 'darwin' ? 'llama-server'
  : platform === 'win32' ? 'llama-server-cpu.exe' : 'llama-server-cpu');
const bindingsBinary = path.join(path.dirname(binary), platform === 'win32'
  ? 'export_bindings.exe' : 'export_bindings');
await access(binary);
await access(sidecar);
await assertMissing(bindingsBinary);
const pythonManifests = (await readdir(installed, { recursive: true }))
  .filter(file => file.split(path.sep).join('/').endsWith('resources/learning-python/runtime-manifest.json'));
if (pythonManifests.length !== 1) throw new Error(`Expected one bundled Python runtime, found ${pythonManifests.length}`);
const pythonRuntime = path.dirname(path.join(installed, pythonManifests[0]));
run(process.execPath, [path.join(root, 'scripts/verify-learning-python.mjs'), pythonRuntime]);
// Unit test executables are unsigned. This probe runs the actual installed,
// signed application so macOS executable-memory policy and packaged paths
// cannot go untested behind a green source-build test.
const runtimeProbe = spawnSync(binary, ['--learning-runtime-self-test', pythonRuntime], {
  cwd: installed, env, encoding: 'utf8', timeout: 180_000,
});
if (runtimeProbe.error) throw runtimeProbe.error;
if (runtimeProbe.status !== 0) throw new Error(`Installed runtime probe failed (status ${runtimeProbe.status}, signal ${runtimeProbe.signal}): ${runtimeProbe.stderr || runtimeProbe.stdout}`);
const runtimeProof = JSON.parse(runtimeProbe.stdout.trim());
if (runtimeProof.status !== 'passed') throw new Error('Installed runtime probe did not pass');
await mkdir(path.join(output, 'reports'), { recursive: true });
await writeFile(path.join(output, 'reports', 'embedded-runtime.json'), `${JSON.stringify({
  ...runtimeProof, identifier: config.identifier, binary, pythonRuntime,
  platform, arch: process.arch, profile: release ? 'release' : 'debug',
}, null, 2)}\n`);
await writeFile(fixture, '# Compatibility fixture\n\nThe Cedar observatory opens at 08:40 on Tuesday. CEDAR-7319.\n');
await writeFile(path.join(output, 'manifest.json'), JSON.stringify({
  identifier: config.identifier, binary, sidecar, pythonRuntime, fixture, platform, arch: process.arch,
  profile: release ? 'release' : 'debug', installed,
}, null, 2));
console.log(`Desktop candidate ready: ${binary}`);

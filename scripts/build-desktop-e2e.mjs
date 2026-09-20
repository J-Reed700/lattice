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
config.identifier = `tech.lattice.compatibility.t${randomUUID().replaceAll('-', '')}`;
const configPath = path.join(output, 'config.json');
await writeFile(configPath, JSON.stringify(config, null, 2));
const env = { ...process.env };
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
const bundle = { darwin: 'app', linux: 'deb', win32: 'nsis' }[platform];
run(process.execPath, [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'build',
  ...(!release ? ['--debug'] : []), '--ci', '--features', 'desktop-e2e', '--config', configPath,
  '--bundles', bundle]);
const target = path.resolve(root, process.env.CARGO_TARGET_DIR || 'src-tauri/target');
const bundles = path.join(target, release ? 'release' : 'debug', 'bundle');
// Keep each installed candidate separate, including spaces and Unicode in its path.
const installed = path.join(output, `Installed apps café ${config.identifier.split('.').at(-1)}`);
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
    run(installer, ['/S', `/D=${installed}`]);
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
await access(binary);
await access(sidecar);
await writeFile(fixture, '# Compatibility fixture\n\nThe Cedar observatory opens at 08:40 on Tuesday. CEDAR-7319.\n');
await writeFile(path.join(output, 'manifest.json'), JSON.stringify({
  identifier: config.identifier, binary, sidecar, fixture, platform, arch: process.arch,
  profile: release ? 'release' : 'debug', installed,
}, null, 2));
console.log(`Desktop candidate ready: ${binary}`);

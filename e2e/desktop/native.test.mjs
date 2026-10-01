import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { closeSync, openSync } from 'node:fs';
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { test } from 'node:test';
import { remote } from 'webdriverio';
import { sampleProcessTree, scanNativeLeaks } from './resources.mjs';

const output = path.resolve('e2e-results/desktop');
const reports = path.join(output, 'reports', String(Date.now()));
const manifest = JSON.parse(await readFile(path.join(output, 'manifest.json'), 'utf8'));
assert.match(manifest.identifier, /^tech\.lattice\.compatibility\.t[a-f0-9]{32}$/);
await mkdir(reports, { recursive: true });
const results = { ...manifest, os: os.release(), checks: [], passed: false };
let app;
let browser;
let launches = 0;
const expectedTitle = 'Compatibility café 7319';
const expectedBody = 'Cedar compatibility café 7319. This must survive closing immediately.';
let collectionId;

async function invoke(command, args = {}) {
  const response = await browser.executeAsync((command, args, done) => {
    window.__TAURI__.core.invoke(command, args)
      .then(value => done({ ok: true, value }))
      .catch(error => done({ ok: false, failure: error }));
  }, command, args);
  assert.equal(response.ok, true, `${command}: ${JSON.stringify(response.failure)}`);
  return response.value;
}

async function launch() {
  assert.ok(!app, 'The previous app process must be closed before relaunch');
  if (process.platform === 'darwin') {
    const session = execFileSync('/usr/sbin/ioreg', ['-n', 'Root', '-d1', '-l'], {
      encoding: 'utf8', timeout: 10_000,
    });
    assert.equal(/"CGSSessionScreenIsLocked"\s*=\s*Yes/.test(session), false,
      'Native UI checks require an unlocked macOS desktop; locked WebKit views pause their animation clocks');
  }
  const socket = net.createServer();
  await new Promise(resolve => socket.listen(0, '127.0.0.1', resolve));
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const log = openSync(path.join(reports, `app-${++launches}.log`), 'w');
  app = spawn(manifest.binary, [], {
    cwd: path.dirname(manifest.binary),
    env: { ...process.env, TAURI_WEBDRIVER_PORT: String(port),
      ...(process.platform === 'darwin' && process.env.LATTICE_NATIVE_LEAK_SCAN === '1'
        ? { MallocStackLogging: '1' } : {}),
    },
    stdio: ['ignore', log, log],
  });
  closeSync(log);
  let spawnError;
  app.on('error', error => { spawnError = error; });
  const deadline = Date.now() + 90_000;
  while (true) {
    if (spawnError) throw spawnError;
    assert.equal(app.exitCode, null, 'Native app exited before WebDriver became ready');
    try {
      const status = await fetch(`http://127.0.0.1:${port}/status`, { signal: AbortSignal.timeout(1000) });
      if (status.ok) break;
    } catch { /* startup has not bound the port yet */ }
    assert.ok(Date.now() < deadline, 'Native startup timed out');
    await delay(100);
  }
  browser = await remote({
    hostname: '127.0.0.1', port, logLevel: 'error',
    connectionRetryCount: 0,
    connectionRetryTimeout: process.env.LATTICE_NATIVE_LEAK_SCAN === '1' ? 45_000 : 15_000,
    capabilities: { browserName: 'wry', 'wdio:tauriServiceOptions': { windowLabel: 'main' } },
  });
  await browser.setTimeout({ script: 30_000, implicit: 0 });
  await browser.waitUntil(async () => browser.execute(() => Boolean(window.__TAURI__?.core)), { timeout: 30_000 });
  assert.equal(await invoke('plugin:app|identifier'), manifest.identifier);
  const titled = await browser.executeAsync(done => {
    const candidate = window.__TAURI__.window.getCurrentWindow();
    candidate.setTitle('Lattice Compatibility Test — isolated test library')
      .then(() => candidate.show())
      .then(() => candidate.setFocus())
      .then(() => done(true)).catch(error => done(String(error)));
  });
  assert.equal(titled, true, 'The isolated test window must identify itself and receive focus');
  await browser.waitUntil(async () => browser.execute(() => document.visibilityState === 'visible'), {
    timeout: 10_000, timeoutMsg: 'The native candidate webview must be visible before UI interaction',
  });
}

async function closeNormally() {
  const closing = app;
  const descendants = (await sampleProcessTree(closing.pid)).processes.filter(row => row.pid !== closing.pid);
  // Exercise the real native close event and renderer save gate, without
  // clicking another element and accidentally committing a focused title first.
  await browser.execute(() => {
    setTimeout(() => window.__TAURI__.window.getCurrentWindow().close(), 20);
  });
  const deadline = Date.now() + 30_000;
  while (closing.exitCode === null && closing.signalCode === null && Date.now() < deadline) await delay(100);
  assert.equal(closing.exitCode, 0, 'Normal native close must exit successfully after flushing saves');
  const shutdownLog = await readFile(path.join(reports, `app-${launches}.log`), 'utf8');
  assert.doesNotMatch(shutdownLog, /Graceful shutdown timed out|Database close timed out|forcing exit/i,
    'A forced shutdown must not count as a successful lifecycle check');
  assert.doesNotMatch(shutdownLog, /Reaped orphan llama-server process from previous Lattice run/,
    'A clean preceding close must not leave sidecars for the next launch to reap');
  const isAlive = pid => {
    try { process.kill(pid, 0); return true; } catch (error) {
      if (error.code === 'ESRCH') return false;
      throw error;
    }
  };
  const childDeadline = Date.now() + 3000;
  while (descendants.some(row => isAlive(row.pid)) && Date.now() < childDeadline) await delay(100);
  assert.deepEqual(descendants.filter(row => isAlive(row.pid)).map(row => row.pid), [],
    'Native child processes must exit with their owner');
  app = undefined;
  browser = undefined;
}

test('packaged desktop: onboarding, native file access, editing, close and persistence', { timeout: 480_000 }, async t => {
  // Re-running a candidate starts with an empty test library. Preserve the old
  // directory for debugging; the strict identity check excludes real user data.
  const dataRoot = process.platform === 'darwin' ? path.join(os.homedir(), 'Library/Application Support')
    : process.platform === 'win32' ? process.env.APPDATA
      : process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local/share');
  assert.ok(dataRoot, 'Cannot locate the test application data directory');
  const dataDir = path.join(dataRoot, manifest.identifier);
  await rename(dataDir, `${dataDir}.previous-${Date.now()}`).catch(error => {
    if (error.code !== 'ENOENT') throw error;
  });
  const step = async (name, action) => {
    let failure;
    await t.test(name, async () => {
      try { await action(); } catch (error) { failure = error; throw error; }
    });
    if (failure) throw failure;
  };
  try {
    await launch();
    await step('fresh startup and persisted onboarding dismissal', async () => {
      const skip = await browser.$('button=Not now');
      await skip.waitForDisplayed({ timeout: 60_000 });
      await skip.click();
      await browser.waitUntil(async () => (await invoke('plugin:settings|get_settings')).onboarding.firstRunDismissed, { timeout: 15_000 });
      results.checks.push('onboarding');
    });
    await step('real file reads support spaces and Unicode; missing files reject cleanly', async () => {
      const dataDir = await browser.executeAsync(done => window.__TAURI__.path.appDataDir().then(done));
      assert.ok(dataDir.includes(manifest.identifier), 'Fixture must live in this isolated test library');
      const fixture = path.join(dataDir, 'sample notes café.md');
      await writeFile(fixture, await readFile(manifest.fixture));
      const content = await invoke('plugin:file|read_file_content', { path: fixture });
      assert.match(content, /CEDAR-7319/);
      await assert.rejects(() => invoke('plugin:file|read_file_content', { path: `${fixture}.missing` }));
      // A failed operation must not take down the backend.
      assert.ok(await invoke('plugin:settings|get_settings'));
      results.checks.push('native-file-access');
    });
    await step('journal UI works at the minimum supported window size', async () => {
      await browser.setWindowSize(800, 600);
      const journal = await browser.$('button[aria-label="Journal"]');
      await journal.click();
      const create = await browser.$('button=New journal');
      const title = await browser.$('textarea[aria-label="Page title"]');
      await browser.waitUntil(async () => (await title.isExisting()) || (await create.isExisting()), { timeout: 30_000 });
      if (!(await title.isExisting())) await create.click();
      await title.waitForDisplayed({ timeout: 30_000 });
      await (await browser.$('.ProseMirror[contenteditable="true"]')).waitForDisplayed({ timeout: 15_000 });
      const writingWidth = await title.getSize('width');
      assert.ok(writingWidth >= 240, `Journal writing column is only ${writingWidth}px wide`);
      const showContext = await browser.$('button[aria-label="Show conversation and highlights"]');
      await showContext.waitForDisplayed({ timeout: 15_000 });
      await showContext.click();
      const contextRail = await browser.$('aside[aria-label="Beside this page"]');
      await contextRail.waitForDisplayed({ timeout: 15_000 });
      assert.equal(await title.getSize('width'), writingWidth, 'Compact context rail must overlay rather than crush the page');
      await (await browser.$('button[aria-label="Hide panel"]')).click();
      await contextRail.waitForDisplayed({ timeout: 15_000, reverse: true });
      await browser.saveScreenshot(path.join(reports, 'journal-small-window.png'));
      results.checks.push('minimum-window-journal');
    });
    await step('quit flushes pending body edits and the still-focused title', async () => {
      collectionId = await invoke('plugin:file|create_custom_collection', { request: {
        name: 'Compatibility collection', kind: 'manual', parentId: null, documentIds: [],
      } });
      const editor = await browser.$('.ProseMirror[contenteditable="true"]');
      await editor.click();
      await editor.addValue(expectedBody);
      const title = await browser.$('textarea[aria-label="Page title"]');
      await title.setValue(expectedTitle);
      assert.equal(await browser.execute(() => document.activeElement?.getAttribute('aria-label')), 'Page title');
      await closeNormally();
      results.checks.push('native-close');
    });
    await step('reopening preserves settings and the exact saved journal content', async () => {
      await launch();
      assert.equal((await invoke('plugin:settings|get_settings')).onboarding.firstRunDismissed, true);
      const collections = await invoke('plugin:file|list_custom_collections');
      assert.equal(collections.find(collection => collection.id === collectionId)?.name, 'Compatibility collection',
        'Custom collections must survive a complete native restart');
      await invoke('plugin:file|rename_custom_collection', { collectionId, name: 'Updated compatibility collection' });
      assert.equal((await invoke('plugin:file|list_custom_collections')).find(collection => collection.id === collectionId)?.name,
        'Updated compatibility collection');
      await invoke('plugin:file|delete_custom_collection', { collectionId });
      assert.ok(!(await invoke('plugin:file|list_custom_collections')).some(collection => collection.id === collectionId));
      results.checks.push('collection-persistence');
      const { notes } = await invoke('plugin:dailynotes|list_workspace_notes', { request: { journalId: null } });
      const note = notes.find(note => note.title === expectedTitle);
      assert.ok(note, 'Focused page title was not saved during shutdown');
      assert.ok(note.content.includes(expectedBody), 'The complete journal body must survive shutdown');
      await (await browser.$('button[aria-label="Journal"]')).click();
      const title = await browser.$('textarea[aria-label="Page title"]');
      await title.waitForDisplayed({ timeout: 30_000 });
      assert.equal(await title.getValue(), expectedTitle);
      await browser.saveScreenshot(path.join(reports, 'journal-after-restart.png'));
      if (process.env.LATTICE_NATIVE_LEAK_SCAN === '1') {
        results.nativeLeaksBaseline = await scanNativeLeaks(app.pid);
        await writeFile(path.join(reports, 'native-leaks-baseline.txt'),
          results.nativeLeaksBaseline.output ?? results.nativeLeaksBaseline.reason);
      }
      const samples = [];
      for (let cycle = 0; cycle < 25; cycle++) {
        await (await browser.$('button[aria-label="Show conversation and highlights"]')).click();
        await (await browser.$('aside[aria-label="Beside this page"]')).waitForDisplayed({ timeout: 5000 });
        await (await browser.$('button[aria-label="Hide panel"]')).click();
        await (await browser.$('aside[aria-label="Beside this page"]')).waitForDisplayed({ reverse: true, timeout: 5000 });
        // Exercise real IPC/result allocation as well as mount/unmount cleanup.
        for (let read = 0; read < 10; read++) await invoke('plugin:dailynotes|list_workspace_notes', { request: { journalId: null } });
        const renderer = await browser.execute(() => ({
          domNodes: document.querySelectorAll('*').length,
          jsHeapBytes: performance.memory?.usedJSHeapSize ?? null,
        }));
        samples.push({ cycle, ...renderer, ...await sampleProcessTree(app.pid) });
      }
      const settled = samples.slice(5);
      const mean = values => values.reduce((sum, value) => sum + value, 0) / values.length;
      const baseline = mean(settled.slice(0, 5).map(sample => sample.residentBytes));
      const final = mean(settled.slice(-5).map(sample => sample.residentBytes));
      const residentGrowthBytes = final - baseline;
      results.resources = { workload: '25 panel open/close cycles and 250 native note reads',
        coverage: 'Tauri parent and descendants; macOS WebKit XPC/Metal require separate profiling',
        residentGrowthBytes, samples };
      if (process.env.LATTICE_NATIVE_LEAK_SCAN === '1') {
        results.nativeLeaks = await scanNativeLeaks(app.pid);
        await writeFile(path.join(reports, 'native-leaks.txt'), results.nativeLeaks.output ?? results.nativeLeaks.reason);
      }
      // Warmup excluded. These are regression budgets, not claims of zero leaks.
      assert.ok(residentGrowthBytes < 64 * 1024 * 1024, `Retained process-tree growth: ${residentGrowthBytes} bytes`);
      assert.ok(Math.max(...settled.map(sample => sample.domNodes)) - Math.min(...settled.map(sample => sample.domNodes)) < 100,
        'Closed panel cycles must not accumulate DOM nodes');
      assert.ok(settled.at(-1).processes.length <= settled[0].processes.length + 2, 'Child processes must plateau');
      results.checks.push('resource-churn');
      results.checks.push('restart-persistence');
      await closeNormally();
    });
    results.passed = results.checks.length === 7;
    assert.equal(results.passed, true);
  } catch (error) {
    if (browser) {
      results.failureContext = await browser.execute(() => ({
        focused: document.hasFocus(),
        visibility: document.visibilityState,
        animations: document.getAnimations().map(animation => ({
          playState: animation.playState, currentTime: animation.currentTime,
        })),
      })).catch(() => null);
      await browser.saveScreenshot(path.join(reports, 'failure.png')).catch(() => {});
      await writeFile(path.join(reports, 'failure.html'), await browser.getPageSource().catch(() => '')).catch(() => {});
    }
    throw error;
  } finally {
    if (app && app.exitCode === null) {
      app.kill();
      for (let attempt = 0; attempt < 50 && app.exitCode === null && app.signalCode === null; attempt++) await delay(100);
      if (app.exitCode === null && app.signalCode === null) app.kill('SIGKILL');
    }
    await writeFile(path.join(reports, 'result.json'), JSON.stringify(results, null, 2));
  }
});

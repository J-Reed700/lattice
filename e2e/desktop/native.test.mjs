import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { closeSync, openSync } from 'node:fs';
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { test } from 'node:test';
import { remote } from 'webdriverio';

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
  const socket = net.createServer();
  await new Promise(resolve => socket.listen(0, '127.0.0.1', resolve));
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const log = openSync(path.join(reports, `app-${++launches}.log`), 'w');
  app = spawn(manifest.binary, [], {
    cwd: path.dirname(manifest.binary),
    env: { ...process.env, TAURI_WEBDRIVER_PORT: String(port) },
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
    connectionRetryCount: 0, connectionRetryTimeout: 15_000,
    capabilities: { browserName: 'wry', 'wdio:tauriServiceOptions': { windowLabel: 'main' } },
  });
  await browser.setTimeout({ script: 30_000, implicit: 0 });
  await browser.waitUntil(async () => browser.execute(() => Boolean(window.__TAURI__?.core)), { timeout: 30_000 });
  assert.equal(await invoke('plugin:app|identifier'), manifest.identifier);
}

async function closeNormally() {
  const closing = app;
  // Exercise the real native close event and renderer save gate, without
  // clicking another element and accidentally committing a focused title first.
  await browser.execute(() => {
    setTimeout(() => window.__TAURI__.window.getCurrentWindow().close(), 20);
  });
  const deadline = Date.now() + 30_000;
  while (closing.exitCode === null && closing.signalCode === null && Date.now() < deadline) await delay(100);
  assert.equal(closing.exitCode, 0, 'Normal native close must exit successfully after flushing saves');
  app = undefined;
  browser = undefined;
}

test('packaged desktop: onboarding, native file access, editing, close and persistence', { timeout: 240_000 }, async t => {
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
      const { notes } = await invoke('plugin:dailynotes|list_workspace_notes', { request: { journalId: null } });
      const note = notes.find(note => note.title === expectedTitle);
      assert.ok(note, 'Focused page title was not saved during shutdown');
      assert.ok(note.content.includes(expectedBody), 'The complete journal body must survive shutdown');
      await (await browser.$('button[aria-label="Journal"]')).click();
      const title = await browser.$('textarea[aria-label="Page title"]');
      await title.waitForDisplayed({ timeout: 30_000 });
      assert.equal(await title.getValue(), expectedTitle);
      await browser.saveScreenshot(path.join(reports, 'journal-after-restart.png'));
      results.checks.push('restart-persistence');
      await closeNormally();
    });
    results.passed = results.checks.length === 5;
    assert.equal(results.passed, true);
  } catch (error) {
    if (browser) {
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

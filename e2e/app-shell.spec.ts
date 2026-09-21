import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { makePreviewPdf } from './fixtures/previewPdf';
import { makeAppSettings } from '../src/tests/fixtures/appSettings';

test.beforeEach(async ({ page }) => {
  // Renderer checks must not wait for external font servers when DNS is offline.
  await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ contentType: 'text/css', body: '' }));
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
    let shutdownHandler: number | undefined;
    window.addEventListener('test:native-quit', (event) => {
      if (shutdownHandler !== undefined) void callbacks.get(shutdownHandler)?.({ event: 'lattice:shutdown-requested', id: 1, payload: (event as CustomEvent<number>).detail });
    });
    let nextCallbackId = 1;
    let nextListenerId = 1;
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();

    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { label: 'main' },
        },
        transformCallback(callback: (...args: unknown[]) => unknown, once = false) {
          const id = nextCallbackId++;
          callbacks.set(id, once ? (...args) => {
            callbacks.delete(id);
            return callback(...args);
          } : callback);
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        async invoke(command: string, args?: { event?: string; handler?: number; payload?: unknown }) {
          if (command === 'plugin:event|emit') {
            if (args?.event === 'lattice:shutdown-response') document.documentElement.dataset.shutdownResponse = JSON.stringify(args.payload);
            return undefined;
          }
          if (command === 'plugin:event|listen') {
            if (args?.event === 'lattice:shutdown-requested') shutdownHandler = args.handler;
            return nextListenerId++;
          }
          if (command === 'plugin:event|unlisten') {
            return undefined;
          }
          if (command === 'plugin:model|check_first_run_status') {
            return JSON.stringify({
              needs_setup: false,
              recommended_model_id: null,
              recommended_model_name: null,
              estimated_size_bytes: null,
            });
          }
          const override = (window as unknown as { __LATTICE_TEST_INVOKE__?: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__;
          if (override) return override(command, args);
          throw new Error(`Tauri backend unavailable in renderer smoke test: ${command}`);
        },
      },
    });

    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
      configurable: true,
      value: { unregisterListener: () => undefined },
    });
  });
});

test('renders the application shell and navigates to settings', async ({ page }) => {
  const pageErrors: Error[] = [];
  page.on('pageerror', (error) => pageErrors.push(error));

  await page.goto('/');

  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Journal', exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  expect(pageErrors).toEqual([]);
});

test('opens imported PDF bytes in the library and renders pages after reopening', async ({ page }) => {
  // A native smoke check can supply the exact bytes returned by read_file_bytes.
  const bytes = process.env.LATTICE_PREVIEW_PDF
    ? Array.from(await readFile(process.env.LATTICE_PREVIEW_PDF))
    : makePreviewPdf();
  const filePath = `/home/test/.lattice/files/${'a'.repeat(64)}/preview.pdf`;
  await page.addInitScript(({ settings, bytes, filePath }) => {
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:file|list_all_documents') return [{
        id: 'imported-pdf', fileName: 'preview.pdf', filePath, fileType: 'pdf',
        category: 'Document', language: 'en', wordCount: 100,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      }];
      if (command === 'plugin:file|read_file_bytes') {
        if ((args as { path: string }).path !== filePath) throw new Error('Unexpected PDF path');
        return bytes;
      }
      if (command === 'plugin:health|initialize_database') return undefined;
      if (command === 'plugin:download|list_downloads') return [];
      if (command === 'plugin:file|get_indexed_folders' || command === 'plugin:conversation|list_conversation_spaces') return [];
      throw new Error(`Unsupported PDF fixture command: ${command}`);
    };
  }, { settings: makeAppSettings(), bytes, filePath });
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/files');
  await page.getByRole('button', { name: 'Tree view', exact: true }).click();
  await expect(page.getByRole('button', { name: /^Imported files\s*1$/ })).toBeVisible();
  await expect(page.getByText('a'.repeat(64), { exact: true })).toHaveCount(0);
  const document = page.getByRole('button').filter({ has: page.getByText('preview.pdf', { exact: true }) });
  await document.press('Enter');
  const viewer = page.getByRole('dialog', { name: 'preview.pdf' });
  await expect(viewer.getByText(/Page 1 of (?:[2-9]|[1-9]\d+)/)).toBeVisible();
  const canvas = viewer.locator('canvas');
  // Wait for ink, not just an allocated blank canvas or a loaded page count.
  const expectInk = () => expect.poll(() => canvas.evaluate((element: HTMLCanvasElement) => {
    const pixels = element.getContext('2d')?.getImageData(0, 0, element.width, element.height).data;
    return pixels?.some((value, index) => index % 4 === 0 && value < 200 && pixels[index + 3] > 0);
  })).toBe(true);
  await expectInk();
  await viewer.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(viewer.getByText(/Page 2 of /)).toBeVisible();
  await expect(viewer.locator('.react-pdf__Page[data-page-number="2"] canvas')).toBeVisible();
  await expectInk();
  await viewer.getByRole('button', { name: 'Zoom in', exact: true }).click();
  await expect(viewer.getByText('120%', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(viewer).toHaveCount(0);
  await document.press('Enter');
  await expect(viewer.getByText(/Page 1 of (?:[2-9]|[1-9]\d+)/)).toBeVisible();
  await expect(canvas).toBeVisible();
  await expectInk();
  await expect(viewer.getByText('Failed to load PDF', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-pdf-preview.png' });
  expect(errors).toEqual([]);
});

test.describe('Library collections', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(settings => {
      const documents = ['Field guide.pdf', 'Interview notes.pdf', 'Planning.pdf'].map((fileName, index) => ({
        id: `collection-doc-${index}`, fileName,
        filePath: `/home/test/.lattice/files/${String(index).repeat(64)}/${fileName}`,
        fileType: 'pdf', category: 'Document', language: 'en', wordCount: 1200 + index * 200,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      }));
      (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async command => {
        if (command === 'plugin:settings|get_settings') return settings;
        if (command === 'plugin:file|list_all_documents') return documents;
        if (command === 'plugin:health|initialize_database') return undefined;
        if (['plugin:download|list_downloads', 'plugin:file|get_indexed_folders', 'plugin:conversation|list_conversation_spaces', 'plugin:conversation|list_document_space_memberships'].includes(command)) return [];
        throw new Error(`Unsupported collection fixture command: ${command}`);
      };
    }, makeAppSettings());
    await page.goto('/files');
    await page.getByRole('button', { name: 'List view', exact: true }).click();
  });

  test('organizes selected documents, edits membership, and persists collections across reload', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.getByRole('checkbox', { name: 'Select Field guide.pdf', exact: true }).click();
    await page.getByRole('checkbox', { name: 'Select Interview notes.pdf', exact: true }).click();
    await page.screenshot({ path: '/tmp/lattice-library-selection.png' });
    await page.getByRole('button', { name: 'Add to collection', exact: true }).click();
    const add = page.getByRole('dialog', { name: 'Add to collection', exact: true });
    await add.getByLabel('New collection', { exact: true }).fill('Fieldwork');
    await add.getByRole('button', { name: 'Create and add' }).click();

    await page.getByRole('button', { name: /^Fieldwork\s*2$/ }).click();
    await expect(page.getByText('Planning.pdf', { exact: true })).toHaveCount(0);
    await page.getByRole('checkbox', { name: 'Select Interview notes.pdf', exact: true }).click();
    await page.getByRole('button', { name: 'Remove from collection', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Fieldwork\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Add documents', exact: true }).click();
    const picker = page.getByRole('dialog', { name: 'Add documents to Fieldwork' });
    await expect(picker.getByText('Field guide.pdf', { exact: true })).toHaveCount(0);
    await picker.getByRole('button', { name: 'Select shown' }).click();
    await picker.getByRole('button', { name: 'Add 2 documents' }).click();
    await expect(page.getByRole('button', { name: /^Fieldwork\s*3$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Actions for Fieldwork', exact: true }).click();
    await page.getByRole('button', { name: 'Rename collection', exact: true }).click();
    const rename = page.getByRole('dialog', { name: 'Rename collection' });
    await rename.getByRole('textbox', { name: 'Collection name' }).fill('Field notes');
    await rename.getByRole('button', { name: 'Rename', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Field notes', exact: true })).toBeVisible();
    await page.reload();
    await page.getByRole('button', { name: 'List view', exact: true }).click();
    await page.getByRole('button', { name: /^Field notes\s*3$/ }).click();
    await expect(page.getByText('Interview notes.pdf', { exact: true })).toBeVisible();
    await page.screenshot({ path: '/tmp/lattice-library-collections.png' });

    await page.getByRole('button', { name: 'Actions for Planning.pdf', exact: true }).click();
    await page.getByRole('menuitem', { name: 'Add to collection…' }).click();
    await add.getByLabel('New collection', { exact: true }).fill('Reference');
    await add.getByRole('button', { name: 'Create and add' }).click();
    await expect(page.getByRole('button', { name: /^Reference\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Actions for Field notes', exact: true }).click();
    await page.getByRole('button', { name: 'Delete collection', exact: true }).click();
    await page.getByRole('dialog', { name: 'Delete collection?' }).getByRole('button', { name: /Delete collection/ }).click();
    await expect(page.getByRole('button', { name: /^Field notes\s*3$/ })).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'All documents', exact: true })).toBeVisible();
    await expect(page.getByText('Field guide.pdf', { exact: true })).toBeVisible();
    await expect(page.getByText('Interview notes.pdf', { exact: true })).toBeVisible();
    await expect(page.getByText('Planning.pdf', { exact: true })).toBeVisible();
  });

  test('fills a new empty collection and adds to an existing collection from the grid', async ({ page }) => {
    await page.getByRole('button', { name: 'New collection', exact: true }).click();
    await page.getByRole('textbox', { name: 'New collection', exact: true }).fill('Reading');
    await page.getByRole('textbox', { name: 'New collection', exact: true }).press('Enter');
    const picker = page.getByRole('dialog', { name: 'Add documents to Reading' });
    await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByText('This collection is empty.')).toBeVisible();
    await page.getByRole('button', { name: 'Choose documents', exact: true }).click();
    await picker.getByRole('textbox', { name: 'Find documents' }).fill('Field');
    await picker.getByRole('checkbox', { name: 'Select Field guide.pdf', exact: true }).check();
    await page.screenshot({ path: '/tmp/lattice-library-collection-picker.png' });
    await picker.getByRole('button', { name: 'Add 1 document', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Reading\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'All documents', exact: true }).click();
    await page.getByRole('button', { name: 'Grid view', exact: true }).click();
    await page.getByRole('button', { name: 'Actions for Planning.pdf', exact: true }).press('Enter');
    await page.getByRole('menuitem', { name: 'Add to collection…' }).click();
    const add = page.getByRole('dialog', { name: 'Add to collection', exact: true });
    await add.getByRole('button', { name: 'Reading', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Reading\s*2$/ })).toBeVisible();
  });
});

for (const [cached, system, expected] of [
  ['dark', 'light', 'dark'],
  ['light', 'dark', 'light'],
  ['system', 'dark', 'dark'],
  ['invalid', 'light', 'light'],
] as const) {
  test(`applies startup theme ${cached}/${system} before React loads`, async ({ page }) => {
    await page.addInitScript(value => localStorage.setItem('lattice-theme', value), cached);
    await page.emulateMedia({ colorScheme: system });
    // Hold back the production bundle: the blocking bootstrap must work alone.
    await page.route('**/assets/*.js', route => route.abort());
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', expected);
    await expect(page.locator('html')).toHaveCSS('color-scheme', expected);
    await expect(page.locator('#root')).toBeEmpty();
  });
}

test('theme text and destructive actions keep readable contrast', async ({ page }) => {
  await page.goto('/settings');
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    const ratios = await page.evaluate(() => {
      const sample = document.createElement('span');
      document.body.append(sample);
      const color = (token: string) => {
        sample.style.color = `hsl(var(--${token}))`;
        return getComputedStyle(sample).color.match(/[\d.]+/g)!.slice(0, 3).map(Number);
      };
      const luminance = (rgb: number[]) => rgb.reduce((sum, channel, index) => {
        const value = channel / 255;
        return sum + [0.2126, 0.7152, 0.0722][index] * (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
      }, 0);
      const contrast = (fg: number[], bg: number[]) => {
        const [low, high] = [luminance(fg), luminance(bg)].sort((a, b) => a - b);
        return (high + 0.05) / (low + 0.05);
      };
      const results: Record<string, number> = {};
      for (const surface of ['chrome', 'bg', 'surface', 'surface-raised', 'surface-overlay', 'surface-sunken']) {
        results[`muted/${surface}`] = contrast(color('text-muted'), color(surface));
        results[`destructive-hover/${surface}`] = contrast(
          color('accent-fg').map((value, index) => value * 0.9 + color(surface)[index] * 0.1),
          color('danger').map((value, index) => value * 0.9 + color(surface)[index] * 0.1),
        );
      }
      results.destructive = contrast(color('accent-fg'), color('danger'));
      results['destructive-brightness-hover'] = contrast(
        color('accent-fg').map(value => Math.min(255, value * 1.1)),
        color('danger').map(value => Math.min(255, value * 1.1)),
      );
      sample.remove();
      return results;
    });
    for (const [pair, ratio] of Object.entries(ratios)) {
      expect(ratio, `${theme} ${pair}`).toBeGreaterThanOrEqual(4.5);
    }
  }
});

test('HTML and code previews follow light and dark themes', async ({ page }, testInfo) => {
  const fileRoot = `/home/test/.lattice/files/${'b'.repeat(64)}`;
  await page.addInitScript(({ settings, fileRoot }) => {
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:file|list_all_documents') return ['article.html', 'example.ts'].map((fileName, index) => ({
        id: `preview-${index}`, fileName, filePath: `${fileRoot}/${fileName}`, fileType: index ? 'ts' : 'html',
        category: 'Document', language: 'en', wordCount: 100,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      }));
      if (command === 'plugin:file|read_file_content') return (args as { path: string }).path.endsWith('.ts')
        ? 'const answer = 42;'
        : '<style>body { color: black; background: white; }</style><h1>Theme preview</h1><p style="color: black; background: white">Readable article</p><pre>const answer = 42;</pre>';
      if (command === 'plugin:health|initialize_database') return undefined;
      if (command === 'plugin:download|list_downloads' || command === 'plugin:file|get_indexed_folders' || command === 'plugin:conversation|list_conversation_spaces') return [];
      throw new Error(`Unsupported theme preview fixture command: ${command}`);
    };
  }, { settings: makeAppSettings(), fileRoot });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/files');
  await page.getByRole('button', { name: 'Tree view', exact: true }).click();
  await page.getByRole('button').filter({ has: page.getByText('article.html', { exact: true }) }).press('Enter');
  const article = page.frameLocator('iframe[title="article.html"]');
  await expect(article.getByText('Readable article')).toBeVisible();
  await expect(article.locator('html')).toHaveCSS('color-scheme', 'light');
  const lightText = await article.locator('p').evaluate(element => getComputedStyle(element).color);
  await page.screenshot({ path: testInfo.outputPath('article-light.png') });
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(article.locator('html')).toHaveCSS('color-scheme', 'dark');
  await expect(article.locator('p')).not.toHaveCSS('color', lightText);
  await page.screenshot({ path: testInfo.outputPath('article-dark.png') });
  await page.keyboard.press('Escape');
  await page.getByRole('button').filter({ has: page.getByText('example.ts', { exact: true }) }).press('Enter');
  const code = page.getByRole('dialog', { name: 'example.ts' }).locator('pre');
  await expect(code).toBeVisible();
  const darkCodeBackground = await code.evaluate(element => getComputedStyle(element).backgroundColor);
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(code).not.toHaveCSS('background-color', darkCodeBackground);
});

// These tests exercise the real renderer/settings controls against a simulated
// persistent IPC repository. They do not claim native desktop persistence.
test('switches themes, remembers the choice on reload, and follows system changes', async ({ page }) => {
  await page.addInitScript((defaults) => {
    // Init scripts have no guaranteed order, including after a reload. Install
    // an independent handler that the common IPC fixture calls at invocation.
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      const saved = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
      if (command === 'plugin:settings|get_settings') return saved;
      if (command === 'plugin:settings|update_settings') {
        if (localStorage.getItem('test:fail-save')) throw new Error('Disk is full');
        const { category, updates } = (args as { settings: { category: string | null; updates: Record<string, unknown> } }).settings;
        if (category) Object.assign(saved[category], updates);
        else Object.assign(saved, updates);
        localStorage.setItem('test:settings', JSON.stringify(saved));
        return saved;
      }
      throw new Error(`Unsupported settings fixture command: ${command}`);
    };
  }, makeAppSettings());
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Display', exact: true }).click();
  const dark = page.getByRole('radio', { name: 'Dark', exact: true });
  await dark.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await page.evaluate(() => localStorage.getItem('lattice-theme'))).toBe('dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'Display', exact: true }).click();
  await page.getByRole('radio', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.evaluate(() => localStorage.setItem('test:fail-save', 'true'));
  await dark.click();
  await expect(page.getByRole('alert')).toContainText("Couldn't save your theme");
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  expect(await page.evaluate(() => localStorage.getItem('lattice-theme'))).toBe('light');
  await page.evaluate(() => localStorage.removeItem('test:fail-save'));
  await page.getByRole('radio', { name: 'System', exact: true }).click();
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-light.png' });
  await page.getByRole('radio', { name: 'System', exact: true }).press('ArrowLeft');
  await expect(dark).toHaveAttribute('aria-checked', 'true');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-dark.png' });
  await page.setViewportSize({ width: 800, height: 600 });
  await expect(dark).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(800);
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-compact.png' });
});

// Simulated native event transport: verifies the renderer becomes inert before exit.
test('acknowledges native quit and prevents further editing while closing', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('test:native-quit', { detail: 42 })));
  await expect(page.getByRole('status')).toContainText('Saving your work before quitting…');
  await expect(page.locator('html')).toHaveAttribute('data-shutdown-response', JSON.stringify({ requestId: 42, saved: true }));
  await expect(page.locator('[inert]')).toHaveCount(1);
  await page.screenshot({ path: '/tmp/lattice-acceptance/shutdown.png' });
});

// Exercise the decomposed sidebar as one renderer flow, backed by a simulated
// repository. This catches broken prop/hook wiring that isolated hooks miss.
test('chat sidebar renames a conversation and opens the spaces editor', async ({ page }) => {
  await page.addInitScript((settings) => {
    const stamp = new Date().toISOString();
    const conversation = { id: 'conversation-1', title: 'Reading notes', modelName: 'test-model', systemPrompt: null, createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isSaved: false, isBookmarked: false, isPinned: false, isArchived: false };
    const space = { id: 'space_general', name: 'General', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: stamp, updatedAt: stamp };
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:conversation|list_conversation_spaces') return [space];
      if (command === 'plugin:conversation|list_journals') return [];
      if (command === 'plugin:conversation|list_conversations_explorer' || command === 'plugin:conversation|list_conversations') return { conversations: [conversation], total: 1 };
      if (command === 'plugin:conversation|get_conversation') return { conversation };
      if (command === 'plugin:conversation|get_conversation_messages') return { messages: [], total: 0 };
      if (command === 'plugin:conversation|list_message_bookmarks') return { bookmarks: [], total: 0 };
      if (command === 'plugin:conversation|rename_conversation') {
        conversation.title = (args as { request: { newTitle: string } }).request.newTitle;
        return { status: 'success' };
      }
      if (command === 'plugin:conversation|list_conversation_linked_documents' || command === 'plugin:conversation|list_conversation_web_sources') return [];
      if (command === 'plugin:model|list_downloaded_models') return [];
      throw new Error(`Unsupported chat fixture command: ${command}`);
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/chat');
  const list = page.getByRole('navigation', { name: 'Conversations', exact: true });
  await expect(list.getByText('Reading notes', { exact: true })).toBeVisible();
  await list.getByText('Reading notes', { exact: true }).dblclick();
  const title = page.getByRole('textbox', { name: 'Rename conversation: Reading notes' });
  await title.fill('Updated reading notes');
  await title.press('Enter');
  await expect(list.getByText('Updated reading notes', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Spaces', exact: true })).toBeVisible();
  const panel = page.locator('aside').filter({ has: page.getByRole('heading', { name: 'Spaces', exact: true }) });
  await expect(panel).toHaveCSS('opacity', '1');
  await page.screenshot({ path: '/tmp/lattice-architecture-fix/chat-spaces.png' });
  await panel.getByRole('button', { name: 'New space', exact: true }).click();
  await panel.getByPlaceholder('Space name (e.g. Product, Research, Personal)').fill('Unsaved space draft');
  await page.getByRole('button', { name: 'Close spaces panel', exact: true }).last().click();
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  await expect(page.getByPlaceholder('Space name (e.g. Product, Research, Personal)')).toHaveValue('Unsaved space draft');
  await page.getByRole('button', { name: 'Close spaces panel', exact: true }).last().click();
  await expect(page.getByRole('heading', { name: 'Spaces', exact: true })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('creates a space in Settings and opens it in Chat', async ({ page }) => {
  await page.addInitScript((settings) => {
    const stamp = new Date().toISOString();
    const general = { id: 'space_general', name: 'General', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: stamp, updatedAt: stamp };
    let spaces = [general];
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:conversation|list_conversation_spaces') return spaces;
      if (command === 'plugin:conversation|create_conversation_space') {
        if (document.documentElement.dataset.spaceCreationAllowed !== 'true') {
          throw new Error('Disk full');
        }
        const request = (args as { request: { name: string } }).request;
        const space = { ...general, ...request, id: 'space_research', sortOrder: 1 };
        spaces = [...spaces, space];
        return space;
      }
      if (command === 'plugin:conversation|list_conversations_explorer' || command === 'plugin:conversation|list_conversations') return { conversations: [], total: 0 };
      if (command === 'plugin:conversation|list_message_bookmarks') return { bookmarks: [], total: 0 };
      if (command === 'plugin:conversation|list_journals' || command === 'plugin:model|list_downloaded_models' || command === 'plugin:download|list_downloads' || command === 'plugin:file|get_indexed_folders') return [];
      throw new Error(`Unsupported spaces fixture command: ${command}`);
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));

  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Spaces', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Spaces', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Open General in Chat', exact: true })).toBeVisible();
  const name = page.getByRole('textbox', { name: 'Space name', exact: true });
  await name.fill('Research');
  await name.press('Enter');
  await expect(page.getByText('Could not update space', { exact: true })).toBeVisible();
  await expect(name).toHaveValue('Research');
  await expect(page.getByRole('button', { name: 'Open Research in Chat', exact: true })).toHaveCount(0);

  await page.evaluate(() => { document.documentElement.dataset.spaceCreationAllowed = 'true'; });
  await page.getByRole('button', { name: 'Create space', exact: true }).click();
  await expect(page.getByText('Space created', { exact: true })).toBeVisible();
  await expect(name).toHaveValue('');
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('button', { name: 'Open Research in Chat', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: '/tmp/lattice-settings-spaces-light.png', animations: 'disabled' });
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: '/tmp/lattice-settings-spaces-dark.png', animations: 'disabled' });
  await page.getByRole('button', { name: 'Open Research in Chat', exact: true }).click();
  await expect(page).toHaveURL(/\/chat$/);
  await expect(page.getByRole('button', { name: 'Change scope', exact: true })).toHaveText('Research');
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  const panel = page.locator('aside').filter({ has: page.getByRole('heading', { name: 'Spaces', exact: true }) });
  await expect(panel.getByText('Research', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test('keeps Ollama and llama.cpp connections separate across provider changes', async ({ page }) => {
  await page.addInitScript((defaults) => {
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      const settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:settings|update_settings') {
        const payload = args as { settings: { updates: Record<string, unknown> } };
        Object.assign(settings.llm, payload.settings.updates);
        localStorage.setItem('test:settings', JSON.stringify(settings));
        return settings;
      }
      if (command === 'plugin:settings|test_llama_cpp_connection') return { endpoint: '/v1/chat/completions', models: ['test-model.gguf'] };
      if (command === 'plugin:model|list_downloaded_models') return [];
      if (command.startsWith('plugin:model|get_active')) return null;
      throw new Error(`Unsupported connection fixture command: ${command}`);
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByLabel('Chat provider').selectOption('llamacpp');
  await expect(page.getByRole('heading', { name: 'llama.cpp server' })).toBeVisible();
  await page.getByLabel('llama.cpp URL').fill('https://llama.example.com');
  await page.getByRole('button', { name: 'Test llama.cpp connection', exact: true }).click();
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('test-model.gguf');
  await page.getByRole('button', { name: 'Save connection' }).click();
  await page.getByLabel('Chat provider').selectOption('ollama');
  await expect(page.getByRole('heading', { name: 'Ollama server' })).toBeVisible();
  await expect(page.getByLabel('Server URL')).toHaveValue('http://localhost:11434');
  await page.getByLabel('Chat provider').selectOption('llamacpp');
  await expect(page.getByLabel('llama.cpp URL')).toHaveValue('https://llama.example.com');
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('test-model.gguf');
  await page.screenshot({ path: '/tmp/lattice-llamacpp-settings.png' });
});

// A large catalog must stay navigable without a token or a local chat provider.
test('catalog offers category previews, bounded pages, and honest search results', async ({ page }) => {
  await page.addInitScript((settings) => {
    settings.llm.provider = 'llamacpp';
    const groups = [
      { category: 'LLM', names: ['Qwen 3 8B', 'Gemma 3 4B', 'Phi 4 Mini', 'Mistral 7B', 'Llama 3.2 3B', 'Qwen 3 14B', 'DeepSeek R1 8B', 'SmolLM 3B'], variants: 3 },
      { category: 'Embedding', names: ['BGE Small English', 'Nomic Embed Text', 'All MiniLM L6', 'BGE Base English', 'E5 Small', 'GTE Base'], variants: 1 },
      { category: 'OCR', names: ['PaddleOCR', 'DeepSeek OCR', 'GOT OCR'], variants: 1 },
      { category: 'Transcription', names: ['Whisper Small', 'Whisper Base', 'Whisper Tiny'], variants: 1 },
    ];
    const models = groups.flatMap(({ category, names, variants }) => names.flatMap((name, index) =>
      Array.from({ length: variants }, (_, variant) => {
        const id = `${category}-${index}-${variant}`;
        const quantization = ['Q4_K_M', 'Q5_K_M', 'Q8_0'][variant];
        return {
          model: {
            id, name: variants > 1 ? `${name} · ${quantization}` : name, category,
            description: 'A model available in the catalog.', size_gb: 1 + index + variant / 10,
            minimum_ram_gb: 4, recommended_ram_gb: 8, context_length: 32768,
            performance_tier: 'Balanced', supported_quantizations: category === 'LLM' ? [quantization] : [],
            capabilities: ['chat'], download_url: `https://huggingface.co/catalog/${id}`,
            license: 'Apache-2.0', requires_auth: false, model_id: `catalog/${id}`,
            default_filename: `${id}.gguf`, files: [], total_size_bytes: 1000000000,
            embedding_dimensions: category === 'Embedding' ? 384 : null, embedding_compatibility: null,
          },
          compatibility: { compatibility_level: 'Good', overall_score: 80, ram_score: 90, gpu_score: 80, disk_score: 90, estimated_tokens_per_second: null, estimated_loading_time_seconds: 3, recommendations: [], blockers: [] },
          ranking_score: 80 - index, popularity_downloads: 100000 - index * 1000 - variant, popularity_likes: 100,
        };
      })
    ));
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:model|get_model_download_path') return '/Users/example/Models';
      if (command === 'plugin:huggingface|get_huggingface_token_status') return { isSet: false };
      if (command === 'plugin:model|list_downloaded_models' || command === 'plugin:download|list_downloads') return [];
      if (command === 'plugin:model|is_model_already_downloaded') return false;
      if (command === 'plugin:model|get_all_recommended_models') return models;
      if (command === 'plugin:model|detect_system_capabilities') return { total_ram_gb: 32, available_ram_gb: 24, cpu_cores: 10, cpu_architecture: 'ARM64', gpu_type: 'AppleSilicon', gpu_acceleration: 'Metal', vram_gb: null, available_disk_gb: 500, os_type: 'macOS' };
      if (command === 'plugin:model|get_model_catalog_stats') return { total_entries: 36, expired_entries: 0, cache_size_bytes: 12000 };
      if (command === 'plugin:model|search_model_catalog') {
        if (localStorage.getItem('test:catalog-search-error')) throw new Error('Catalog search unavailable');
        const query = (args as { request: { query: string } }).request.query.toLowerCase();
        return models.filter(({ model }) => model.name.toLowerCase().includes(query)).map((entry) => ({ ...entry, relevance_score: 80 }));
      }
      throw new Error(`Unsupported catalog fixture command: ${command}`);
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Models', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Explore by purpose' })).toBeVisible();
  const chat = page.getByRole('region', { name: 'Chat & writing' });
  await expect(chat.getByRole('button', { name: 'Download', exact: true })).toHaveCount(3);
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(12);
  await expect(page.getByRole('region', { name: 'Search & retrieval' })).toBeVisible();
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/lattice-catalog-overview.png' });

  await chat.getByRole('button', { name: 'View all chat & writing models' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 24 models');
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(8);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 9–16 of 24 models');
  await page.getByRole('button', { name: /Phi 4 Mini · Q8_0/ }).click();
  await expect(page.getByRole('heading', { name: 'Phi 4 Mini · Q8_0', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 9–16 of 24 models');
  await page.getByRole('combobox', { name: 'Sort models' }).selectOption('size_asc');
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 24 models');
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/lattice-catalog-pages.png' });

  await page.getByRole('textbox', { name: 'Search models' }).fill('no-such-model');
  await expect(page.getByText('No models match.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(0);
  await page.evaluate(() => localStorage.setItem('test:catalog-search-error', 'true'));
  await page.getByRole('textbox', { name: 'Search models' }).fill('Qwen');
  await expect(page.getByText(/Couldn't load the catalog/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(0);
  await page.evaluate(() => localStorage.removeItem('test:catalog-search-error'));
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–6 of 6 models');
  await page.getByRole('button', { name: 'Clear search', exact: true }).click();
  await page.getByRole('button', { name: 'Reset filters', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Explore by purpose' })).toBeVisible();
  await page.getByRole('button', { name: 'Browse all models', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 36 models');
  await page.getByRole('button', { name: 'Explore categories', exact: true }).click();
  await page.setViewportSize({ width: 800, height: 700 });
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(800);
  await page.screenshot({ path: '/tmp/lattice-catalog-compact.png' });
});

test('restores a 45-PDF import and updates progress as files finish', async ({ page }) => {
  await page.addInitScript(({ settings }) => {
    localStorage.setItem('ingestHub.lastTab', 'files');
    const state = window as unknown as { __IMPORT_FINISHED__: number; __LATTICE_TEST_INVOKE__: (command: string) => Promise<unknown> };
    state.__IMPORT_FINISHED__ = 0;
    state.__LATTICE_TEST_INVOKE__ = async (command) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:batch|get_batch_history') return { jobs: [{ jobId: 'job', jobType: 'file_import', status: 'running', totalItems: 45, completedItems: 0, failedItems: 0 }] };
      if (command === 'plugin:batch|get_batch_status') return {
        jobId: 'job', jobType: 'file_import', status: 'running', totalItems: 45,
        completedItems: state.__IMPORT_FINISHED__, failedItems: 0,
        items: Array.from({ length: 45 }, (_, i) => ({
          itemId: `pdf-${i}`, target: `/Downloads/chapter-${i}.pdf`,
          status: i < state.__IMPORT_FINISHED__ ? 'completed' : i === state.__IMPORT_FINISHED__ ? 'processing' : 'pending',
        })),
      };
      if (command === 'plugin:health|initialize_database') return undefined;
      if (command === 'plugin:download|list_downloads' || command === 'plugin:file|get_indexed_folders' || command === 'plugin:conversation|list_conversation_spaces') return [];
      throw new Error(`Unsupported import fixture command: ${command}`);
    };
  }, { settings: makeAppSettings() });
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/ingest');
  await expect(page.getByText('0 of 45 files processed · 0%')).toBeVisible();
  await expect(page.getByText('Processing chapter-0.pdf — extracting text and building search index')).toBeVisible();
  await page.evaluate(() => { (window as unknown as { __IMPORT_FINISHED__: number }).__IMPORT_FINISHED__ = 3; });
  await expect(page.getByText('3 of 45 files processed · 7%')).toBeVisible();
  await expect(page.getByText('Processing chapter-3.pdf — extracting text and building search index')).toBeVisible();
  await expect(page.getByText('Imported', { exact: true })).toHaveCount(3);
  await expect(page.getByText('Queued', { exact: true })).toHaveCount(41);
  await page.screenshot({ path: '/tmp/lattice-import-progress.png' });
  expect(errors).toEqual([]);
});

test('creates a subject-agnostic study deck, reviews, quizzes, opens sources and saves edits', async ({ page }) => {
  await page.addInitScript(settings => {
    const source = { chunkId: 'biology-chunk', documentId: 'biology', fileName: 'biology.md', filePath: '/library/biology.md', excerpt: 'Chlorophyll absorbs the light used in photosynthesis.' };
    const original = {
      id: 'biology-deck', title: 'Biology review', focus: 'photosynthesis', studyGoal: 'Biology exam', modelName: 'test-model', createdAt: Date.now(),
      cards: [
        { id: 'light', deckId: 'biology-deck', question: 'What absorbs the light used in photosynthesis?', answer: 'Chlorophyll', options: ['Chlorophyll', 'Water', 'Oxygen', 'Glucose', 'Carbon dioxide'], correctIndex: 0, explanation: 'The passage identifies chlorophyll as the light absorber.', topic: 'Light absorption', source, dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0 },
        { id: 'energy', deckId: 'biology-deck', question: 'What form of energy does photosynthesis produce?', answer: 'Chemical energy', options: ['Sound', 'Chemical energy', 'Motion', 'Electricity', 'Gravity'], correctIndex: 1, explanation: 'The source describes a conversion from light to chemical energy.', topic: 'Energy conversion', source: { ...source, excerpt: 'Photosynthesis converts light energy into chemical energy.' }, dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0 },
      ],
    };
    type Deck = typeof original;
    const load = (): Deck | null => JSON.parse(localStorage.getItem('test:study-deck') ?? 'null');
    const save = (deck: Deck) => localStorage.setItem('test:study-deck', JSON.stringify(deck));
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') {
        settings.ui.theme = localStorage.getItem('test:study-theme') === 'light' ? 'light' : 'dark';
        return settings;
      }
      if (command === 'plugin:health|initialize_database') return undefined;
      if (command === 'plugin:download|list_downloads' || command === 'plugin:batch|get_batch_history') return [];
      if (command === 'plugin:file|list_all_documents') return [{ id: 'biology', fileName: source.fileName, filePath: source.filePath, fileType: 'md', category: 'Document', wordCount: 20 }];
      if (command === 'plugin:file|read_file_content') {
        if ((args as { path: string }).path !== source.filePath) throw new Error('Unexpected source');
        return '# Photosynthesis notes\n\nChlorophyll absorbs the light used in photosynthesis.\n\nPhotosynthesis converts light energy into chemical energy.';
      }
      const deck = load();
      if (command === 'plugin:study|list_study_decks') return deck ? [{ ...deck, cardCount: deck.cards.length, dueCount: deck.cards.filter(c => c.dueAt <= Date.now()).length, quizAttempts: 0, quizCorrect: 0 }] : [];
      if (command === 'plugin:study|get_study_deck') return deck;
      if (command === 'plugin:study|generate_study_deck') {
        const request = (args as { request: { title: string; focus: string; studyGoal: string; documentIds: string[] } }).request;
        if (request.documentIds.join(',') !== 'biology') throw new Error('Incorrect source scope');
        Object.assign(original, { title: request.title, focus: request.focus, studyGoal: request.studyGoal });
        await new Promise<void>(resolve => window.addEventListener('test:finish-generation', () => resolve(), { once: true }));
        save(original); return original;
      }
      if (command === 'plugin:study|review_study_card' && deck) {
        if (localStorage.getItem('test:study-fail-review')) throw new Error('Disk full');
        const request = (args as { request: { cardId: string; expectedReviews: number; selectedOption: number | null; rating: string } }).request;
        const card = deck.cards.find(c => c.id === request.cardId)!;
        if (request.expectedReviews !== card.reviewCount) throw new Error('Stale review');
        card.reviewCount++;
        const wrong = request.selectedOption !== null ? request.selectedOption !== card.correctIndex : request.rating === 'again';
        card.lapses += Number(wrong); card.dueAt = Date.now() + (wrong ? 600_000 : 86_400_000); card.intervalDays = wrong ? 0 : 1;
        save(deck); return card;
      }
      if (command === 'plugin:study|update_study_card' && deck) {
        const request = (args as { request: { cardId: string; question: string; answer: string; explanation: string } }).request;
        const card = deck.cards.find(c => c.id === request.cardId)!;
        Object.assign(card, { question: request.question, answer: request.answer, explanation: request.explanation });
        card.options[card.correctIndex] = card.answer; save(deck); return undefined;
      }
      if (command === 'plugin:study|delete_study_deck') { localStorage.removeItem('test:study-deck'); return undefined; }
      throw new Error(`Unsupported study fixture command: ${command}`);
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/study');
  await expect(page.getByRole('button', { name: 'Study', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('Put what you learn into practice.')).toBeVisible();
  await page.getByRole('button', { name: 'New deck', exact: true }).click();
  await page.getByLabel('Deck title').fill('Biology review');
  await page.getByLabel(/Topic or section/).fill('photosynthesis');
  await page.getByLabel(/Learning goal/).fill('Biology exam');
  await page.getByRole('checkbox', { name: 'biology.md' }).check();
  await page.getByLabel('Questions', { exact: true }).selectOption('2');
  await page.screenshot({ path: '/tmp/lattice-study-new-dark.png' });
  await page.getByRole('button', { name: 'Generate deck' }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Study', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Generating 2 questions' })).toBeVisible();
  await expect(page.getByText('Put what you learn into practice.')).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-study-background-dark.png' });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-generation')));
  await page.getByRole('button', { name: 'Study', exact: true }).click();
  await page.getByRole('button').filter({ hasText: 'Biology review' }).click();
  await expect(page.getByRole('heading', { name: 'Biology review', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Review due (2)' }).click();
  await expect(page.getByText('Chlorophyll', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Reveal answer' }).click();
  await page.evaluate(() => localStorage.setItem('test:study-fail-review', '1'));
  await page.getByRole('button', { name: 'Good', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Disk full');
  await page.evaluate(() => localStorage.removeItem('test:study-fail-review'));
  await page.getByRole('button', { name: 'Good', exact: true }).click();
  await expect(page.getByText('2 of 2 · Biology review')).toBeVisible();
  await page.getByRole('button', { name: 'Reveal answer' }).click();
  await page.getByRole('button', { name: 'Again', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Review complete' })).toBeVisible();
  await page.getByRole('button', { name: 'Back to deck' }).click();
  await page.getByRole('button', { name: 'Practice quiz', exact: true }).click();
  await page.getByRole('radio').nth(4).check();
  await page.getByRole('button', { name: 'Check answer' }).click();
  await expect(page.getByText('Review this answer')).toBeVisible();
  await page.getByText('Citation · biology.md', { exact: true }).click();
  await page.getByRole('button', { name: 'Open source' }).click();
  const viewer = page.getByRole('dialog', { name: 'biology.md' });
  await expect(viewer.getByRole('heading', { name: 'Photosynthesis notes' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.screenshot({ path: '/tmp/lattice-study-quiz-dark.png' });
  await page.getByRole('button', { name: 'End session' }).click();
  await page.locator('summary').filter({ hasText: 'What absorbs the light used in photosynthesis?' }).click();
  await page.getByRole('button', { name: 'Edit card' }).click();
  await page.getByLabel('Question', { exact: true }).fill('Which pigment absorbs the light used in photosynthesis?');
  await page.getByRole('button', { name: 'Save card' }).click();
  await page.reload();
  await expect(page.locator('summary').filter({ hasText: 'Which pigment absorbs the light used in photosynthesis?' })).toBeVisible();
  await page.evaluate(() => localStorage.setItem('test:study-theme', 'light'));
  await page.reload();
  await expect(page.locator('html')).not.toHaveClass(/dark/);
  await expect(page.getByRole('heading', { name: 'Biology review', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-study-deck-light.png' });
  await page.getByRole('button', { name: 'Delete deck', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete deck', exact: true }).click();
  await expect(page.getByText('Put what you learn into practice.')).toBeVisible();
  expect(errors).toEqual([]);
});


test('keeps both remote connections in Auto settings after saving and reloading', async ({ page }) => {
  await page.addInitScript(defaults => {
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      const saved = JSON.parse(localStorage.getItem('test:auto-settings') ?? JSON.stringify(defaults));
      if (command === 'plugin:settings|get_settings') return saved;
      if (command === 'plugin:settings|update_settings') {
        Object.assign(saved.llm, (args as { settings: { updates: object } }).settings.updates);
        localStorage.setItem('test:auto-settings', JSON.stringify(saved)); return saved;
      }
      if (command === 'plugin:model|list_downloaded_models' || command === 'plugin:download|list_downloads' || command === 'plugin:batch|get_batch_history') return [];
      throw new Error(`Unsupported Auto fixture command: ${command}`);
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByLabel('llama.cpp model', { exact: true }).fill('qwen.gguf');
  await page.getByRole('button', { name: 'Save connection' }).click();
  await expect(page.getByLabel('Chat provider')).toHaveValue('auto');
  await page.reload();
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await expect(page.getByLabel('Chat provider')).toHaveValue('auto');
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('qwen.gguf');
  await expect(page.getByText('Ollama server', { exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-auto-connections.png', fullPage: true });
});

test('chat exposes provider errors and persisted PDF import failures', async ({ page }) => {
  const settings = makeAppSettings();
  settings.llm.provider = 'auto';
  settings.llm.llamaCpp.model = 'qwen-test.gguf';
  await page.addInitScript((settings) => {
    const stamp = new Date().toISOString();
    const conversation = { id: 'patent-chat', title: 'Patent Training', modelName: '__ollama_server__', createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isArchived: false };
    const job = { jobId: 'pdf-import', jobType: 'file_import', status: 'completed', totalItems: 45, completedItems: 44, failedItems: 1, createdAt: stamp };
    const earlier = { ...job, jobId: 'earlier-pdf', totalItems: 1, completedItems: 1, failedItems: 0, createdAt: new Date(Date.now() - 3600000).toISOString() };
    let item = { itemId: 'failed-pdf', target: '/Downloads/mpep-2100.pdf', status: 'failed', errorMessage: 'PDF extraction timed out' as string | null };
    const saved = localStorage.getItem('test:pdf-recovery');
    if (saved) { const state = JSON.parse(saved); Object.assign(job, state.job); item = state.item; }
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:conversation|list_conversation_spaces') return [{ id: 'space_general', name: 'General', isArchived: false }];
      if (command === 'plugin:conversation|list_conversations_explorer' || command === 'plugin:conversation|list_conversations') return { conversations: [conversation], total: 1 };
      if (command === 'plugin:conversation|get_conversation') return { conversation };
      if (command === 'plugin:conversation|get_conversation_messages') return { messages: [], total: 0 };
      if (command === 'plugin:conversation|list_message_bookmarks') return { bookmarks: [], total: 0 };
      if (command === 'plugin:conversation|chat_with_conversation') throw { code: 'NETWORK_ERROR', message: 'Network error', details: 'llama.cpp request timed out' };
      if (command === 'plugin:batch|get_batch_history') return { jobs: [job, earlier] };
      if (command === 'plugin:batch|get_batch_status') return (args as { request: { jobId: string } }).request.jobId === earlier.jobId
        ? { ...earlier, completedAt: stamp, items: [{ itemId: 'earlier-item', target: '/Downloads/mpep-9035-appx-p.pdf', status: 'completed' }] }
        : { ...job, completedAt: job.status === 'completed' ? stamp : null, items: [item] };
      if (command === 'plugin:dialog|open') return '/Downloads/mpep-2100-corrected.pdf';
      if (command === 'plugin:batch|retry_failed_items') {
        const request = args as { jobId: string; itemId: string; replacementPath?: string };
        if (request.jobId !== job.jobId || request.itemId !== item.itemId) throw new Error('Retry must target the exact failed PDF');
        job.status = 'running'; job.failedItems = 0; item.status = 'processing'; item.errorMessage = null;
        if (request.replacementPath) item.target = request.replacementPath;
        setTimeout(() => {
          if (request.replacementPath) { job.status = 'completed'; job.completedItems = 45; item.status = 'completed'; }
          else { job.status = 'failed'; job.failedItems = 1; item.status = 'failed'; item.errorMessage = 'PDF extraction timed out again'; }
          localStorage.setItem('test:pdf-recovery', JSON.stringify({ job, item }));
        }, 600);
        return { newJobId: job.jobId, retriedCount: 1 };
      }
      if (command === 'plugin:conversation|list_journals' || command === 'plugin:conversation|list_conversation_linked_documents' || command === 'plugin:conversation|list_conversation_web_sources' || command === 'plugin:model|list_downloaded_models' || command === 'plugin:download|list_downloads' || command === 'plugin:file|get_indexed_folders') return [];
      throw new Error(`Unsupported failure fixture command: ${command}`);
    };
  }, settings);
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/chat?conversationId=patent-chat');
  await expect(page.getByText('qwen-test.gguf', { exact: true })).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: '1 file failed to import: mpep-2100.pdf' })).toBeVisible();
  const composer = page.getByRole('textbox', { name: 'Message composer' });
  await composer.fill('Help me learn the MPEP using only cited PDF passages.');
  await composer.press('Enter');
  await expect(page.getByText('llama.cpp request timed out', { exact: true }).first()).toBeVisible();
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await page.screenshot({ path: `/tmp/lattice-chat-failures-${theme}.png`, animations: 'disabled' });
  }
  await page.getByRole('link', { name: 'Review import history' }).click();
  await expect(page.getByRole('tab', { name: 'History', exact: true })).toHaveAttribute('data-state', 'active');
  await expect(page.getByText('44 imported, 1 failed', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /45 files Added/ })).toHaveAttribute('aria-expanded', 'true');
  await expect(page.getByRole('button', { name: /mpep-9035-appx-p.pdf Added/ })).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('heading', { name: '2 imports' })).toBeVisible();
  await expect(page.getByText('Failed — PDF extraction timed out', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry mpep-2100.pdf', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-import-failures.png', animations: 'disabled' });
  await page.getByRole('button', { name: 'Retry mpep-2100.pdf', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry all failed' })).toHaveCount(0);
  await expect(page.getByText('Failed — PDF extraction timed out again', { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText('Failed — PDF extraction timed out again', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Choose replacement for mpep-2100.pdf' }).click();
  await expect(page.getByText('45 imported', { exact: true })).toBeVisible();
  await expect(page.getByText('mpep-2100-corrected.pdf', { exact: true })).toBeVisible();
  await expect(page.getByText('Imported', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '2 imports' })).toBeVisible();
  await expect(page.getByText('Processing — not ready to search yet', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-import-recovered.png', animations: 'disabled' });
  await page.goto('/chat?conversationId=patent-chat');
  await expect(page.getByText('qwen-test.gguf', { exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Review import history' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('chat replaces an initial retrieval failure with tool results and keeps it corrected after reload', async ({ page }) => {
  const settings = makeAppSettings();
  settings.llm.provider = 'auto';
  settings.llm.llamaCpp.model = 'qwen-test.gguf';
  await page.addInitScript((settings) => {
    const native = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown>; transformCallback: (fn: (...args: unknown[]) => unknown) => number } }).__TAURI_INTERNALS__;
    // Capture the real controller listener while retaining the shell fixture.
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();
    const transform = native.transformCallback.bind(native);
    native.transformCallback = fn => { const id = transform(fn); callbacks.set(id, fn); return id; };
    const invoke = native.invoke.bind(native);
    let streamHandler: number | undefined;
    native.invoke = async (command, args) => {
      const input = args as { event?: string; handler?: number } | undefined;
      if (command === 'plugin:event|listen' && input?.event === 'llm-stream') streamHandler = input.handler;
      return invoke(command, args);
    };
    const stamp = new Date().toISOString();
    const conversation = { id: 'retrieval-chat', title: 'Patent Training', modelName: 'qwen-test.gguf', createdAt: stamp, updatedAt: stamp, spaceId: 'space_general', messageCount: 0, totalTokens: 0 };
    const recovered = { searchedDocuments: 0, passages: 7, files: 7, scope: 'vault' };
    const oldFailure = { ...recovered, passages: 0, files: 0, unavailableReason: 'the active space has no indexed documents yet' };
    let messages: unknown[] = JSON.parse(localStorage.getItem('test:retrieval-messages') ?? '[]');
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:conversation|list_conversation_spaces') return [{ id: 'space_general', name: 'General', isArchived: false }];
      if (command === 'plugin:conversation|list_conversations_explorer' || command === 'plugin:conversation|list_conversations') return { conversations: [conversation], total: 1 };
      if (command === 'plugin:conversation|get_conversation') return { conversation };
      if (command === 'plugin:conversation|get_conversation_messages') return { messages, total: messages.length };
      if (command === 'plugin:conversation|list_message_bookmarks') return { bookmarks: [], total: 0 };
      if (command === 'plugin:batch|get_batch_history') return { jobs: [] };
      if (command === 'plugin:conversation|chat_with_conversation') {
        const request = args as { requestId: string; message: string };
        const emit = (retrieval: unknown) => callbacks.get(streamHandler!)?.({ payload: { conversationId: conversation.id, requestId: request.requestId, status: 'retrieval', retrieval, done: false } });
        // QA also publishes on this channel. Its tokens and completion must
        // neither enter this conversation nor detach its stream listener.
        callbacks.get(streamHandler!)?.({ payload: { type: 'token', content: 'Unrelated QA response', done: false } });
        callbacks.get(streamHandler!)?.({ payload: { type: 'done', done: true } });
        emit(oldFailure);
        document.documentElement.dataset.retrievalStage = 'initial';
        await new Promise<void>(resolve => window.addEventListener('test:recover-retrieval', () => resolve(), { once: true }));
        emit(recovered);
        document.documentElement.dataset.retrievalStage = 'recovered';
        await new Promise<void>(resolve => window.addEventListener('test:finish-retrieval', () => resolve(), { once: true }));
        messages = [
          { id: 'user', role: 'user', content: request.message, status: 'completed', createdAt: stamp },
          { id: 'assistant', role: 'assistant', content: 'I retrieved seven passages from your documents.', status: 'completed', createdAt: stamp, metadata: JSON.stringify({ retrieval: recovered }) },
        ];
        localStorage.setItem('test:retrieval-messages', JSON.stringify(messages));
        return { conversationId: conversation.id, messages, contextUsed: 0, sources: [] };
      }
      if (['plugin:conversation|list_journals', 'plugin:conversation|list_conversation_linked_documents', 'plugin:conversation|list_conversation_web_sources', 'plugin:model|list_downloaded_models', 'plugin:download|list_downloads', 'plugin:file|get_indexed_folders'].includes(command)) return [];
      throw new Error(`Unsupported retrieval fixture command: ${command}`);
    };
  }, settings);
  await page.goto('/chat?conversationId=retrieval-chat');
  const composer = page.getByRole('textbox', { name: 'Message composer' });
  await composer.fill('Help me learn the MPEP from my PDFs.');
  await composer.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('data-retrieval-stage', 'initial');
  await expect(page.getByText('Unrelated QA response', { exact: true })).toHaveCount(0);
  await expect(page.getByText(/no indexed documents|Answered without your documents|Document search was unavailable/)).toHaveCount(0);
  await page.evaluate(() => window.dispatchEvent(new Event('test:recover-retrieval')));
  await expect(page.getByText('Retrieved 7 passages from 7 files', { exact: true })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-retrieval')));
  await expect(page.locator('#message-assistant').getByText('I retrieved seven passages from your documents.')).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: 'Select conversation: Patent Training', exact: true }).press('Enter');
  await expect(page.getByText('Retrieved 7 passages from 7 files', { exact: true })).toBeVisible();
  await expect(page.getByText(/no indexed documents|Answered without your documents|Document search was unavailable/)).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-retrieval-recovered.png', animations: 'disabled' });
});

test('cancels a 43-document deletion after the pending document completes', async ({ page }) => {
  await page.addInitScript(settings => {
    let documents = Array.from({ length: 43 }, (_, index) => ({
      id: `delete-${index}`, fileName: `Delete ${index}.pdf`,
      filePath: `/home/test/.lattice/files/${String(index).padStart(64, '0')}/Delete ${index}.pdf`,
      fileType: 'pdf', category: 'Document', language: 'en', wordCount: 100,
      modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
    }));
    let calls = 0;
    (window as unknown as { __LATTICE_TEST_INVOKE__: (command: string, args?: unknown) => Promise<unknown> }).__LATTICE_TEST_INVOKE__ = async (command, args) => {
      if (command === 'plugin:settings|get_settings') return settings;
      if (command === 'plugin:file|list_all_documents') return documents;
      if (command === 'plugin:file|delete_document') {
        document.documentElement.dataset.deleteCalls = String(++calls);
        await new Promise<void>(resolve => window.addEventListener('test:finish-delete', () => resolve(), { once: true }));
        documents = documents.filter(doc => doc.id !== (args as { documentId: string }).documentId);
        document.documentElement.dataset.deleteFinished = 'true';
        return undefined;
      }
      if (command === 'plugin:health|initialize_database') return undefined;
      if (['plugin:download|list_downloads', 'plugin:file|get_indexed_folders', 'plugin:conversation|list_conversation_spaces', 'plugin:conversation|list_document_space_memberships'].includes(command)) return [];
      throw new Error(`Unsupported deletion fixture command: ${command}`);
    };
  }, makeAppSettings());
  await page.goto('/files');
  await page.getByRole('button', { name: 'List view', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Select Delete 0.pdf', exact: true }).click();
  await page.getByRole('button', { name: 'Select all', exact: true }).click();
  await page.getByLabel('Document selection actions').getByRole('button', { name: 'Delete', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Delete documents?' });
  await expect(dialog).toContainText('43 documents');
  await dialog.getByRole('button', { name: /Delete/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-delete-calls', '1');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-delete')));
  await expect(page.getByText('1 document deleted', { exact: true })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-delete-calls', '1');
  await expect(page.getByRole('checkbox', { name: 'Select Delete 0.pdf', exact: true })).toHaveCount(0);
  await expect(page.getByRole('checkbox', { name: 'Select Delete 1.pdf', exact: true })).toBeVisible();
});

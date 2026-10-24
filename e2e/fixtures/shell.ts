import { expect, test as base, type Page } from '@playwright/test';
import { makeAppSettings } from '../../src/tests/fixtures/appSettings';
import { installTauriMock, serveCommands, type MockArgs } from './tauri';

/** One fixed instant, so relative dates in fixtures read the same on every run. */
export const STAMP = '2026-10-20T09:30:00Z';

export interface ShellBackend {
  /** Every command answered in Node, in order. */
  calls: Array<{ command: string; args: MockArgs }>;
  /** Commands no fixture answered. */
  unsupported: string[];
  /** Uncaught renderer exceptions. */
  pageErrors: string[];
}

/**
 * The bridge plus what every route's shell asks for on start: settings,
 * downloads, background activity and the indexed folders. Route fixtures add
 * their own commands with another `serveCommands` call; handlers run in Node,
 * so their state outlives a reload the way the database does.
 */
export async function installShell(page: Page): Promise<ShellBackend> {
  const pageErrors: string[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  // Renderer checks must not wait for external font servers when DNS is offline.
  await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ contentType: 'text/css', body: '' }));
  const mock = await installTauriMock(page);
  const settings = makeAppSettings();
  const served = await serveCommands(page, () => ({
    get_settings: () => settings,
    initialize_database: () => 'ready',
    list_downloads: () => [],
    activity_snapshot: () => ({ items: [], running: 0 }),
    get_indexed_folders: () => [],
    get_index_progress: () => ({ is_indexing: false, current_file: null, files_processed: 0, total_files: 0, percent_complete: 0, failed: 0, status: 'idle', paused: false, failures: [] }),
    detect_system_capabilities: () => ({ total_ram_gb: 16, cpu_cores: 8, cpu_architecture: 'e2e', gpu_type: 'none', gpu_acceleration: 'none', vram_gb: null, available_disk_gb: 100 }),
    list_downloaded_models: () => [],
    list_journal_syntheses: () => [],
    list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
    // The sidebar's recent conversations; chat fixtures replace these.
    list_conversations_explorer: () => ({ conversations: [], total: 0 }),
    list_conversations: () => ({ conversations: [], total: 0 }),
  }));
  return { calls: served.calls, unsupported: mock.unsupported, pageErrors };
}

/**
 * A `test` whose `backend` fixture installs `install` before the page loads
 * and, after the test, fails on any command no fixture answered and on any
 * uncaught renderer exception.
 */
export function journeyTest<Backend extends ShellBackend>(install: (page: Page) => Promise<Backend>) {
  return base.extend<{ backend: Backend }>({
    backend: [async ({ page }, use) => {
      const backend = await install(page);
      await use(backend);
      expect(backend.unsupported, 'Every IPC dependency must have an explicit fixture contract').toEqual([]);
      expect(backend.pageErrors, 'No uncaught renderer exceptions').toEqual([]);
    }, { auto: true }],
  });
}

/** A backend reply held until the test releases it, so in-flight states can be asserted. */
export function gate() {
  let release!: () => void;
  const released = new Promise<void>(resolve => { release = resolve; });
  return { release, released };
}

/** Arguments of every served call to `command`, in order. */
export function argsOf(backend: ShellBackend, command: string) {
  return backend.calls.filter(call => call.command === command).map(call => call.args);
}

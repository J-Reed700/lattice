import type { Page } from '@playwright/test';
import type { ExplorerFileDto, ExplorerFolderDto, ExplorerEntryDto } from '../../src/lib/bindings';
import { makeAppSettings } from '../../src/tests/fixtures/appSettings';

export const ROOT = '/fixtures/Project café';
export const OTHER_ROOT = '/fixtures/Other project';

/** State lives outside the renderer so a reload cannot reset the simulated backend. */
export async function installExplorerFixture(page: Page) {
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  const unsupported: string[] = [];
  const pageErrors: string[] = [];
  const state = { failOpen: false, failSearch: false, closeCount: 0, picker: ROOT as string | null };
  const settings = makeAppSettings();
  const stamp = '2026-10-08T12:00:00Z';
  const folders: ExplorerFolderDto[] = [ROOT, OTHER_ROOT].map(root => ({
    root, name: root.split('/').pop()!, pinned: false, exists: true,
    addedAt: stamp, lastOpenedAt: stamp, threadCount: 1, instructions: null, spaceId: 'space_general',
    index: { state: 'indexed', filesTotal: 6, filesIndexed: 6, passagesTotal: 8, passagesEmbedded: 8, bytes: 1024, indexRoot: null, etaSeconds: null, message: null },
  }));
  const threads = folders.map((folder, i) => ({ id: `thread-${i}`, title: `${folder.name} discussion`, modelName: '', createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isArchived: false, explorerRoot: folder.root }));
  const entry = (path: string, kind: 'file' | 'directory' = 'file'): ExplorerEntryDto => ({ path, name: path.split('/').pop()!, kind, size: kind === 'file' ? 128 : null, ignored: false });
  const listings: Record<string, ExplorerEntryDto[]> = {
    '': [entry('src', 'directory'), entry('README.md'), entry('empty.txt'), entry('image.bin'), entry('large.log'), entry('missing.txt')],
    src: [entry('src/hello.ts'), entry('src/資料.ts')],
  };
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.exposeFunction('__explorerInvoke', async (command: string, args: Record<string, unknown> = {}) => {
    calls.push({ command, args });
    const request = (args.request ?? {}) as Record<string, unknown>;
    switch (command) {
      case 'plugin:model|check_first_run_status': return JSON.stringify({ needs_setup: false });
      case 'plugin:settings|get_settings': return settings;
      case 'plugin:health|initialize_database': return undefined;
      case 'plugin:file|get_index_progress': return { totalFiles: 0, processed: 0, failed: 0, status: 'idle', percentage: 0, paused: false, failures: [] };
      case 'plugin:file|get_indexing_stats': return { indexedDocuments: 0, totalChunks: 0 };
      case 'plugin:model|detect_system_capabilities': return { total_ram_gb: 8, cpu_cores: 8, cpu_architecture: 'e2e', gpu_type: 'none', gpu_acceleration: 'none', vram_gb: null, available_disk_gb: 10 };
      case 'plugin:qa|generate_chat_starters_wrapper': return { fingerprint: '', generatedAt: stamp, starters: [], documentCount: 0 };
      case 'plugin:dialog|open': return state.picker;
      case 'plugin:download|list_downloads':
      case 'plugin:batch|get_batch_history':
      case 'plugin:file|list_all_documents':
      case 'plugin:file|get_indexed_folders':
      case 'plugin:model|list_downloaded_models':
      case 'plugin:conversation|list_journals':
      case 'plugin:conversation|list_conversation_tangents':
      case 'plugin:conversation|list_conversation_linked_documents':
      case 'plugin:conversation|list_conversation_web_sources': return [];
      case 'plugin:conversation|list_conversation_spaces': return [{ id: 'space_general', name: 'General', isArchived: false }];
      case 'plugin:conversation|list_conversations_explorer':
      case 'plugin:conversation|list_conversations': return { conversations: threads, total: threads.length };
      case 'plugin:conversation|get_conversation': return { conversation: threads.find(thread => thread.id === request.conversationId) };
      case 'plugin:conversation|get_conversation_messages': return { messages: [], total: 0 };
      case 'plugin:conversation|list_message_bookmarks': return { bookmarks: [], total: 0 };
      case 'plugin:explorer|explorer_folders_list': return { home: '/fixtures', folders };
      case 'plugin:explorer|explorer_resolve_root': {
        if (state.failOpen) throw new Error('Folder permission denied');
        const folder = folders.find(folder => folder.root === args.path);
        if (!folder) throw new Error('Folder does not exist');
        return { root: folder.root, name: folder.name };
      }
      case 'plugin:explorer|explorer_index_open':
      case 'plugin:explorer|explorer_index_rebuild': return { root: args.root, indexRoot: args.root, state: 'ready', filesTotal: 6, filesIndexed: 6, passagesTotal: 8, passagesEmbedded: 8, passagesPerSecond: null, etaSeconds: null, message: null };
      case 'plugin:explorer|explorer_index_close': state.closeCount++; return undefined;
      case 'plugin:explorer|explorer_list_dir': return { root: args.root, path: args.path, entries: listings[String(args.path)] ?? [] };
      case 'plugin:explorer|explorer_read_file': {
        const path = String(args.path);
        if (path === 'missing.txt') throw new Error('File no longer exists');
        const text = path === 'empty.txt' ? '' : path === 'README.md' ? `# ${args.root}\nRead this project first.\n` : 'export const greeting = "café";\nexport const answer = 42;\n';
        const file: ExplorerFileDto = { root: String(args.root), path, text, language: path.endsWith('.ts') ? 'typescript' : null, sizeBytes: path === 'large.log' ? 5 * 1024 * 1024 : text.length, lineCount: text.split('\n').length, binary: path === 'image.bin', tooLarge: path === 'large.log' };
        return file;
      }
      case 'plugin:explorer|explorer_locate_file': return [];
      case 'plugin:explorer|explorer_search': {
        if (state.failSearch) throw new Error('Search index unavailable');
        const matches = String(args.query).includes('answer') ? [{ path: 'src/hello.ts', line: 2, column: 14, preview: 'export const answer = 42;' }] : [];
        return { matches, filesScanned: 6, truncated: false };
      }
      default: unsupported.push(command); throw new Error(`Unimplemented Explorer fixture command: ${command}`);
    }
  });
  await page.addInitScript(() => {
    let nextId = 1;
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();
    const listeners = new Map<number, { event: string; handler: number }>();
    Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback(callback: (...args: unknown[]) => unknown, once = false) {
        const id = nextId++;
        callbacks.set(id, (...args) => { if (once) callbacks.delete(id); return callback(...args); });
        return id;
      },
      unregisterCallback(id: number) { callbacks.delete(id); },
      async invoke(command: string, args: Record<string, unknown> = {}) {
        if (command === 'plugin:event|listen') {
          const id = nextId++;
          listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
          return id;
        }
        if (command === 'plugin:event|unlisten') { listeners.delete(Number(args.eventId)); return; }
        if (command === 'plugin:event|emit') {
          for (const [id, listener] of listeners) if (listener.event === args.event) callbacks.get(listener.handler)?.({ event: args.event, id, payload: args.payload });
          return;
        }
        return (window as unknown as { __explorerInvoke: (command: string, args: Record<string, unknown>) => Promise<unknown> }).__explorerInvoke(command, args);
      },
    } });
    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', { configurable: true, value: { unregisterListener: (id: number) => listeners.delete(id) } });
  });
  return { calls, unsupported, pageErrors, state };
}

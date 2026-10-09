import type { Page } from '@playwright/test';
import type { ExplorerFileDto, ExplorerFolderDto, ExplorerEntryDto } from '../../src/lib/bindings';
import { makeAppSettings } from '../../src/tests/fixtures/appSettings';
import { installTauriMock, serveCommands, type MockArgs } from './tauri';

export const ROOT = '/fixtures/Project café';
export const OTHER_ROOT = '/fixtures/Other project';

/** State lives outside the renderer so a reload cannot reset the simulated backend. */
export async function installExplorerFixture(page: Page) {
  const pageErrors: string[] = [];
  const state = { failOpen: false, failSearch: false, closeCount: 0, picker: ROOT as string | null };
  const settings = makeAppSettings();
  const stamp = '2026-10-08T12:00:00Z';
  const folders: ExplorerFolderDto[] = [ROOT, OTHER_ROOT].map(root => ({
    root, name: root.split('/').pop()!, pinned: false, exists: true,
    addedAt: stamp, lastOpenedAt: stamp, threadCount: 1, instructions: null, spaceId: 'space_general', lastThreadId: null,
    index: { state: 'indexed', filesTotal: 6, filesIndexed: 6, passagesTotal: 8, passagesEmbedded: 8, bytes: 1024, indexRoot: null, etaSeconds: null, message: null },
  }));
  const threads = folders.map((folder, i) => ({ id: `thread-${i}`, title: `${folder.name} discussion`, modelName: '', createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isArchived: false, explorerRoot: folder.root }));
  const entry = (path: string, kind: 'file' | 'directory' = 'file'): ExplorerEntryDto => ({ path, name: path.split('/').pop()!, kind, size: kind === 'file' ? 128 : null, ignored: false });
  const listings: Record<string, ExplorerEntryDto[]> = {
    '': [entry('src', 'directory'), entry('README.md'), entry('empty.txt'), entry('image.bin'), entry('large.log'), entry('missing.txt')],
    src: [entry('src/hello.ts'), entry('src/資料.ts')],
  };
  page.on('pageerror', error => pageErrors.push(error.message));
  const mock = await installTauriMock(page);
  const thread = (args: MockArgs) => threads.find(item => item.id === (args.request as { conversationId?: string } | undefined)?.conversationId);
  const indexStatus = (args: MockArgs) => ({ root: String(args.root), indexRoot: String(args.root), state: 'ready' as const, filesTotal: 6, filesIndexed: 6, passagesTotal: 8, passagesEmbedded: 8, passagesPerSecond: null, etaSeconds: null, message: null });
  const served = await serveCommands(page, () => ({
    get_settings: () => settings,
    initialize_database: () => 'ready',
    get_index_progress: () => ({ is_indexing: false, current_file: null, files_processed: 0, total_files: 0, percent_complete: 0, failed: 0, status: 'idle', paused: false, failures: [] }),
    get_indexing_stats: () => ({ indexedDocuments: 0, totalChunks: 0 }),
    detect_system_capabilities: () => ({ total_ram_gb: 8, cpu_cores: 8, cpu_architecture: 'e2e', gpu_type: 'none', gpu_acceleration: 'none', vram_gb: null, available_disk_gb: 10 }),
    generate_chat_starters_wrapper: () => ({ fingerprint: '', generatedAt: stamp, starters: [], documentCount: 0 }),
    'dialog|open': () => state.picker,
    list_downloads: () => [],
    get_batch_history: () => ({ jobs: [] }),
    list_all_documents: () => [],
    get_indexed_folders: () => [],
    list_downloaded_models: () => [],
    list_journals: () => [],
    list_conversation_tangents: () => [],
    list_conversation_linked_documents: () => [],
    list_conversation_web_sources: () => [],
    list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
    list_conversations_explorer: () => ({ conversations: threads, total: threads.length }),
    list_conversations: () => ({ conversations: threads, total: threads.length }),
    get_conversation: (args) => ({ conversation: thread(args) }),
    get_conversation_messages: () => ({ messages: [], total: 0 }),
    list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
    explorer_folders_list: () => ({ home: '/fixtures', folders }),
    explorer_folder_set_last_thread: (args) => {
      const folder = folders.find(item => item.root === args.root);
      if (folder) folder.lastThreadId = String(args.conversationId);
      return null;
    },
    explorer_resolve_root: (args) => {
      if (state.failOpen) throw new Error('Folder permission denied');
      const folder = folders.find(item => item.root === args.path);
      if (!folder) throw new Error('Folder does not exist');
      return { root: folder.root, name: folder.name };
    },
    explorer_index_open: indexStatus,
    explorer_index_rebuild: indexStatus,
    explorer_index_close: () => { state.closeCount++; return null; },
    explorer_list_dir: (args) => ({ root: String(args.root), path: String(args.path), entries: listings[String(args.path)] ?? [] }),
    explorer_read_file: (args) => {
      const path = String(args.path);
      if (path === 'missing.txt') throw new Error('File no longer exists');
      const text = path === 'empty.txt' ? '' : path === 'README.md' ? `# ${String(args.root)}\nRead this project first.\n` : 'export const greeting = "café";\nexport const answer = 42;\n';
      const file: ExplorerFileDto = { root: String(args.root), path, text, language: path.endsWith('.ts') ? 'typescript' : null, sizeBytes: path === 'large.log' ? 5 * 1024 * 1024 : text.length, lineCount: text.split('\n').length, binary: path === 'image.bin', tooLarge: path === 'large.log' };
      return file;
    },
    explorer_locate_file: () => [],
    explorer_search: (args) => {
      if (state.failSearch) throw new Error('Search index unavailable');
      const matches = String(args.query).includes('answer') ? [{ path: 'src/hello.ts', line: 2, column: 14, preview: 'export const answer = 42;' }] : [];
      return { matches, filesScanned: 6, truncated: false };
    },
  }));
  return { calls: served.calls, unsupported: mock.unsupported, pageErrors, state };
}

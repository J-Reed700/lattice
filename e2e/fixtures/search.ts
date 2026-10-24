import type { Page } from '@playwright/test';
import type { HybridSearchArgs, SearchResultDto } from '../../src/lib/bindings';
import { installShell, type ShellBackend } from './shell';
import { serveCommands } from './tauri';

export const PLANT_NOTES = '/home/test/Notes/Plant biology notes.md';
export const BOTANY_HISTORY = '/home/test/Archive/History of botany.pdf';
export const LIGHT_ARTICLE = 'https://example.org/botany/light-reactions';

function hit(id: string, documentId: string, title: string, path: string, content: string, score: number, extra: Partial<SearchResultDto> = {}): SearchResultDto {
  return {
    id, documentId, title, path, content, score, position: null,
    vectorScore: null, bm25Score: null, vectorRank: null, bm25Rank: null,
    ...extra,
  };
}

/** Passages by search mode. Two passages of the plant notes collapse into one result. */
const PASSAGES: Record<string, SearchResultDto[]> = {
  hybrid: [
    hit('plants-1', 'doc-plants', 'Plant biology notes.md', PLANT_NOTES, 'Photosynthesis turns light, water and carbon dioxide into glucose and oxygen.', 0.82, { vectorScore: 0.79, bm25Score: 7.4 }),
    hit('light-1', 'doc-light', 'The light reactions', LIGHT_ARTICLE, 'In the light reactions of photosynthesis, chlorophyll absorbs red and blue light.', 0.71),
    hit('plants-2', 'doc-plants', 'Plant biology notes.md', PLANT_NOTES, 'Stomata close in drought, which slows photosynthesis.', 0.64),
    hit('history-1', 'doc-history', 'History of botany.pdf', BOTANY_HISTORY, 'Jan Ingenhousz showed in 1779 that only the green parts of plants, in sunlight, refresh the air: an early step toward photosynthesis.', 0.41),
  ],
  keyword: [
    hit('plants-1', 'doc-plants', 'Plant biology notes.md', PLANT_NOTES, 'Photosynthesis turns light, water and carbon dioxide into glucose and oxygen.', 0.9, { bm25Score: 7.4 }),
  ],
};

export async function installSearchFixture(page: Page) {
  const shell = await installShell(page);
  const state = {
    /** While set, every search fails with this message. */
    searchError: null as string | null,
  };
  await serveCommands(page, () => ({
    hybrid_search: (args) => {
      if (state.searchError) throw new Error(state.searchError);
      const { query, search_mode } = args.args as HybridSearchArgs;
      return query.toLowerCase().includes('photosynthesis') ? PASSAGES[search_mode] ?? [] : [];
    },
    // The renderer asks the unified search path again when hybrid search comes back empty.
    search_documents: () => ({ results: [], fromCache: false, executionTimeMs: 4 }),
    open_file_by_id: (args) => {
      if (args.fileId === 'doc-history') throw new Error('History of botany.pdf is no longer in your library folder.');
      // A local note opens in the system viewer; nothing renders in the app.
      return { action: 'opened_external', fileType: 'text', contentPath: PLANT_NOTES, title: null };
    },
    // Search has no conversation, so no frozen copy of a web page exists for it.
    get_web_page_snapshot: () => null,
    open_file: () => {
      throw new Error('No file at /home/test/Archive/History of botany.pdf.');
    },
  }));
  return { ...shell, state } satisfies ShellBackend & { state: typeof state };
}

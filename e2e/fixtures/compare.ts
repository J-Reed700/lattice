import type { Page } from '@playwright/test';
import type { CompareDocumentsRequestDto, CompareRowDto, CompareTableDto } from '../../src/lib/bindings';
import { installShell, STAMP, type ShellBackend } from './shell';
import { serveCommands } from './tauri';

const FILES = '/home/test/.lattice/files';

/** Three recipes in the Library; Compare reads two of them. */
export const RECIPES = [
  {
    id: 'doc-shakshuka', fileName: 'Weeknight shakshuka.md',
    text: 'Ready in 25 minutes. Serves 4. Eggs are poached directly in a spiced tomato and pepper sauce.',
  },
  {
    id: 'doc-dal', fileName: 'Red lentil dal.md',
    text: 'Simmer for 40 minutes until the lentils collapse. Red lentils carry the dish; finish with a tarka of cumin seeds.',
  },
  {
    id: 'doc-coq', fileName: 'Coq au vin.md',
    text: 'Marinate the chicken overnight in red wine, then braise for two hours.',
  },
].map(recipe => ({ ...recipe, filePath: `${FILES}/${recipe.id}/${recipe.fileName}` }));

export const COLUMNS = ['prep time', 'servings', 'main protein'];

function row(id: string, cells: CompareRowDto['cells'], extra: Partial<CompareRowDto> = {}): CompareRowDto {
  const recipe = RECIPES.find(item => item.id === id)!;
  return { documentId: id, title: recipe.fileName, filePath: recipe.filePath, cells, error: null, degraded: null, ...extra };
}

const cite = (id: string, excerpt: string) => ({ chunkId: `${id}-chunk-1`, excerpt });

/** What the model fills in for each recipe; `null` is "the document does not say". */
const ROWS: Record<string, () => CompareRowDto> = {
  'doc-shakshuka': () => row('doc-shakshuka', [
    { value: '25 minutes', citation: cite('doc-shakshuka', 'Ready in 25 minutes.') },
    { value: '4', citation: cite('doc-shakshuka', 'Serves 4.') },
    { value: 'Eggs', citation: cite('doc-shakshuka', 'Eggs are poached directly in a spiced tomato and pepper sauce.') },
  ]),
  'doc-dal': () => row('doc-dal', [
    { value: '40 minutes', citation: cite('doc-dal', 'Simmer for 40 minutes until the lentils collapse.') },
    { value: null, citation: null },
    { value: 'Red lentils', citation: cite('doc-dal', 'Red lentils carry the dish; finish with a tarka of cumin seeds.') },
  ]),
  // The model could not finish this one: the row says why, unfilled cells stay "not stated".
  'doc-coq': () => row('doc-coq', [
    { value: 'Overnight plus two hours', citation: cite('doc-coq', 'Marinate the chicken overnight in red wine, then braise for two hours.') },
    { value: null, citation: null },
    { value: null, citation: null },
  ], { error: 'Stopped after the first column: the model took too long on this document.' }),
};

export async function installCompareFixture(page: Page) {
  const shell = await installShell(page);
  const state = {
    /** The next compare call fails with this message, once. */
    failNext: null as string | null,
    /** Holds compare replies until released. */
    hold: null as Promise<void> | null,
    captured: [] as string[],
  };
  await serveCommands(page, () => ({
    list_all_documents: () => RECIPES.map(recipe => ({
      id: recipe.id, fileName: recipe.fileName, filePath: recipe.filePath,
      fileType: 'md', category: 'Document', language: 'en', wordCount: 300,
      modifiedAt: STAMP, indexedAt: STAMP,
    })),
    list_custom_collections: () => [],
    list_conversation_spaces: () => [],
    list_document_space_memberships: () => [],
    compare_documents: async (args) => {
      const { documentIds, columns } = args.request as CompareDocumentsRequestDto;
      if (state.hold) await state.hold;
      if (state.failNext) {
        const message = state.failNext;
        state.failNext = null;
        throw new Error(message);
      }
      const table: CompareTableDto = {
        columns,
        rows: documentIds.map(id => ROWS[id]()),
        modelName: 'Qwen3.5-9B',
        generatedAt: STAMP,
      };
      return table;
    },
    read_file_content: (args) => {
      const recipe = RECIPES.find(item => item.filePath === args.path);
      if (!recipe) throw new Error(`No such file: ${String(args.path)}`);
      return `# ${recipe.fileName.replace(/\.md$/, '')}\n\n${recipe.text}\n`;
    },
    quick_capture: (args) => {
      state.captured.push(String(args.content));
      return { noteId: 'note-kitchen', noteTitle: 'Kitchen notebook', created: false };
    },
  }));
  return { ...shell, state } satisfies ShellBackend & { state: typeof state };
}

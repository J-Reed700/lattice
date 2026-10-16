import tsParser from '@typescript-eslint/parser';
import { ESLint, Linter } from 'eslint';
import { describe, expect, it } from 'vitest';

/**
 * Settings is the only global switch for the chat model. The lint rule that
 * keeps it so must catch the shapes the old chat picker used, in the places
 * it used them, and must not cost views the native-access rule it shares
 * `no-restricted-syntax` with.
 */

// Vitest runs from the repository root, where eslint.config.js lives.
const ROOT = process.cwd();

const GLOBAL_SWITCHES = `
const models = useDownloadedModels();
void models.setActiveChatModel('ornith');
const { clearActiveChatModel } = useDownloadedModels();
void VaultAPI.updateSettings({ category: 'llm', updates: { provider: 'local' } });
`;
const NATIVE_IMPORT = `import { open } from '@tauri-apps/plugin-dialog';\nvoid open;\n`;

async function lint(path: string, code: string): Promise<string[]> {
  const config = await new ESLint({ cwd: ROOT }).calculateConfigForFile(path);
  const rule = (config.rules as Record<string, Linter.RuleEntry> | undefined)?.['no-restricted-syntax'];
  if (!rule) return [];
  const linter = new Linter({ configType: 'flat' });
  return linter
    .verify(code, [{ files: ['**/*.ts'], languageOptions: { parser: tsParser }, rules: { 'no-restricted-syntax': rule } }], 'fixture.ts')
    .map((message) => message.message);
}

describe('only Settings changes the app-wide chat model', () => {
  it.each([
    'src/features/chat/hooks/useChatSwitchActions.ts',
    'src/features/chat/components/message/useMessageActions.ts',
    'src/features/explorer/hooks/useExplorerThread.ts',
  ])('fails %s for every global switch', async (path) => {
    const messages = await lint(path, GLOBAL_SWITCHES);
    expect(messages).toHaveLength(3);
    for (const message of messages) expect(message).toMatch(/^Only Settings changes the app-wide chat model/);
  });

  it.each([
    'src/features/settings/components/modelRoles/ModelRolesContext.tsx',
    'src/features/model/hooks/useDownloadedModels.ts',
  ])('lets %s set it', async (path) => {
    expect(await lint(path, GLOBAL_SWITCHES)).toEqual([]);
  });

  it('keeps the native-access rule on views', async () => {
    const messages = await lint('src/features/chat/components/ChatPanel.tsx', NATIVE_IMPORT + GLOBAL_SWITCHES);
    expect(messages).toHaveLength(4);
    expect(messages[0]).toMatch(/^Views reach native plugins/);
  });
});

import { LanguageSupport, syntaxTree } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import {
  codeLanguages,
  languageForFence,
  loadedLanguage,
  loadLanguage,
  PLAIN_TEXT_LABEL,
  resolveLanguage,
} from '../languages';

const idOf = (path: string, backendId?: string | null) => resolveLanguage(path, backendId)?.id ?? null;

// Every name `language_for` in src-tauri/src/features/explorer/fs.rs returns.
const BACKEND_IDS = [
  'dockerfile', 'makefile', 'rust', 'typescript', 'tsx', 'javascript', 'jsx', 'python', 'go', 'java', 'kotlin',
  'swift', 'c', 'cpp', 'csharp', 'ruby', 'php', 'shell', 'json', 'toml', 'yaml', 'markdown', 'html', 'css', 'scss',
  'sql', 'xml', 'lua', 'dart', 'zig', 'elixir', 'haskell', 'scala', 'r', 'vue', 'svelte',
];
// No CodeMirror grammar or mode exists for these; they read as plain text.
const UNCOVERED = ['zig', 'elixir'];

describe('resolveLanguage', () => {
  it('reads the extension of the file name, in any case and under any folder', () => {
    expect(idOf('src/audio/Processor.cpp')).toBe('cpp');
    expect(idOf('src/audio/Processor.HPP')).toBe('cpp');
    expect(idOf('include/juce_core.h')).toBe('c');
    expect(idOf('docs/Architecture.md')).toBe('markdown');
    expect(idOf('presets/factory.json')).toBe('json');
    expect(idOf('tools/gen.py')).toBe('python');
    expect(idOf('scripts/build.sh')).toBe('shell');
    expect(idOf('C:\\work\\MusicVST\\Source\\Plugin.mm')).toBe('objectivecpp');
    expect(idOf('types/index.d.ts')).toBe('typescript');
    expect(idOf('lesson.py?draft=1#cell')).toBe('python');
  });

  it('lets a whole file name decide before its extension, ignoring case', () => {
    expect(idOf('CMakeLists.txt')).toBe('cmake');
    expect(idOf('modules/dsp/cmakelists.TXT')).toBe('cmake');
    expect(idOf('notes.txt')).toBeNull();
    expect(idOf('Dockerfile')).toBe('dockerfile');
    expect(idOf('Makefile')).toBe('makefile');
    expect(idOf('makefile')).toBe('makefile');
    expect(idOf('Cargo.lock')).toBe('toml');
    expect(idOf('Gemfile')).toBe('ruby');
    expect(idOf('/Users/me/.zshrc')).toBe('shell');
    expect(idOf('.clang-format')).toBe('yaml');
    expect(idOf('.env')).toBe('ini');
  });

  it("falls back to the backend's language, then to plain text", () => {
    expect(idOf('scripts/run', 'shell')).toBe('shell');
    expect(idOf('Processor.cpp', 'python')).toBe('cpp');
    expect(idOf('build.zig', 'zig')).toBeNull();
    expect(idOf('LICENSE')).toBeNull();
    expect(idOf('.gitignore', null)).toBeNull();
    expect(PLAIN_TEXT_LABEL).toBe('Plain text');
  });

  it("covers every language the backend names, but the ones CodeMirror has nothing for", () => {
    const missing = BACKEND_IDS.filter((id) => idOf('no-extension', id) === null);
    expect(missing).toEqual(UNCOVERED);
  });

  it('claims each id, extension and file name once', () => {
    const claims = (pick: (language: (typeof codeLanguages)[number]) => readonly string[]) =>
      codeLanguages.flatMap((language) => pick(language).map((name) => name.toLowerCase()));
    for (const names of [claims((language) => [language.id]), claims((language) => language.extensions), claims((language) => language.filenames ?? [])]) {
      expect(names.filter((name, index) => names.indexOf(name) !== index)).toEqual([]);
    }
  });
});

describe('languageForFence', () => {
  it('reads a Markdown fence by id, alias or extension', () => {
    expect(languageForFence('cpp')?.id).toBe('cpp');
    expect(languageForFence('C++')?.id).toBe('cpp');
    expect(languageForFence('rust,ignore')?.id).toBe('rust');
    expect(languageForFence('sh')?.id).toBe('shell');
    expect(languageForFence('bash title="setup"')?.id).toBe('shell');
    expect(languageForFence('py')?.id).toBe('python');
    expect(languageForFence('cmake')?.id).toBe('cmake');
    expect(languageForFence('mermaid')).toBeNull();
    expect(languageForFence('')).toBeNull();
  });
});

describe('loadLanguage', () => {
  it('loads a grammar on first use and hands back the same one after', async () => {
    const toml = resolveLanguage('Cargo.toml')!;
    expect(loadedLanguage(toml)).toBeUndefined();
    const support = await loadLanguage(toml);
    expect(support).toBeInstanceOf(LanguageSupport);
    expect(support.extension).toBeTruthy();
    expect(await loadLanguage(toml)).toBe(support);
    expect(loadedLanguage(toml)).toBe(support);
  });

  it('loads every language in the registry', async () => {
    const supports = await Promise.all(codeLanguages.map(loadLanguage));
    for (const [index, support] of supports.entries()) {
      expect(support, codeLanguages[index]!.id).toBeInstanceOf(LanguageSupport);
      // The grammar parses its own kind of text without throwing.
      const state = EditorState.create({ doc: 'name = "value" // 42\n', extensions: support });
      expect(syntaxTree(state).length).toBeGreaterThan(0);
    }
  });

  it('colors fenced Markdown blocks through the same registry', async () => {
    const markdown = await loadLanguage(resolveLanguage('README.md')!);
    await loadLanguage(resolveLanguage('main.cpp')!);
    const doc = '# Build\n\n```cpp\n#include <vector>\nint main() { return 0; }\n```\n';
    const state = EditorState.create({ doc, extensions: markdown });
    const node = syntaxTree(state).resolveInner(doc.indexOf('int main'), 1);
    expect(node.name).toBe('PrimitiveType');
  });
});

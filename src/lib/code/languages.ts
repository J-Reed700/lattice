import { foldService, LanguageDescription, LanguageSupport, StreamLanguage, type StreamParser } from '@codemirror/language';
import { countColumn } from '@codemirror/state';

/**
 * One language the code views can color. Adding a language is one entry in
 * `codeLanguages` plus its package: the grammar is imported the first time a
 * file of that language opens, so none of them reach the startup bundle.
 */
export interface CodeLanguage {
  /** The backend's name for it (`language_for` in explorer/fs.rs) where it has one. */
  id: string;
  label: string;
  /** Lower case, without the dot. */
  extensions: readonly string[];
  /** Whole file names that decide the language on their own, e.g. `CMakeLists.txt`. */
  filenames?: readonly string[];
  /** Other names a Markdown fence may use for it (```c++, ```sh). */
  aliases?: readonly string[];
  load: () => Promise<LanguageSupport>;
}

export const PLAIN_TEXT_LABEL = 'Plain text';

const indentOf = (text: string, tabSize: number) => countColumn(/^\s*/.exec(text)![0], tabSize);

/**
 * Folds a line together with the more-indented lines under it. Grammars carry
 * their own folds; CodeMirror 5 modes have no tree, so they fold this way.
 */
const indentationFolding = foldService.of((state, lineStart) => {
  const line = state.doc.lineAt(lineStart);
  if (!line.text.trim()) return null;
  const indent = indentOf(line.text, state.tabSize);
  let last = 0;
  for (let number = line.number + 1; number <= state.doc.lines; number += 1) {
    const text = state.doc.line(number).text;
    if (!text.trim()) continue;
    if (indentOf(text, state.tabSize) <= indent) break;
    last = number;
  }
  return last ? { from: line.to, to: state.doc.line(last).to } : null;
});

/**
 * Wraps a CodeMirror 5 mode. `rename` maps the mode's token names where its
 * meaning differs from the shared reading: CMake calls its commands `def`,
 * Shell its variables, `.properties` its keys.
 */
function legacy<State>(parser: StreamParser<State>, rename?: Record<string, string>): LanguageSupport {
  const token: StreamParser<State>['token'] = rename
    ? (stream, state) => {
      const style = parser.token(stream, state);
      return (style && rename[style]) ?? style;
    }
    : parser.token;
  return new LanguageSupport(StreamLanguage.define({ ...parser, token }), indentationFolding);
}

const cpp = () => import('@codemirror/lang-cpp').then(({ cpp }) => cpp());
const javascript = (config: { jsx?: boolean; typescript?: boolean }) =>
  import('@codemirror/lang-javascript').then(({ javascript }) => javascript(config));
const html = () => import('@codemirror/lang-html').then(({ html }) => html());
const clike = (mode: 'kotlin' | 'dart' | 'scala' | 'objectiveC' | 'objectiveCpp') =>
  import('@codemirror/legacy-modes/mode/clike').then((modes) => legacy(modes[mode]));
// `$(…)` is a command, not a quotation.
const shell = () => import('@codemirror/legacy-modes/mode/shell')
  .then(({ shell }) => legacy(shell, { def: 'variableName.special', quote: 'string.special' }));

export const codeLanguages: readonly CodeLanguage[] = [
  // Grammars: a real parse tree, so folding and nested languages come free.
  { id: 'c', label: 'C', extensions: ['c', 'h'], load: cpp },
  {
    id: 'cpp',
    label: 'C++',
    extensions: ['cc', 'cpp', 'cxx', 'c++', 'hpp', 'hh', 'hxx', 'h++', 'ipp', 'inl', 'tpp'],
    aliases: ['c++'],
    load: cpp,
  },
  { id: 'csharp', label: 'C#', extensions: ['cs', 'csx'], aliases: ['c#'], load: () => import('@replit/codemirror-lang-csharp').then(({ csharp }) => csharp()) },
  { id: 'css', label: 'CSS', extensions: ['css'], load: () => import('@codemirror/lang-css').then(({ css }) => css()) },
  { id: 'go', label: 'Go', extensions: ['go'], aliases: ['golang'], load: () => import('@codemirror/lang-go').then(({ go }) => go()) },
  { id: 'html', label: 'HTML', extensions: ['html', 'htm', 'xhtml'], load: html },
  { id: 'java', label: 'Java', extensions: ['java'], load: () => import('@codemirror/lang-java').then(({ java }) => java()) },
  { id: 'javascript', label: 'JavaScript', extensions: ['js', 'mjs', 'cjs'], aliases: ['node'], load: () => javascript({}) },
  { id: 'jsx', label: 'JSX', extensions: ['jsx'], load: () => javascript({ jsx: true }) },
  {
    id: 'json',
    label: 'JSON',
    extensions: ['json', 'jsonc', 'webmanifest'],
    filenames: ['.babelrc', '.eslintrc', '.prettierrc'],
    load: () => import('@codemirror/lang-json').then(({ json }) => json()),
  },
  {
    id: 'markdown',
    label: 'Markdown',
    extensions: ['md', 'markdown', 'mdx'],
    aliases: ['md'],
    // Fenced blocks color through this same registry, loading on first sight.
    load: () => import('@codemirror/lang-markdown').then(({ markdown, markdownLanguage }) =>
      markdown({ base: markdownLanguage, codeLanguages: (info) => fenceDescription(info) })),
  },
  { id: 'php', label: 'PHP', extensions: ['php'], load: () => import('@codemirror/lang-php').then(({ php }) => php()) },
  { id: 'python', label: 'Python', extensions: ['py', 'pyi', 'pyw'], aliases: ['python3'], load: () => import('@codemirror/lang-python').then(({ python }) => python()) },
  { id: 'rust', label: 'Rust', extensions: ['rs'], load: () => import('@codemirror/lang-rust').then(({ rust }) => rust()) },
  { id: 'sass', label: 'Sass', extensions: ['sass'], load: () => import('@codemirror/lang-sass').then(({ sass }) => sass({ indented: true })) },
  { id: 'scss', label: 'SCSS', extensions: ['scss'], load: () => import('@codemirror/lang-sass').then(({ sass }) => sass()) },
  { id: 'sql', label: 'SQL', extensions: ['sql'], load: () => import('@codemirror/lang-sql').then(({ sql }) => sql()) },
  // Svelte has no official grammar; HTML colors its markup, script and style.
  { id: 'svelte', label: 'Svelte', extensions: ['svelte'], load: html },
  { id: 'tsx', label: 'TSX', extensions: ['tsx'], load: () => javascript({ typescript: true, jsx: true }) },
  { id: 'typescript', label: 'TypeScript', extensions: ['ts', 'mts', 'cts'], load: () => javascript({ typescript: true }) },
  { id: 'vue', label: 'Vue', extensions: ['vue'], load: () => import('@codemirror/lang-vue').then(({ vue }) => vue()) },
  {
    id: 'xml',
    label: 'XML',
    extensions: ['xml', 'svg', 'plist', 'xsd', 'xsl', 'xslt', 'xaml', 'csproj', 'vcxproj', 'props', 'targets', 'resx', 'storyboard', 'xib', 'jucer'],
    load: () => import('@codemirror/lang-xml').then(({ xml }) => xml()),
  },
  {
    id: 'yaml',
    label: 'YAML',
    extensions: ['yaml', 'yml'],
    filenames: ['.clang-format', '.clang-tidy'],
    load: () => import('@codemirror/lang-yaml').then(({ yaml }) => yaml()),
  },

  // CodeMirror 5 modes, one chunk each (the C-like family shares one).
  {
    id: 'cmake',
    label: 'CMake',
    extensions: ['cmake'],
    filenames: ['CMakeLists.txt'],
    load: () => import('@codemirror/legacy-modes/mode/cmake').then(({ cmake }) => legacy({
      ...cmake,
      // The mode takes one digit at a time; `3.22` is one number.
      token: (stream, state) => {
        const style = cmake.token(stream, state);
        if (style === 'number') stream.match(/^[\d.]*/);
        return style;
      },
    }, { def: 'variableName.function' })),
  },
  { id: 'dart', label: 'Dart', extensions: ['dart'], load: () => clike('dart') },
  { id: 'diff', label: 'Diff', extensions: ['diff', 'patch'], load: () => import('@codemirror/legacy-modes/mode/diff').then(({ diff }) => legacy(diff)) },
  {
    id: 'dockerfile',
    label: 'Dockerfile',
    extensions: ['dockerfile'],
    filenames: ['Dockerfile', 'Containerfile'],
    aliases: ['docker'],
    load: () => import('@codemirror/legacy-modes/mode/dockerfile').then(({ dockerFile }) => legacy(dockerFile)),
  },
  {
    id: 'groovy',
    label: 'Groovy',
    extensions: ['groovy', 'gradle'],
    filenames: ['Jenkinsfile'],
    load: () => import('@codemirror/legacy-modes/mode/groovy').then(({ groovy }) => legacy(groovy)),
  },
  { id: 'haskell', label: 'Haskell', extensions: ['hs'], load: () => import('@codemirror/legacy-modes/mode/haskell').then(({ haskell }) => legacy(haskell)) },
  {
    id: 'ini',
    label: 'INI',
    extensions: ['ini', 'cfg', 'conf', 'properties', 'env'],
    filenames: ['.editorconfig', '.gitconfig'],
    aliases: ['properties'],
    load: () => import('@codemirror/legacy-modes/mode/properties').then(({ properties }) => legacy(properties, { def: 'propertyName' })),
  },
  { id: 'kotlin', label: 'Kotlin', extensions: ['kt', 'kts'], load: () => clike('kotlin') },
  { id: 'lua', label: 'Lua', extensions: ['lua'], load: () => import('@codemirror/legacy-modes/mode/lua').then(({ lua }) => legacy(lua)) },
  // Make has no mode of its own; recipes are shell, and so are `$(…)` and `#`.
  { id: 'makefile', label: 'Makefile', extensions: ['mk', 'mak'], filenames: ['Makefile', 'GNUmakefile'], aliases: ['make'], load: shell },
  { id: 'objectivec', label: 'Objective-C', extensions: ['m'], aliases: ['objc'], load: () => clike('objectiveC') },
  { id: 'objectivecpp', label: 'Objective-C++', extensions: ['mm'], aliases: ['objc++'], load: () => clike('objectiveCpp') },
  { id: 'perl', label: 'Perl', extensions: ['pl', 'pm'], load: () => import('@codemirror/legacy-modes/mode/perl').then(({ perl }) => legacy(perl)) },
  {
    id: 'powershell',
    label: 'PowerShell',
    extensions: ['ps1', 'psm1', 'psd1'],
    aliases: ['pwsh'],
    load: () => import('@codemirror/legacy-modes/mode/powershell').then(({ powerShell }) => legacy(powerShell)),
  },
  { id: 'protobuf', label: 'Protocol Buffers', extensions: ['proto'], load: () => import('@codemirror/legacy-modes/mode/protobuf').then(({ protobuf }) => legacy(protobuf)) },
  { id: 'r', label: 'R', extensions: ['r'], load: () => import('@codemirror/legacy-modes/mode/r').then(({ r }) => legacy(r)) },
  {
    id: 'ruby',
    label: 'Ruby',
    extensions: ['rb', 'rake', 'gemspec'],
    filenames: ['Gemfile', 'Rakefile', 'Podfile', 'Brewfile'],
    load: () => import('@codemirror/legacy-modes/mode/ruby').then(({ ruby }) => legacy(ruby)),
  },
  { id: 'scala', label: 'Scala', extensions: ['scala', 'sbt'], load: () => clike('scala') },
  {
    id: 'shell',
    label: 'Shell',
    extensions: ['sh', 'bash', 'zsh', 'ksh'],
    filenames: ['.bashrc', '.bash_profile', '.zshrc', '.zprofile', '.profile'],
    aliases: ['shellscript', 'console'],
    load: shell,
  },
  { id: 'swift', label: 'Swift', extensions: ['swift'], load: () => import('@codemirror/legacy-modes/mode/swift').then(({ swift }) => legacy(swift)) },
  {
    id: 'toml',
    label: 'TOML',
    extensions: ['toml'],
    filenames: ['Cargo.lock', 'Pipfile'],
    load: () => import('@codemirror/legacy-modes/mode/toml').then(({ toml }) => legacy(toml)),
  },
];

const byId = new Map(codeLanguages.map((language) => [language.id, language]));
const byFilename = new Map(codeLanguages.flatMap((language) =>
  (language.filenames ?? []).map((name) => [name.toLowerCase(), language] as const)));
const byExtension = new Map(codeLanguages.flatMap((language) =>
  language.extensions.map((extension) => [extension, language] as const)));
const byFenceName = new Map(codeLanguages.flatMap((language) =>
  [language.id, ...(language.aliases ?? [])].map((name) => [name, language] as const)));

/**
 * The language for a file: its whole name first (`CMakeLists.txt`), then its
 * extension, then the backend's name for it. `null` means plain text.
 */
export function resolveLanguage(path: string, backendId?: string | null): CodeLanguage | null {
  const name = (path.split(/[?#]/, 1)[0] ?? '').split(/[\\/]/).pop()!.toLowerCase();
  const dot = name.lastIndexOf('.');
  return byFilename.get(name)
    ?? (dot >= 0 ? byExtension.get(name.slice(dot + 1)) : undefined)
    ?? (backendId ? byId.get(backendId) : undefined)
    ?? null;
}

/** The language a Markdown fence names (```cpp, ```c++, ```rs), if any. */
export function languageForFence(info: string): CodeLanguage | null {
  const name = info.trim().split(/[\s,{]/, 1)[0]?.toLowerCase() ?? '';
  if (!name) return null;
  return byFenceName.get(name) ?? byExtension.get(name) ?? null;
}

const descriptions = new WeakMap<CodeLanguage, LanguageDescription>();

/**
 * CodeMirror's lazy handle on a language. It loads once and keeps the result,
 * so a viewer, an editor and Markdown fences all share one load.
 */
function descriptionOf(language: CodeLanguage): LanguageDescription {
  let description = descriptions.get(language);
  if (!description) {
    description = LanguageDescription.of({
      name: language.label,
      alias: [language.id, ...(language.aliases ?? [])],
      extensions: [...language.extensions],
      load: language.load,
    });
    descriptions.set(language, description);
  }
  return description;
}

function fenceDescription(info: string): LanguageDescription | null {
  const language = languageForFence(info);
  return language ? descriptionOf(language) : null;
}

/** Loads the grammar; later calls get the same promise, then the same support. */
export function loadLanguage(language: CodeLanguage): Promise<LanguageSupport> {
  return descriptionOf(language).load();
}

/** The grammar if an earlier load already finished, so a view can start colored. */
export function loadedLanguage(language: CodeLanguage): LanguageSupport | undefined {
  return descriptionOf(language).support;
}

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import js from '@eslint/js';
import tsPlugin from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import reactPlugin from 'eslint-plugin-react';
import reactHooksPlugin from 'eslint-plugin-react-hooks';
import importPlugin from 'eslint-plugin-import';
import globals from 'globals';

const RAW_INVOKE_MESSAGE =
  'Only src/shared/ipc/transport.ts calls invoke(). Add a wrapper to the owning ' +
  'feature client (src/features/<feature>/api/client.ts) and call that.';

// The transport is the one way into the backend, so it is the one place that
// wraps invoke(): routing, error normalization and diagnostics live there.
const RAW_INVOKE_IMPORTS = [
  { name: '@tauri-apps/api/core', importNames: ['invoke'], message: RAW_INVOKE_MESSAGE },
  { name: '@tauri-apps/api', importNames: ['core'], message: RAW_INVOKE_MESSAGE },
];

const VIEW_BACKEND_IMPORTS = [
  '@/lib/api', '**/lib/api',
  '@/lib/bindings', '**/lib/bindings',
  '@/shared/ipc/**', '**/shared/ipc/**',
  '**/api/client', '**/files/api/documents',
];

const VIEW_NATIVE_MESSAGE =
  'Views reach native plugins and asset URLs through a hook or shared helper, not @tauri-apps directly.';
const VIEW_NATIVE_SELECTORS = [
  { selector: 'ImportDeclaration[source.value=/^@tauri-apps\\u002Fplugin-/]', message: VIEW_NATIVE_MESSAGE },
  { selector: 'ImportExpression[source.value=/^@tauri-apps\\u002Fplugin-/]', message: VIEW_NATIVE_MESSAGE },
  {
    selector: "ImportDeclaration[source.value='@tauri-apps/api/core'] ImportSpecifier[imported.name='convertFileSrc']",
    message: VIEW_NATIVE_MESSAGE,
  },
];

const ROOT = fileURLToPath(new URL('.', import.meta.url));

/**
 * Returns an allowlist once every entry still breaks the rule it is exempt
 * from. Fixing, moving or deleting a listed file fails the lint run until its
 * entry goes, so these lists can only shrink.
 */
function ratchet(name, files, breaksRule) {
  const stale = files.filter((file) => {
    const path = `${ROOT}${file}`;
    return !existsSync(path) || !breaksRule(readFileSync(path, 'utf8'));
  });
  if (stale.length > 0) {
    throw new Error(`eslint.config.js: remove these ${name} entries, the files no longer need them:\n  ${stale.join('\n  ')}`);
  }
  return files;
}

const valueImportSources = (text) =>
  [...text.matchAll(/^\s*(?:import|export)\s+(?!type\s)[^;]*?\bfrom\s+['"]([^'"]+)['"]/gm)].map((match) => match[1]);
const isBackendSource = (source) =>
  /(^|\/)(lib\/api|lib\/bindings|api\/client|files\/api\/documents)$/.test(source) || /(^|\/)shared\/ipc(\/|$)/.test(source);

// View files that imported VaultAPI, a feature client or the transport when
// the rule arrived. Do not add to this list: move the file's backend calls
// into a query or mutation hook, then delete its entry.
const VIEW_BACKEND_ALLOWLIST = ratchet('VIEW_BACKEND_ALLOWLIST', [
  'src/features/chat/components/ChatView.tsx',
  'src/features/chat/components/ConversationLinkedDocumentsPanel.tsx',
  'src/features/chat/components/ConversationSpotlight.tsx',
  'src/features/chat/components/KnowledgePanel.tsx',
  'src/features/chat/components/sidebar/workspaceQueries.ts',
  'src/features/chat/components/tangents/ConversationTangents.tsx',
  'src/features/chat/components/tangents/TangentPanel.tsx',
  'src/features/dashboard/components/Dashboard.tsx',
  'src/features/explorer/components/ExplorerFileView.tsx',
  'src/features/explorer/components/ExplorerSearch.tsx',
  'src/features/explorer/components/ExplorerTree.tsx',
  'src/features/files/components/Ingest/BatchFileImport.tsx',
  'src/features/files/components/Ingest/LibraryFilePicker.tsx',
  'src/features/files/components/IngestHub/IngestHub.tsx',
  'src/features/journal/components/EntryFromConversation.tsx',
  'src/features/journal/components/JournalWorkspace.tsx',
  'src/features/model/components/FirstRun/FirstRunGate.tsx',
  'src/features/model/components/FirstRun/ModelSetupModal.tsx',
  'src/features/palette/components/CommandPalette/CommandPalette.tsx',
  'src/features/reading/components/ContentViewer/ContentViewer.tsx',
  'src/features/reading/components/ContentViewer/renderers/DocxViewer.tsx',
  'src/features/reading/components/ContentViewer/renderers/HTMLViewer.tsx',
  'src/features/reading/components/ContentViewer/renderers/PDFViewerImpl.tsx',
  'src/features/reading/components/ContentViewer/renderers/TextViewer.tsx',
  'src/features/reading/components/JournalCapturePreview.tsx',
  'src/features/reading/components/SourceReaderBody.tsx',
  'src/features/search/components/SearchInterface/SearchInterface.tsx',
  'src/features/settings/components/AITab/ChatTab.tsx',
  'src/features/settings/components/AITab/LlamaCppConnection.tsx',
  'src/features/settings/components/AITab/ModelsTab.tsx',
  'src/features/settings/components/AITab/ToolsTab.tsx',
  'src/features/settings/components/BackupSection.tsx',
  'src/features/settings/components/IndexingTab.tsx',
  'src/features/settings/components/ModelCatalog/ModelVariantPicker.tsx',
  'src/features/settings/components/ModelCatalog/startModelDownload.ts',
  'src/features/settings/components/modelRoles/LocalModelRow.tsx',
  'src/features/settings/components/Settings.tsx',
], (text) => valueImportSources(text).some(isBackendSource));

// View files that imported a Tauri plugin or convertFileSrc when the rule
// arrived. Do not add to this list: wrap the native call in a hook or shared
// helper, then delete the file's entry.
const VIEW_NATIVE_ALLOWLIST = ratchet('VIEW_NATIVE_ALLOWLIST', [
  'src/features/explorer/components/ExplorerPage.tsx',
  'src/features/files/components/FileBrowser/FileBrowser.tsx',
  'src/features/files/components/Ingest/ImportHistory.tsx',
  'src/features/journal/components/JournalPickerMenu.tsx',
  'src/features/journal/components/JournalWorkspace.tsx',
  'src/features/reading/components/ContentViewer/renderers/HTMLViewer.tsx',
  'src/features/reading/components/SourceReaderBody.tsx',
  'src/features/settings/components/AITab/ModelsTab.tsx',
  'src/features/settings/components/AITab/ToolsTab.tsx',
  'src/features/settings/components/IndexingTab.tsx',
  'src/features/settings/components/LogsTab.tsx',
  'src/features/settings/components/Settings.tsx',
  'src/features/settings/components/VaultTab.tsx',
], (text) =>
  /\bfrom\s+['"]@tauri-apps\/plugin-|\bimport\(\s*['"]@tauri-apps\/plugin-/.test(text) ||
  /import\s*\{[^}]*\bconvertFileSrc\b[^}]*\}\s*from\s*['"]@tauri-apps\/api\/core['"]/.test(text));

export default [
  // Ignore patterns (migrated from .eslintignore)
  {
    ignores: [
      'dist/**',
      'build/**',
      'node_modules/**',
      '*.config.js',
      '*.config.ts',
      'src-tauri/**', // Rust source directory
      'public/**',
      'coverage/**',
      '.tauri/**',
      'e2e-results/**',
      'playwright-report/**',
      'src/lib/bindings.ts', // generated by Specta; checked via bindings:check + tsc
    ],
  },

  // Base recommended configs
  js.configs.recommended,

  // Main TypeScript/React configuration
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        project: './tsconfig.json',
        ecmaFeatures: {
          jsx: true,
        },
      },
      globals: {
        ...globals.browser,
        ...globals.es2021,
        ...globals.node,
        // Web Crypto API
        crypto: 'readonly',
        // React globals
        React: 'readonly',
        JSX: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'react': reactPlugin,
      'react-hooks': reactHooksPlugin,
      'import': importPlugin,
    },
    settings: {
      react: {
        version: 'detect',
      },
      'import/parsers': {
        '@typescript-eslint/parser': ['.ts', '.tsx'],
      },
      'import/resolver': {
        typescript: {
          alwaysTryTypes: true,
          project: './tsconfig.json',
        },
      },
    },
    rules: {
      // ===================================
      // Core ESLint Rules
      // ===================================
      'no-console': 'off', // Allow console in development, will be stripped in production
      'no-debugger': 'warn',
      'no-unused-vars': 'off', // Disabled in favor of TypeScript rule
      'no-undef': 'off', // TypeScript handles this
      'no-redeclare': 'off', // TypeScript handles this for enums/types
      'prefer-const': 'error',
      'no-var': 'error',
      'object-shorthand': ['error', 'always'],
      'quote-props': ['error', 'as-needed'],
      'prefer-template': 'error',
      'prefer-arrow-callback': 'error',
      'arrow-body-style': ['error', 'as-needed'],
      'eqeqeq': ['error', 'always', { null: 'ignore' }],
      'no-nested-ternary': 'off', // Temporarily disabled - 37 warnings

      // ===================================
      // TypeScript Rules
      // ===================================
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/explicit-function-return-type': 'off', // Too many warnings, TypeScript infers types well
      '@typescript-eslint/no-floating-promises': 'off', // Temporarily disabled - too many to fix now (58 warnings)
      '@typescript-eslint/await-thenable': 'warn',
      '@typescript-eslint/no-misused-promises': 'off', // Temporarily disabled - too many to fix now (65 warnings),
      '@typescript-eslint/prefer-nullish-coalescing': 'off', // Too many false positives
      '@typescript-eslint/prefer-optional-chain': 'warn',
      '@typescript-eslint/consistent-type-imports': 'off', // Conflicts with some TypeScript patterns
      '@typescript-eslint/naming-convention': [
        'warn', // Changed to warn to allow generated code
        {
          selector: 'typeLike',
          format: ['PascalCase'],
        },
        {
          selector: 'interface',
          format: ['PascalCase'],
          custom: {
            regex: '^I[A-Z]',
            match: false,
          },
        },
      ],

      // ===================================
      // React Rules
      // ===================================
      'react/react-in-jsx-scope': 'off', // Not needed in React 17+
      'react/prop-types': 'off', // Using TypeScript
      'react/jsx-uses-react': 'off',
      'react/jsx-uses-vars': 'error',
      'react/jsx-key': ['error', { checkFragmentShorthand: true }],
      'react/jsx-no-target-blank': 'error',
      'react/jsx-curly-brace-presence': ['error', { props: 'never', children: 'never' }],
      'react/self-closing-comp': 'error',
      'react/jsx-boolean-value': ['error', 'never'],

      // ===================================
      // React Hooks Rules
      // ===================================
      'react-hooks/rules-of-hooks': 'warn', // Some utility functions use hooks pattern
      'react-hooks/exhaustive-deps': 'warn',

      // ===================================
      // Import Rules
      // ===================================
      // Case-sensitive, so an import that only resolves through macOS case
      // folding fails here instead of on a case-sensitive filesystem.
      'import/no-unresolved': 'error',
      'import/no-cycle': 'error',
      // import/no-cycle sees file-level cycles only. These zones fix the
      // direction between folders that used to import each other, so a
      // folder-level cycle cannot come back one file at a time.
      'import/no-restricted-paths': ['error', {
        zones: [
          {
            target: './src/features',
            from: './src/components/RootLayout.tsx',
            message: 'Features cannot import the app shell. Put shared constants in src/shared.',
          },
          {
            target: './src/features/spaces',
            from: ['./src/features/chat', './src/features/journal', './src/features/explorer'],
            message: 'Spaces is the leaf that Chat, Journal and Explorer build on.',
          },
          {
            target: './src/features/journal',
            from: './src/features/chat',
            message: 'Chat imports Journal, so Journal cannot import Chat. Move what both need to a shared module.',
          },
          {
            target: './src/features/chat',
            from: './src/features/explorer',
            message: 'Explorer embeds Chat, so Chat cannot import Explorer. Pass Explorer UI into ChatPanel as props, or reach it through the folder-thread host.',
          },
          {
            target: './src/features/reading',
            from: './src/features',
            except: ['./reading'],
            message: 'Reading is the source reader every feature opens, so it imports no other feature. Take what it needs as props.',
          },
          {
            target: './src/shared',
            from: './src/features',
            message: 'Shared modules sit below every feature. Move the shared part here instead.',
          },
          {
            // src/components holds shared UI plus the app shell (Layout,
            // RootLayout), which composes features and so is not listed.
            target: [
              './src/components/ConfirmDialog', './src/components/EmptyState', './src/components/ErrorBoundary',
              './src/components/ErrorToast', './src/components/LoadingState', './src/components/Skeleton',
              './src/components/TiptapEditor', './src/components/Toast', './src/components/ui',
              './src/stores', './src/utils',
            ],
            from: './src/features',
            message: 'Shared UI, stores and utilities sit below every feature. A view that needs a feature belongs in that feature.',
          },
        ],
      }],
      'import/no-self-import': 'error',
      'import/no-duplicates': 'error',
      'import/first': 'error',
      'import/newline-after-import': 'error',

      'import/order': [
        'error',
        {
          groups: [
            'builtin',
            'external',
            'internal',
            ['parent', 'sibling', 'index'],
            'type',
          ],
          'newlines-between': 'always',
          alphabetize: {
            order: 'asc',
            caseInsensitive: true,
          },
          pathGroups: [
            {
              pattern: 'react',
              group: 'external',
              position: 'before',
            },
            {
              pattern: '@/**',
              group: 'internal',
              position: 'before',
            },
          ],
          pathGroupsExcludedImportTypes: ['react'],
        },
      ],

      // ===================================
      // Design System Enforcement Rules
      // ===================================
      // Disabled. The design tokens are the CSS variables in src/index.css
      // (exposed to Tailwind as --color-*); re-enable once views use only those.
      /*
      'no-restricted-syntax': [
        'error',
        // Background colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/bg-(white|black|gray|slate|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\\d+/]',
          message:
            'Hardcoded background color detected. Use CSS variables instead: bg-[var(--*)].\n' +
            'Example: bg-blue-600 → bg-[var(--accent-primary)]\n' +
            'See the tokens in src/index.css.',
        },
        // Text colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/text-(white|black|gray|slate|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\\d+/]',
          message:
            'Hardcoded text color detected. Use CSS variables instead: text-[var(--*)].\n' +
            'Example: text-gray-900 → text-[var(--text-primary)]\n' +
            'See the tokens in src/index.css.',
        },
        // Border colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/border-(white|black|gray|slate|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\\d+/]',
          message:
            'Hardcoded border color detected. Use CSS variables instead: border-[var(--*)].\n' +
            'Example: border-gray-200 → border-[var(--border-color)]\n' +
            'See the tokens in src/index.css.',
        },
        // Ring colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/ring-(blue|red|green|yellow|purple|pink|indigo|gray)-\\d+/]',
          message:
            'Hardcoded ring color detected. Use CSS variables instead: ring-[var(--*)].\n' +
            'Example: ring-blue-500 → ring-[var(--accent-primary)]\n' +
            'See the tokens in src/index.css.',
        },
        // Gradient from colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/from-(blue|red|green|yellow|purple|pink|indigo|gray)-\\d+/]',
          message:
            'Hardcoded gradient color detected. Use CSS variable gradients or .gradient-* classes.\n' +
            'Example: Use .gradient-brand class or define gradient in src/index.css\n' +
            'See the tokens in src/index.css.',
        },
        // Gradient to colors
        {
          selector: 'JSXAttribute[name.name="className"] Literal[value=/to-(blue|red|green|yellow|purple|pink|indigo|gray)-\\d+/]',
          message:
            'Hardcoded gradient color detected. Use CSS variable gradients or .gradient-* classes.\n' +
            'Example: Use .gradient-brand class or define gradient in src/index.css\n' +
            'See the tokens in src/index.css.',
        },
        // Template literal background colors
        {
          selector: 'JSXAttribute[name.name="className"] TemplateLiteral > TemplateElement[value.raw=/bg-(white|black|gray|slate|red|blue|green|yellow|purple|pink)-\\d+/]',
          message:
            'Hardcoded background color in template literal detected. Use CSS variables instead.\n' +
            'Example: `bg-${active ? "blue-600" : "gray-200"}` → `bg-[var(${active ? "--accent-primary" : "--bg-secondary"})]`\n' +
            'See the tokens in src/index.css.',
        },
        // Template literal text colors
        {
          selector: 'JSXAttribute[name.name="className"] TemplateLiteral > TemplateElement[value.raw=/text-(white|black|gray|slate|red|blue|green|yellow|purple|pink)-\\d+/]',
          message:
            'Hardcoded text color in template literal detected. Use CSS variables instead.\n' +
            'See the tokens in src/index.css.',
        },
        // Template literal border colors
        {
          selector: 'JSXAttribute[name.name="className"] TemplateLiteral > TemplateElement[value.raw=/border-(white|black|gray|slate|red|blue|green|yellow|purple|pink)-\\d+/]',
          message:
            'Hardcoded border color in template literal detected. Use CSS variables instead.\n' +
            'See the tokens in src/index.css.',
        },
      ],
      */
    },
  },

  // invoke() stays behind the transport. Tests are exempt: they import the
  // mocked invoke() to drive it. Later blocks that set no-restricted-imports
  // must keep RAW_INVOKE_IMPORTS (or ban @tauri-apps/** outright).
  {
    files: ['src/**/*.{ts,tsx}'],
    ignores: ['src/**/*.test.{ts,tsx}', 'src/**/__tests__/**', 'src/tests/**'],
    rules: {
      'no-restricted-imports': ['error', { paths: RAW_INVOKE_IMPORTS }],
    },
  },

  // Views reach the backend through hooks, which own caching and
  // invalidation, never through VaultAPI, a feature client or the transport.
  // Type-only imports are fine. VIEW_BACKEND_ALLOWLIST holds the files that
  // predate the rule.
  {
    files: ['src/components/**/*.{ts,tsx}', 'src/features/*/components/**/*.{ts,tsx}'],
    ignores: ['**/__tests__/**', '**/*.test.{ts,tsx}', '**/*.stories.{ts,tsx}', ...VIEW_BACKEND_ALLOWLIST],
    rules: {
      '@typescript-eslint/no-restricted-imports': ['error', { patterns: [{
        group: VIEW_BACKEND_IMPORTS,
        allowTypeImports: true,
        message: 'Views call the backend through a query or mutation hook, not VaultAPI, a feature client or the transport.',
      }] }],
    },
  },

  // Native plugins and asset URLs are backend access too; views get them
  // through a hook or a shared helper. VIEW_NATIVE_ALLOWLIST holds the files
  // that predate the rule.
  {
    files: ['src/components/**/*.{ts,tsx}', 'src/features/*/components/**/*.{ts,tsx}'],
    ignores: ['**/__tests__/**', '**/*.test.{ts,tsx}', '**/*.stories.{ts,tsx}', ...VIEW_NATIVE_ALLOWLIST],
    rules: {
      'no-restricted-syntax': ['error', ...VIEW_NATIVE_SELECTORS],
    },
  },

  // The shared IPC transport owns invoke().
  {
    files: ['src/shared/ipc/transport.ts'],
    rules: {
      'no-restricted-imports': 'off', // Transport is the only wrapper around invoke().
    },
  },

  // Zustand stores are client/UI state only. Backend-owned data belongs in
  // React Query hooks so there is one canonical cache and invalidation path.
  {
    files: ['src/stores/**/*.{ts,tsx}', 'src/features/*/stores/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@/lib/api', '**/lib/api', '@/lib/bindings', '**/lib/bindings', '@/features/**/api/**', '**/features/**/api/**', '@/shared/ipc/**', '**/shared/ipc/**', '@tauri-apps/**'],
              message: 'Backend access is prohibited in Zustand stores. Use a React Query hook or service.',
            },
            { group: ['@/components/**', '**/components/**', '@/features/**/components/**'], message: 'Stores cannot depend on view modules.' },
          ],
        },
      ],
    },
  },

  // Sidebar surfaces compose query-backed workflows; transport access belongs
  // in workspaceQueries.ts so sibling views share cache invalidation.
  {
    files: ['src/features/chat/components/ConversationSidebar.tsx', 'src/features/chat/components/sidebar/**/*.tsx'],
    ignores: ['**/__tests__/**', '**/*.test.tsx'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [{
        group: ['@/lib/api', '**/lib/api', '@/lib/bindings', '**/lib/bindings', '@/features/**/api/**', '**/features/**/api/**', '@/shared/ipc/**', '**/shared/ipc/**', '@tauri-apps/**'],
        message: 'Use the sidebar query/mutation hooks instead of accessing the backend from a view.',
      }] }],
    },
  },

  // Feature API boundaries must make every asynchronous outcome explicit.
  {
    files: ['src/features/*/api/*.{ts,tsx}', 'src/shared/ipc/*.ts'],
    ignores: ['**/*.test.{ts,tsx}'],
    rules: {
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
    },
  },
  {
    files: ['src/utils/**/*.{ts,tsx}', 'src/features/*/model/**/*.{ts,tsx}'],
    ignores: ['**/__tests__/**', '**/*.test.{ts,tsx}'],
    rules: {
      'no-restricted-imports': ['error', { paths: RAW_INVOKE_IMPORTS, patterns: [{
        group: ['@/components/**', '**/components/**', '@/features/**/components/**'],
        message: 'State and model helpers cannot depend on views. Move the shared operation to its feature model or shared module.',

      }] }],
    },
  },

  // Test files configuration
  {
    files: [
      'src/**/*.test.{ts,tsx}',
      'src/**/*.spec.{ts,tsx}',
      'src/tests/**/*.{ts,tsx}',
    ],
    languageOptions: {
      globals: {
        ...globals.node,
        describe: 'readonly',
        it: 'readonly',
        test: 'readonly',
        expect: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly',
        jest: 'readonly',
        vi: 'readonly',
      },
    },
    rules: {
      // Relax rules for test files
      'no-restricted-syntax': 'off',
      // Test doubles frequently need to stand in for broad framework types.
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/explicit-function-return-type': 'off',
      'no-console': 'off',
    },
  },

  // Playwright renderer tests run in Node while their init scripts execute in
  // the browser. Keep them in a dedicated type-aware project so test harnesses
  // cannot silently drift outside the main frontend lint/typecheck boundary.
  {
    files: ['e2e/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        project: './tsconfig.e2e.json',
      },
      globals: {
        ...globals.browser,
        ...globals.es2021,
        ...globals.node,
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'import': importPlugin,
    },
    rules: {
      'no-undef': 'off',
      'no-unused-vars': 'off',
      'no-redeclare': 'off',
      'no-var': 'error',
      'prefer-const': 'error',
      'eqeqeq': ['error', 'always', { null: 'ignore' }],
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      'import/no-duplicates': 'error',
    },
  },

  {
    files: ['e2e/**/*.mjs'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.es2021,
        ...globals.node,
      },
    },
  },

  // Storybook files configuration
  {
    files: ['src/**/*.stories.{ts,tsx}'],
    rules: {
      // Allow hardcoded colors in Storybook
      'no-restricted-syntax': 'off',
      '@typescript-eslint/explicit-function-return-type': 'off',
    },
  },

  // Configuration files
  {
    files: ['*.config.{js,ts}', 'scripts/**/*.{js,ts}'],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      '@typescript-eslint/no-var-requires': 'off',
      'no-console': 'off',
    },
  },
];

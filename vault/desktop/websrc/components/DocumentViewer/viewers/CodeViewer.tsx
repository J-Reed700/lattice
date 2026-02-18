import { useState } from 'react';

import { Copy, Check } from 'lucide-react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus, vs } from 'react-syntax-highlighter/dist/esm/styles/prism';

import { sanitizeFileName } from '@/utils/sanitize';

/**
 * CodeViewer
 *
 * Purpose: Display code files with syntax highlighting
 *
 * Features:
 * - Syntax highlighting for 50+ languages
 * - Line numbers
 * - Theme matching (light/dark)
 * - Copy to clipboard functionality
 * - Monospace font
 *
 * States: default, copied
 * Accessibility: WCAG AA, keyboard navigation
 */

export interface CodeViewerProps {
  content: string;
  language: string;
  fileName: string;
}

export function CodeViewer({ content, language, fileName }: CodeViewerProps) {
  const [copied, setCopied] = useState(false);

  // Check if dark mode is enabled
  const isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches ||
    document.documentElement.classList.contains('dark');

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy code:', err);
    }
  };

  // Map file extensions to Prism language identifiers
  const getLanguageId = (lang: string): string => {
    const languageMap: Record<string, string> = {
      js: 'javascript',
      jsx: 'jsx',
      ts: 'typescript',
      tsx: 'tsx',
      py: 'python',
      rs: 'rust',
      go: 'go',
      java: 'java',
      cpp: 'cpp',
      c: 'c',
      h: 'c',
      hpp: 'cpp',
      css: 'css',
      scss: 'scss',
      sass: 'sass',
      less: 'less',
      html: 'html',
      xml: 'xml',
      json: 'json',
      yaml: 'yaml',
      yml: 'yaml',
      toml: 'toml',
      md: 'markdown',
      sh: 'bash',
      bash: 'bash',
      zsh: 'bash',
      sql: 'sql',
      graphql: 'graphql',
      dockerfile: 'docker',
      makefile: 'makefile',
    };

    return languageMap[lang.toLowerCase()] || 'text';
  };

  if (!content) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[var(--bg-tertiary)] rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[var(--text-tertiary)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
            Empty File
          </h3>
          <p className="text-[var(--text-secondary)]">This file has no content to display.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[var(--bg-secondary)]">
      {/* File header with copy button */}
      <div className="bg-[var(--surface-elevated)] text-white px-6 py-3 flex items-center justify-between border-b border-[var(--border-color)]">
        <div className="flex items-center gap-3">
          <svg
            className="w-5 h-5 text-[var(--text-tertiary)]"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
            />
          </svg>
          <span className="text-sm font-mono font-medium">{sanitizeFileName(fileName)}</span>
          <span className="text-xs text-[var(--text-tertiary)] uppercase">{language}</span>
        </div>

        <button
          onClick={handleCopy}
          className="flex items-center gap-2 px-3 py-1.5 text-sm bg-[var(--surface-elevated)] hover:bg-[var(--text-secondary)] rounded-lg transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
          aria-label={copied ? 'Copied!' : 'Copy code'}
        >
          {copied ? (
            <>
              <Check className="w-4 h-4 text-[var(--success)]" />
              <span className="text-[var(--success)]">Copied!</span>
            </>
          ) : (
            <>
              <Copy className="w-4 h-4" />
              <span>Copy</span>
            </>
          )}
        </button>
      </div>

      {/* Code content */}
      <div className="flex-1 overflow-auto">
        <SyntaxHighlighter
          language={getLanguageId(language)}
          style={isDarkMode ? vscDarkPlus : vs}
          showLineNumbers
          customStyle={{
            margin: 0,
            borderRadius: 0,
            fontSize: '14px',
            lineHeight: '1.5',
            padding: '1.5rem',
          }}
          codeTagProps={{
            style: {
              fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
            },
          }}
        >
          {content}
        </SyntaxHighlighter>
      </div>
    </div>
  );
}

export default CodeViewer;

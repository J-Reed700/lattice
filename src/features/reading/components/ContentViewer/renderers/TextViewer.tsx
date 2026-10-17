import { useQuery } from '@tanstack/react-query';
import { Loader2, AlertCircle } from 'lucide-react';
import { oneDark, oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism';

import { sourceTextQueryOptions } from '@/features/reading/hooks/readSourceFile';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';

import { PrismSyntaxHighlighter as SyntaxHighlighter } from './prism';

interface TextViewerProps {
  filePath?: string;
  content?: string;
  title?: string;
  language?: string;
}

// Map file extensions to syntax highlighter languages
const getLanguage = (filePath: string): string => {
  const ext = filePath.split('.').pop()?.toLowerCase();

  const languageMap: Record<string, string> = {
    rs: 'rust',
    ts: 'typescript',
    tsx: 'tsx',
    js: 'javascript',
    jsx: 'jsx',
    py: 'python',
    json: 'json',
    toml: 'toml',
    yaml: 'yaml',
    yml: 'yaml',
    md: 'markdown',
    markdown: 'markdown',
    sh: 'bash',
    bash: 'bash',
    xml: 'xml',
    html: 'html',
    css: 'css',
    txt: 'text',
  };

  return languageMap[ext || ''] || 'text';
};

export function TextViewer({ filePath, content: directContent, title: _title, language: directLanguage }: TextViewerProps) {
  const theme = useEffectiveTheme();
  const fileQuery = useQuery({ ...sourceTextQueryOptions(filePath ?? ''), enabled: Boolean(filePath) });
  const content = filePath ? fileQuery.data ?? '' : directContent ?? '';
  const language = filePath ? getLanguage(filePath) : directLanguage ?? 'text';
  const isLoading = Boolean(filePath) && fileQuery.isPending;
  const error = filePath && fileQuery.isError
    ? fileQuery.error instanceof Error ? fileQuery.error.message : 'Unknown error'
    : null;

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full bg-[hsl(var(--surface))]">
        <Loader2 className="w-8 h-8 animate-spin text-[hsl(var(--accent))]" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8 bg-[hsl(var(--surface))]">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-[hsl(var(--danger-fg))] mx-auto" />
          <p className="text-[hsl(var(--danger-fg))] font-medium">Failed to load file</p>
          <p className="text-sm text-[hsl(var(--text-secondary))]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[hsl(var(--surface))]">
      {/* Language info bar */}
      <div className="flex items-center px-6 py-3 bg-[hsl(var(--bg))] border-b">
        <span className="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]">
          {language.toUpperCase()}
        </span>
        <span className="ml-3 text-sm text-[hsl(var(--text-tertiary))]">
          {content.split('\n').length} lines
        </span>
      </div>

      {/* Code content */}
      <div className="flex-1 overflow-auto">
        <SyntaxHighlighter
          language={language}
          style={theme === 'dark' ? oneDark : oneLight}
          showLineNumbers
          wrapLines
          // Gives PassageHighlighter a block to attach to.
          lineProps={{ className: 'lattice-line' }}
          customStyle={{
            margin: 0,
            padding: '1.5rem',
            fontSize: '14px',
            lineHeight: '1.6',
            minHeight: '100%',
            background: 'hsl(var(--surface))',
          }}
          lineNumberStyle={{
            minWidth: '3em',
            paddingRight: '1em',
            color: 'hsl(var(--text-muted))',
            userSelect: 'none',
          }}
        >
          {content}
        </SyntaxHighlighter>
      </div>
    </div>
  );
}

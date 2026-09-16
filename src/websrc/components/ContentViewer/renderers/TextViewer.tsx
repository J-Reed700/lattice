import { useEffect, useState } from 'react';

import { Loader2, AlertCircle } from 'lucide-react';
import { oneDark, oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism';

import { PrismSyntaxHighlighter as SyntaxHighlighter } from './prism';
import { useEffectiveTheme } from '../../../hooks/useApplyTheme';
import VaultAPI from '../../../lib/api';

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
  const [content, setContent] = useState<string>(directContent || '');
  const [isLoading, setIsLoading] = useState(Boolean(filePath));
  const [error, setError] = useState<string | null>(null);
  const [language, setLanguage] = useState<string>(directLanguage || 'text');

  useEffect(() => {
    let cancelled = false;
    if (!filePath) {
      setContent(directContent ?? '');
      setLanguage(directLanguage ?? 'text');
      setIsLoading(false);
      setError(null);
      return;
    }

    async function loadFile() {
      setIsLoading(true);
      setError(null);

      try {
        const result = await VaultAPI.readFileContent(filePath!);
        if (cancelled) return;
        if (result.ok) {
          setContent(result.data);
          setLanguage(getLanguage(filePath!));
        } else {
          setError(result.error || 'Failed to load file');
        }
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        if (!cancelled) setIsLoading(false);
      }
    }

    void loadFile();
    return () => { cancelled = true; };
  }, [filePath, directContent, directLanguage]);

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

import { useEffect, useState } from 'react';

import { Loader2, AlertCircle } from 'lucide-react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism';

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
  const [content, setContent] = useState<string>(directContent || '');
  const [isLoading, setIsLoading] = useState(!directContent);
  const [error, setError] = useState<string | null>(null);
  const [language, setLanguage] = useState<string>(directLanguage || 'text');

  useEffect(() => {
    if (!filePath) {
      // Direct content mode - already set from props
      return;
    }

    async function loadFile() {
      setIsLoading(true);
      setError(null);

      try {
        const result = await VaultAPI.readFileContent(filePath!);
        if (result.ok) {
          setContent(result.data);
          setLanguage(getLanguage(filePath!));
        } else {
          setError(result.error || 'Failed to load file');
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        setIsLoading(false);
      }
    }

    loadFile();
  }, [filePath]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full bg-[var(--bg-secondary)]">
        <Loader2 className="w-8 h-8 animate-spin text-[var(--accent-primary)]" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8 bg-[var(--bg-secondary)]">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-[var(--error)] mx-auto" />
          <p className="text-[var(--error)] font-medium">Failed to load file</p>
          <p className="text-sm text-[var(--text-secondary)]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[var(--bg-secondary)]">
      {/* Language info bar */}
      <div className="flex items-center px-6 py-3 bg-[var(--bg-primary)] border-b">
        <span className="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-[var(--accent-light)] text-[var(--accent-primary)]">
          {language.toUpperCase()}
        </span>
        <span className="ml-3 text-sm text-[var(--text-tertiary)]">
          {content.split('\n').length} lines
        </span>
      </div>

      {/* Code content */}
      <div className="flex-1 overflow-auto">
        <SyntaxHighlighter
          language={language}
          style={oneDark}
          showLineNumbers
          wrapLines
          customStyle={{
            margin: 0,
            padding: '1.5rem',
            fontSize: '14px',
            lineHeight: '1.6',
            minHeight: '100%',
            background: '#282c34',
          }}
          lineNumberStyle={{
            minWidth: '3em',
            paddingRight: '1em',
            color: '#5c6370',
            userSelect: 'none',
          }}
        >
          {content}
        </SyntaxHighlighter>
      </div>
    </div>
  );
}

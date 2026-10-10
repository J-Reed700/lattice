import { oneDark, oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism';

import { useEffectiveTheme } from '@/hooks/useApplyTheme';

import { PrismSyntaxHighlighter as SyntaxHighlighter } from './prism';

interface CodeViewerProps {
  content: string;
  language?: string;
}

export function CodeViewer({ content, language = 'text' }: CodeViewerProps) {
  const theme = useEffectiveTheme();
  return (
    <div className="h-full overflow-auto">
      <SyntaxHighlighter
        language={language}
        style={theme === 'dark' ? oneDark : oneLight}
        showLineNumbers
        customStyle={{
          margin: 0,
          borderRadius: 0,
          minHeight: '100%',
        }}
      >
        {content}
      </SyntaxHighlighter>
    </div>
  );
}

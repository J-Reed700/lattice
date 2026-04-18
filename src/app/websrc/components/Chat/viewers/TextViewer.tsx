import { type FC } from 'react';

import { Light as SyntaxHighlighter } from 'react-syntax-highlighter';
// Import only needed languages for performance
import go from 'react-syntax-highlighter/dist/esm/languages/hljs/go';
import java from 'react-syntax-highlighter/dist/esm/languages/hljs/java';
import javascript from 'react-syntax-highlighter/dist/esm/languages/hljs/javascript';
import json from 'react-syntax-highlighter/dist/esm/languages/hljs/json';
import markdown from 'react-syntax-highlighter/dist/esm/languages/hljs/markdown';
import python from 'react-syntax-highlighter/dist/esm/languages/hljs/python';
import rust from 'react-syntax-highlighter/dist/esm/languages/hljs/rust';
import typescript from 'react-syntax-highlighter/dist/esm/languages/hljs/typescript';
import { atomOneDark, atomOneLight } from 'react-syntax-highlighter/dist/esm/styles/hljs';

import { useEffectiveTheme } from '@/hooks/useApplyTheme';

// Register languages
SyntaxHighlighter.registerLanguage('javascript', javascript);
SyntaxHighlighter.registerLanguage('typescript', typescript);
SyntaxHighlighter.registerLanguage('python', python);
SyntaxHighlighter.registerLanguage('rust', rust);
SyntaxHighlighter.registerLanguage('go', go);
SyntaxHighlighter.registerLanguage('java', java);
SyntaxHighlighter.registerLanguage('json', json);
SyntaxHighlighter.registerLanguage('markdown', markdown);

interface TextViewerProps {
  content: string;
  language?: string;
}

export const TextViewer: FC<TextViewerProps> = ({ content, language = 'text' }) => {
  const theme = useEffectiveTheme();
  const style = theme === 'dark' ? atomOneDark : atomOneLight;

  return (
    <div className="rounded-lg overflow-hidden border border-[hsl(var(--border-subtle))] dark:border-[hsl(var(--border-default))]">
      <SyntaxHighlighter
        language={language}
        style={style}
        showLineNumbers
        wrapLines
        customStyle={{
          margin: 0,
          borderRadius: 0,
          fontSize: '0.875rem',
        }}
      >
        {content}
      </SyntaxHighlighter>
    </div>
  );
};

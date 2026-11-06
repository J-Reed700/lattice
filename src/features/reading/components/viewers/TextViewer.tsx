import { type FC } from 'react';

import { Light as SyntaxHighlighter } from 'react-syntax-highlighter';
// Import only needed languages for performance
import bash from 'react-syntax-highlighter/dist/esm/languages/hljs/bash';
import c from 'react-syntax-highlighter/dist/esm/languages/hljs/c';
import cpp from 'react-syntax-highlighter/dist/esm/languages/hljs/cpp';
import csharp from 'react-syntax-highlighter/dist/esm/languages/hljs/csharp';
import css from 'react-syntax-highlighter/dist/esm/languages/hljs/css';
import go from 'react-syntax-highlighter/dist/esm/languages/hljs/go';
import ini from 'react-syntax-highlighter/dist/esm/languages/hljs/ini';
import java from 'react-syntax-highlighter/dist/esm/languages/hljs/java';
import javascript from 'react-syntax-highlighter/dist/esm/languages/hljs/javascript';
import json from 'react-syntax-highlighter/dist/esm/languages/hljs/json';
import markdown from 'react-syntax-highlighter/dist/esm/languages/hljs/markdown';
import python from 'react-syntax-highlighter/dist/esm/languages/hljs/python';
import rust from 'react-syntax-highlighter/dist/esm/languages/hljs/rust';
import scss from 'react-syntax-highlighter/dist/esm/languages/hljs/scss';
import sql from 'react-syntax-highlighter/dist/esm/languages/hljs/sql';
import typescript from 'react-syntax-highlighter/dist/esm/languages/hljs/typescript';
import xml from 'react-syntax-highlighter/dist/esm/languages/hljs/xml';
import yaml from 'react-syntax-highlighter/dist/esm/languages/hljs/yaml';
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
SyntaxHighlighter.registerLanguage('cpp', cpp);
SyntaxHighlighter.registerLanguage('c', c);
SyntaxHighlighter.registerLanguage('csharp', csharp);
SyntaxHighlighter.registerLanguage('yaml', yaml);
SyntaxHighlighter.registerLanguage('xml', xml);
SyntaxHighlighter.registerLanguage('css', css);
SyntaxHighlighter.registerLanguage('scss', scss);
SyntaxHighlighter.registerLanguage('bash', bash);
SyntaxHighlighter.registerLanguage('sql', sql);
SyntaxHighlighter.registerLanguage('ini', ini);

interface TextViewerProps {
  content: string;
  language?: string;
}

export const TextViewer: FC<TextViewerProps> = ({ content, language = 'text' }) => {
  const theme = useEffectiveTheme();
  const style = theme === 'dark' ? atomOneDark : atomOneLight;

  return (
    <div className="rounded-lg overflow-hidden border border-[hsl(var(--border-subtle))]">
      <SyntaxHighlighter
        language={language}
        style={style}
        showLineNumbers
        wrapLines
        // Gives PassageHighlighter a block to attach to: without a stable class
        // a highlighted code passage would have nothing to mark.
        lineProps={{ className: 'lattice-line' }}
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

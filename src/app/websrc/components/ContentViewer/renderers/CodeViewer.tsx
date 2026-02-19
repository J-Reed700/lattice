import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';

interface CodeViewerProps {
  content: string;
  language?: string;
}

export function CodeViewer({ content, language = 'text' }: CodeViewerProps) {
  return (
    <div className="h-full overflow-auto">
      <SyntaxHighlighter
        language={language}
        style={vscDarkPlus}
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

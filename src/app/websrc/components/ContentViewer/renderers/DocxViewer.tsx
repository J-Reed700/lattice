import { useEffect, useState } from 'react';

import { AlertCircle, Loader2 } from 'lucide-react';
import DOMPurify from 'dompurify';
import * as mammoth from 'mammoth';

import VaultAPI from '../../../lib/api';

interface DocxViewerProps {
  filePath: string;
  title?: string;
}

export function DocxViewer({ filePath, title }: DocxViewerProps) {
  const [htmlContent, setHtmlContent] = useState<string>('');
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    async function loadDocx() {
      setIsLoading(true);
      setError(null);

      try {
        const fileResult = await VaultAPI.readFileBytes(filePath);
        if (!fileResult.ok) {
          throw new Error(fileResult.error || 'Failed to read DOCX bytes');
        }

        const bytes = fileResult.data;
        const arrayBuffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
        const result = await mammoth.convertToHtml({ arrayBuffer });
        const sanitized = DOMPurify.sanitize(result.value, { USE_PROFILES: { html: true } });

        const wrapped = `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 860px; margin: 32px auto; padding: 0 20px; line-height: 1.6; color: #111827; }
img { max-width: 100%; height: auto; }
table { border-collapse: collapse; width: 100%; }
table, th, td { border: 1px solid #e5e7eb; }
th, td { padding: 8px; text-align: left; }
blockquote { border-left: 4px solid #e5e7eb; margin: 0; padding-left: 16px; color: #4b5563; }
code, pre { background: #f3f4f6; }
pre { padding: 12px; overflow-x: auto; }
</style>
</head>
<body>
${sanitized}
</body>
</html>`;

        if (isMounted) {
          setHtmlContent(wrapped);
        }
      } catch (err) {
        if (isMounted) {
          setError(err instanceof Error ? err.message : 'Failed to load DOCX');
        }
      } finally {
        if (isMounted) {
          setIsLoading(false);
        }
      }
    }

    loadDocx();

    return () => {
      isMounted = false;
    };
  }, [filePath]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-destructive mx-auto" />
          <p className="text-destructive font-medium">Failed to load DOCX</p>
          <p className="text-sm text-muted-foreground">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-background">
      {title && (
        <div className="px-6 py-4 border-b">
          <h1 className="text-xl font-semibold text-foreground">{title}</h1>
        </div>
      )}
      <iframe
        srcDoc={htmlContent}
        className="flex-1 w-full border-0"
        sandbox="allow-same-origin"
        title={title || 'DOCX Document'}
      />
    </div>
  );
}

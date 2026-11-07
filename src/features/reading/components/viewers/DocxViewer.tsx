import { useQuery } from '@tanstack/react-query';
import DOMPurify from 'dompurify';
import { AlertCircle, Loader2 } from 'lucide-react';

import { useApi } from '@/contexts/ApiContext';
import { readingKeys, readSourceBytes } from '@/features/reading/hooks/readSourceFile';
import { useEffectiveTheme } from '@/hooks/useApplyTheme';
import type { ApiClient } from '@/lib/api';

interface DocxViewerProps {
  filePath: string;
  title?: string;
}

/** The document as sanitized HTML, ready for a sandboxed frame. */
async function loadDocxHtml(filePath: string, api: ApiClient): Promise<string> {
  const fileResult = await readSourceBytes(filePath, api);
  if (!fileResult.ok) {
    throw new Error(fileResult.error || 'Failed to read DOCX bytes');
  }

  const bytes = fileResult.data;
  const arrayBuffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  const mammoth = await import('mammoth');
  const result = await mammoth.convertToHtml({ arrayBuffer });
  const sanitized = DOMPurify.sanitize(result.value, { USE_PROFILES: { html: true } });

  const wrapped = `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 860px; margin: 32px auto; padding: 0 20px; line-height: 1.6; color: CanvasText; background: Canvas; }
img { max-width: 100%; height: auto; }
table { border-collapse: collapse; width: 100%; }
table, th, td { border: 1px solid color-mix(in srgb, CanvasText 20%, Canvas); }
th, td { padding: 8px; text-align: left; }
blockquote { border-left: 4px solid color-mix(in srgb, CanvasText 20%, Canvas); margin: 0; padding-left: 16px; color: color-mix(in srgb, CanvasText 75%, Canvas); }
code, pre { background: color-mix(in srgb, CanvasText 6%, Canvas); }
pre { padding: 12px; overflow-x: auto; }
</style>
</head>
<body>
${sanitized}
</body>
</html>`;
  return wrapped;
}

export function DocxViewer({ filePath, title }: DocxViewerProps) {
  const api = useApi();
  const theme = useEffectiveTheme();
  const docxQuery = useQuery({
    queryKey: readingKeys.docxHtml(filePath),
    queryFn: () => loadDocxHtml(filePath, api),
    retry: false,
    staleTime: 0,
  });
  const htmlContent = docxQuery.data ?? '';
  const isLoading = docxQuery.isPending;
  const error = docxQuery.isError
    ? docxQuery.error instanceof Error ? docxQuery.error.message : 'Failed to load DOCX'
    : null;

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
          <AlertCircle className="w-12 h-12 text-[hsl(var(--danger))] mx-auto" />
          <p className="text-[hsl(var(--danger))] font-medium">Failed to load DOCX</p>
          <p className="text-sm text-[hsl(var(--text-secondary))]">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[hsl(var(--bg))]">
      {title && (
        <div className="px-6 py-4 border-b">
          <h1 className="text-xl font-semibold text-[hsl(var(--text-primary))]">{title}</h1>
        </div>
      )}
      <iframe
        srcDoc={htmlContent.replace('<style>', `<style>:root { color-scheme: ${theme}; }`)}
        className="flex-1 w-full border-0"
        sandbox="allow-same-origin"
        title={title || 'DOCX Document'}
      />
    </div>
  );
}

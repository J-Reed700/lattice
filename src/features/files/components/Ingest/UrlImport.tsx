import { useState, useCallback, useMemo, type ChangeEvent } from 'react';

import { Globe, Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useWebImport } from '@/features/files/hooks/useWebImport';
import type { UrlPreview } from '@/types/api/web';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface UrlImportProps {
  onImport: (_url: string) => void;
  onImportComplete?: (_success: boolean, _url: string) => void;
}

const SUPPORTED_WEB_HINTS = ['Articles', 'Documentation', 'Research posts', 'Readable web pages'];

function formatInvokeError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err) {
    return String((err as { message?: string }).message);
  }
  try {
    return JSON.stringify(err);
  } catch {
    return "Couldn't fetch preview";
  }
}

function isValidUrl(text: string): boolean {
  try {
    const url = new URL(text);
    return ['http:', 'https:'].includes(url.protocol);
  } catch {
    return false;
  }
}

function normalizeUrl(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (isValidUrl(trimmed)) return trimmed;
  // If it looks like a domain without scheme, assume https.
  if (/^[a-z0-9.-]+\.[a-z]{2,}/i.test(trimmed)) {
    return `https://${trimmed}`;
  }
  return null;
}

function debounce<T extends (..._args: never[]) => unknown>(
  func: T,
  delay: number
): (..._args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => func(...args), delay);
  };
}

function previewMeta(preview: UrlPreview): string {
  const parts: string[] = [];
  if (preview.siteName) parts.push(preview.siteName);
  if (preview.author) parts.push(preview.author);
  if (preview.readingTimeMinutes > 0) parts.push(`${preview.readingTimeMinutes} min read`);
  else if (preview.wordCount) parts.push(`${preview.wordCount.toLocaleString()} words`);
  return parts.join(' · ');
}

export const UrlImport: React.FC<UrlImportProps> = ({ onImport, onImportComplete }) => {
  const { preview: fetchUrlPreview, importPage } = useWebImport();
  const [url, setUrl] = useState('');
  const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
  const [preview, setPreview] = useState<UrlPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchPreview = useCallback(async (url: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetchUrlPreview(url);
      if (!result.ok) {
        throw new Error(result.error);
      }
      setPreview(result.data);
    } catch (err) {
      console.error('Preview failed:', err);
      setError(formatInvokeError(err));
      setPreview(null);
    } finally {
      setLoading(false);
    }
  }, [fetchUrlPreview]);

  const debouncedFetchPreview = useMemo(
    () => debounce(fetchPreview, 500),
    [fetchPreview]
  );

  const handleUrlChange = (e: ChangeEvent<HTMLInputElement>) => {
    const newUrl = e.target.value;
    setUrl(newUrl);
    setError(null);

    const normalized = normalizeUrl(newUrl);
    setResolvedUrl(normalized);

    if (normalized) {
      debouncedFetchPreview(normalized);
    } else {
      setPreview(null);
    }
  };

  const handleImport = async () => {
    if (resolvedUrl) {
      setLoading(true);
      setError(null);
      try {
        const result = await importPage(resolvedUrl);
        if (!result.ok) {
          throw new Error(result.error);
        }

        // Notify parent components
        onImport(resolvedUrl);
        onImportComplete?.(true, resolvedUrl);

        // Clear the form
        setUrl('');
        setResolvedUrl(null);
        setPreview(null);
      } catch (error) {
        console.error('Import failed:', error);
        const errorMessage = formatInvokeError(error);
        setError(errorMessage);
        onImportComplete?.(false, resolvedUrl);
      } finally {
        setLoading(false);
      }
    }
  };

  const meta = preview ? previewMeta(preview) : '';

  return (
    <div className="space-y-5">
      <div className="flex items-start gap-3.5">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent-muted text-accent">
          <Globe className="h-4 w-4" strokeWidth={1.75} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h2 className="text-[15px] font-medium text-text-primary">Save a web page</h2>
          <p className="mt-0.5 text-sm leading-relaxed text-text-secondary">
            Paste a link to archive a clean, offline-readable copy and index it for search and chat.
          </p>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <Input
            type="url"
            placeholder="https://example.com/article"
            aria-label="URL to import"
            value={url}
            onChange={handleUrlChange}
          />
        </div>
        <Button
          onClick={handleAsyncEvent(handleImport)}
          disabled={!resolvedUrl || loading}
          className="shrink-0"
        >
          {loading ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
              Importing…
            </>
          ) : (
            'Import'
          )}
        </Button>
      </div>

      {error && <p className="text-sm text-danger-fg">{error}</p>}

      {preview ? (
        <div className="rounded-lg border border-border-subtle bg-bg/60 p-4">
          <div className="truncate text-sm font-medium text-text-primary">{preview.title}</div>
          {meta && <div className="mt-0.5 truncate text-xs text-text-muted">{meta}</div>}
          {preview.description && (
            <p className="mt-2 line-clamp-2 text-sm leading-relaxed text-text-secondary">
              {preview.description}
            </p>
          )}
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-border-subtle pt-4">
          <span className="mr-1 text-xs text-text-muted">Works best with:</span>
          {SUPPORTED_WEB_HINTS.map((hint) => (
            <span
              key={hint}
              className="inline-flex items-center rounded-md bg-[hsl(var(--text-primary)/0.05)] px-2 py-0.5 text-[11px] font-medium text-text-secondary"
            >
              {hint}
            </span>
          ))}
        </div>
      )}
    </div>
  );
};

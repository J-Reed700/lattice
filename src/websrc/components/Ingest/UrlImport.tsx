import { useState, useCallback, useMemo, type ChangeEvent } from 'react';

import { Button } from '@/components/ui/button';
import { settingsFieldClass } from '@/components/ui/SettingsSection';
import { VaultAPI } from '@/lib/api';
import { cn } from '@/lib/utils';
import type { UrlPreview } from '@/types/api/web';

interface UrlImportProps {
  onImport: (_url: string) => void;
  onImportComplete?: (_success: boolean, _url: string) => void;
}

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
  const [url, setUrl] = useState('');
  const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
  const [preview, setPreview] = useState<UrlPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchPreview = useCallback(async (url: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await VaultAPI.fetchUrlPreview(url);
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
  }, []);

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
        // Actually import the URL via backend
        const result = await VaultAPI.ingestWebUrl(resolvedUrl);
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
    <div>
      <div className="flex items-center gap-2">
        <input
          type="url"
          placeholder="https://example.com/article"
          aria-label="URL to import"
          value={url}
          onChange={handleUrlChange}
          className={cn(settingsFieldClass, 'h-9 flex-1')}
        />
        <Button
          onClick={handleImport}
          disabled={!resolvedUrl || loading}
          className="h-9 shrink-0"
        >
          {loading ? 'Importing…' : 'Import'}
        </Button>
      </div>

      {error && <p className="mt-3 text-sm text-danger-fg">{error}</p>}

      {preview && (
        <div className="mt-6 border-t border-border-subtle">
          <div className="border-b border-border-subtle py-3">
            <div className="truncate text-sm font-medium text-text-primary">{preview.title}</div>
            {meta && <div className="mt-0.5 truncate text-xs text-text-muted">{meta}</div>}
            {preview.description && (
              <p className="mt-1.5 line-clamp-2 text-sm text-text-secondary">
                {preview.description}
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

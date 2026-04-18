import { useState, useCallback, useMemo, type ChangeEvent } from 'react';

import { motion, AnimatePresence } from 'framer-motion';
import { Globe, Download, Loader2, FileText, Clock } from 'lucide-react';

import { Button } from '@/components/ui/button';
import Input from '@/components/ui/input/Input';
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
    return 'Failed to fetch preview';
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

  return (
    <div className="space-y-4">
      <div className="flex gap-3">
        <Input
          type="url"
          placeholder="https://example.com/article"
          value={url}
          onChange={handleUrlChange}
          className="flex-1"
          leftIcon={<Globe className="w-4 h-4" />}
          isLoading={loading}
          error={error || undefined}
        />

        <Button
          onClick={handleImport}
          disabled={!resolvedUrl || loading}
        >
          {loading ? (
            <Loader2 className="w-4 h-4 animate-spin" />
          ) : (
            <>
              <Download className="w-4 h-4 mr-2" />
              Import
            </>
          )}
        </Button>
      </div>

      <AnimatePresence>
        {preview && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className={cn(
              "border rounded-lg p-4",
              "border-[hsl(var(--border-subtle))]",
              "bg-[hsl(var(--surface-raised))]"
            )}
          >
            <div className="flex gap-4">
              {preview.image && (
                <img
                  src={preview.image}
                  alt={preview.title}
                  className="w-24 h-24 object-cover rounded-md flex-shrink-0"
                  onError={(e) => {
                    e.currentTarget.style.display = 'none';
                  }}
                />
              )}

              <div className="flex-1 min-w-0">
                <h4 className="font-semibold text-lg truncate text-[hsl(var(--text-primary))]">
                  {preview.title}
                </h4>
                {preview.author && (
                  <p className="text-sm text-[hsl(var(--text-secondary))]">
                    by {preview.author}
                  </p>
                )}
                {preview.description && (
                  <p className="text-sm text-[hsl(var(--text-secondary))] mt-2 line-clamp-2">
                    {preview.description}
                  </p>
                )}

                <div className="flex gap-4 mt-3 text-xs text-[hsl(var(--text-secondary))]">
                  {preview.siteName && (
                    <span className="flex items-center gap-1">
                      <Globe className="w-3 h-3" />
                      {preview.siteName}
                    </span>
                  )}
                  {preview.wordCount && (
                    <span className="flex items-center gap-1">
                      <FileText className="w-3 h-3" />
                      {preview.wordCount.toLocaleString()} words
                    </span>
                  )}
                  {preview.readingTimeMinutes > 0 && (
                    <span className="flex items-center gap-1">
                      <Clock className="w-3 h-3" />
                      {preview.readingTimeMinutes} min read
                    </span>
                  )}
                </div>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};

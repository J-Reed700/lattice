/**
 * WebUrlInput Component
 *
 * Purpose: Form input for ingesting web URLs into the Vault
 *
 * Features:
 * - URL input field with client-side validation
 * - Loading state during ingestion
 * - Success feedback with document details
 * - Error handling with user-friendly messages
 * - Keyboard shortcuts (Enter to submit)
 * - Clear button to reset input
 *
 * States: idle, loading, success, error
 * Accessibility: WCAG AA, keyboard navigation, ARIA labels
 */

import { useState, type FormEvent, type ChangeEvent } from 'react';

import { Globe} from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import { type WebIngestResponse } from '../../types';
import { showSuccessToast, showErrorToast } from '../../utils/toast';
import Button from '../ui/Button/Button';
import Input from '../ui/input/Input';

interface WebUrlInputProps {
  /** Optional callback when ingestion succeeds */
  onSuccess?: (response: WebIngestResponse) => void;
  /** Optional callback when form is cancelled/closed */
  onCancel?: () => void;
  /** Show as inline form or standalone */
  variant?: 'inline' | 'standalone';
  /** Custom className */
  className?: string;
}

/**
 * Validate URL format (must be http or https)
 */
function validateUrl(url: string): string | undefined {
  if (!url.trim()) {
    return 'URL is required';
  }

  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
      return 'URL must start with http:// or https://';
    }
    return undefined;
  } catch {
    return 'Invalid URL format';
  }
}

export function WebUrlInput({
  onSuccess,
  onCancel,
  variant = 'standalone',
  className = '',
}: WebUrlInputProps) {
  const [url, setUrl] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const handleUrlChange = (e: ChangeEvent<HTMLInputElement>) => {
    setUrl(e.target.value);
    if (error) {
      setError(undefined);
    }
  };

  const handleClear = () => {
    setUrl('');
    setError(undefined);
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();

    // Validate URL
    const validationError = validateUrl(url);
    if (validationError) {
      setError(validationError);
      return;
    }

    setIsLoading(true);
    setError(undefined);

    try {
      const result = await VaultAPI.ingestWebUrl(url.trim());
      if (!result.ok) {
        throw new Error(result.error);
      }
      const response = result.data as WebIngestResponse;

      // Show success toast
      // const readingTime = response.readingTimeMinutes
      //   ? ` • ${response.readingTimeMinutes} min read`
      //   : '';
      showSuccessToast(
        `Ingested: ${response.title}`,
        {
          duration: 5000,
        }
      );

      // Clear form
      setUrl('');

      // Call success callback
      onSuccess?.(response);
    } catch (err) {
      const errorMessage = typeof err === 'string' ? err : 'Failed to ingest URL';
      setError(errorMessage);
      showErrorToast('Web ingestion failed', err);
    } finally {
      setIsLoading(false);
    }
  };

  const isInline = variant === 'inline';

  return (
    <form
      onSubmit={handleSubmit}
      className={`${isInline ? 'flex items-start gap-3' : 'space-y-4'} ${className}`}
    >
      <div className={isInline ? 'flex-1' : 'w-full'}>
        <Input
          type="url"
          value={url}
          onChange={handleUrlChange}
          placeholder="https://example.com/article"
          error={error}
          isLoading={isLoading}
          disabled={isLoading}
          showClearButton={url.length > 0 && !isLoading}
          onClear={handleClear}
          leftIcon={<Globe className="w-4 h-4" />}
          aria-label="Web URL to ingest"
          helperText={!error ? "Enter a web URL to extract and index its content" : undefined}
        />
      </div>

      <div className={`flex items-center gap-2 ${isInline ? '' : 'justify-end'}`}>
        {onCancel && (
          <Button
            type="button"
            variant="ghost"
            onClick={onCancel}
            disabled={isLoading}
          >
            Cancel
          </Button>
        )}
        <Button
          type="submit"
          variant="primary"
          isLoading={isLoading}
          disabled={!url.trim() || isLoading}
          leftIcon={isLoading ? undefined : <Globe className="w-4 h-4" />}
        >
          {isLoading ? 'Ingesting...' : 'Ingest URL'}
        </Button>
      </div>
    </form>
  );
}

export default WebUrlInput;

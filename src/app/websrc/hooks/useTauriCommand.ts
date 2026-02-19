import { useState, useCallback, useEffect } from 'react';

import { useError } from '../contexts/ErrorContext';
import { getErrorMessage } from '../lib/errorUtils';

interface UseTauriCommandOptions<T> {
  onSuccess?: (_data: T) => void;
  onError?: (_error: string) => void;
  showErrorToast?: boolean;
  errorMessage?: string;
}

interface UseTauriCommandResult<T, Args extends unknown[]> {
  data: T | null;
  error: string | null;
  loading: boolean;
  execute: (..._args: Args) => Promise<T | null>;
  reset: () => void;
}

export function useTauriCommand<T, Args extends unknown[] = []>(
  command: (..._args: Args) => Promise<T>,
  options: UseTauriCommandOptions<T> = {}
): UseTauriCommandResult<T, Args> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const { handleTauriError } = useError();

  const execute = useCallback(
    async (...args: Args): Promise<T | null> => {
      setLoading(true);
      setError(null);

      try {
        const result = await command(...args);
        setData(result);

        if (options.onSuccess) {
          options.onSuccess(result);
        }

        return result;
      } catch (err) {
        const errorMessage = getErrorMessage(err);
        setError(errorMessage);

        if (options.showErrorToast !== false) {
          handleTauriError(
            options.errorMessage || errorMessage,
            {
              recoverable: true,
              action: {
                label: 'Retry',
                onClick: () => execute(...args),
              },
            }
          );
        }

        if (options.onError) {
          options.onError(errorMessage);
        }

        return null;
      } finally {
        setLoading(false);
      }
    },
    [command, options, handleTauriError]
  );

  const reset = useCallback(() => {
    setData(null);
    setError(null);
    setLoading(false);
  }, []);

  return {
    data,
    error,
    loading,
    execute,
    reset,
  };
}

export function useTauriQuery<T, Args extends unknown[] = []>(
  command: (..._args: Args) => Promise<T>,
  args: Args,
  options: UseTauriCommandOptions<T> & {
    enabled?: boolean;
    refetchInterval?: number;
  } = {}
): UseTauriCommandResult<T, Args> & { refetch: () => Promise<void> } {
  const commandResult = useTauriCommand(command, options);
  const { execute } = commandResult;

  const refetch = useCallback(async () => {
    await execute(...args);
  }, [execute, args]);

  // Initial fetch on mount
  useEffect(() => {
    if (options.enabled !== false) {
      refetch();
    }
  }, [options.enabled, refetch]);

  // Set up refetch interval with proper cleanup
  useEffect(() => {
    if (!options.refetchInterval) return;

    const interval = setInterval(() => {
      refetch();
    }, options.refetchInterval);

    // Cleanup on unmount or when interval changes
    return () => clearInterval(interval);
  }, [options.refetchInterval, refetch]);

  return {
    ...commandResult,
    refetch,
  };
}

import { QueryClient } from '@tanstack/react-query';

/**
 * React Query Client Configuration
 *
 * Configures the global QueryClient instance with optimized defaults for the Recall desktop app.
 *
 * **Configuration rationale**:
 * - `staleTime: 1000 * 60 * 5` (5 minutes) - Data considered fresh for 5 minutes to reduce unnecessary refetches
 * - `gcTime: 1000 * 60 * 30` (30 minutes) - Cache persists for 30 minutes after queries become inactive
 * - `retry: 3` - Retry failed requests up to 3 times with exponential backoff
 * - `refetchOnWindowFocus: false` - Don't refetch on window focus (desktop app behavior)
 * - `refetchOnReconnect: true` - Refetch when network reconnects (important for sync)
 *
 * @see https://tanstack.com/query/latest/docs/reference/QueryClient
 *
 * @example
 * ```tsx
 * import { QueryClientProvider } from '@tanstack/react-query';
 * import { queryClient } from './lib/queryClient';
 *
 * <QueryClientProvider client={queryClient}>
 *   <App />
 * </QueryClientProvider>
 * ```
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // How long data is considered fresh (5 minutes)
      staleTime: 1000 * 60 * 5,

      // How long inactive queries are cached in memory (30 minutes)
      // Previously called 'cacheTime' in v4, renamed to 'gcTime' (garbage collection time) in v5
      gcTime: 1000 * 60 * 30,

      // Retry failed requests 3 times with exponential backoff
      retry: 3,

      // Retry delay function (exponential backoff: 1s, 2s, 4s)
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000),

      // Don't refetch on window focus (desktop app behavior)
      refetchOnWindowFocus: false,

      // Refetch when network reconnects (important for multi-device sync)
      refetchOnReconnect: true,

      // Don't refetch on mount if data is fresh
      refetchOnMount: false,
    },
    mutations: {
      // Retry mutations once on network errors
      retry: 1,

      // Retry delay for mutations (2 seconds)
      retryDelay: 2000,
    },
  },
});

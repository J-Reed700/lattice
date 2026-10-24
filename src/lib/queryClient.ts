import { QueryClient, QueryCache, MutationCache } from '@tanstack/react-query';

import { diagnostics } from '../utils/diagnostics';

export const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: (error) => diagnostics.capture(error, 'Background query') }),
  mutationCache: new MutationCache({ onError: (error) => diagnostics.capture(error, 'Save operation') }),
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60 * 5,
      gcTime: 1000 * 60 * 30,
      retry: 3,
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 30000),
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
      // Revalidate stale or invalidated data when returning to a screen. Fresh
      // data still uses the cache. `false` also suppresses invalidated queries,
      // hiding changes completed in the background until the next poll.
      refetchOnMount: true,
    },
    mutations: {
      // A failed response can follow a committed write. Only mutations with
      // durable idempotency guarantees may opt in to automatic retries.
      retry: false,
    },
  },
});

import type { ReactNode } from 'react';

import { QueryClientProvider, type QueryClient } from '@tanstack/react-query';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { ConversationsProvider } from '@/features/chat/controller/ConversationsProvider';
import VaultAPI from '@/lib/api';
import { queryClient as defaultQueryClient } from '@/lib/queryClient';

interface AppProvidersProps {
  children: ReactNode;
  api?: ApiClient;
  queryClient?: QueryClient;
}

/** Keep service and cache dependencies together for the lifetime of a tree. */
export function AppProviders({ children, api = VaultAPI, queryClient = defaultQueryClient }: AppProvidersProps) {
  return (
    <ApiProvider client={api}>
      <QueryClientProvider client={queryClient}>
        <ConversationsProvider>{children}</ConversationsProvider>
      </QueryClientProvider>
    </ApiProvider>
  );
}

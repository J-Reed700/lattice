import { createContext, useContext, type ReactNode } from 'react';

import VaultAPI, { type ApiClient } from '@/lib/api';

export type { ApiClient } from '@/lib/api';

// Standalone consumers use the desktop adapter. The app composition root
// supplies it explicitly; tests and embedded trees can supply their own client.
const ApiContext = createContext<ApiClient>(VaultAPI);

export function ApiProvider({ client, children }: { client: ApiClient; children: ReactNode }) {
  return <ApiContext.Provider value={client}>{children}</ApiContext.Provider>;
}

/** Read dependencies during render, then pass them to non-React helpers. */
export function useApi(): ApiClient {
  return useContext(ApiContext);
}

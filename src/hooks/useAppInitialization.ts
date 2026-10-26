import { useEffect, useState } from 'react';

import { useApi } from '@/contexts/ApiContext';
import { toError } from '@/lib/typeGuards';

type InitializationState =
  | { status: 'pending' }
  | { status: 'ready' }
  | { status: 'error'; error: string };

/** Confirm the native bridge is ready without publishing a disposed request. */
export function useAppInitialization(): InitializationState {
  const api = useApi();
  const [state, setState] = useState<InitializationState>({ status: 'pending' });

  useEffect(() => {
    let active = true;
    setState({ status: 'pending' });

    void (async () => {
      try {
        // The backend initializes the database before the renderer starts.
        const result = await api.initializeDatabase();
        if (!active) return;
        setState(result.ok ? { status: 'ready' } : { status: 'error', error: result.error });
      } catch (error) {
        if (active) setState({ status: 'error', error: toError(error).message });
      }
    })();

    return () => { active = false; };
  }, [api]);

  return state;
}

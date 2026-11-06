import { useCallback, useEffect, useState } from 'react';

import { drillsApi } from './api';

/** The languages a drill can be written in, as the backend lists them. */
export function useDrillLanguages() {
  const [languages, setLanguages] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [round, setRound] = useState(0);

  useEffect(() => {
    let current = true;
    setError(null);
    drillsApi.codeReviewLanguages().then(
      (found) => {
        if (current) setLanguages(found);
      },
      (failure: unknown) => {
        if (current) setError(failure instanceof Error ? failure.message : String(failure));
      },
    );
    return () => {
      current = false;
    };
  }, [round]);

  const reload = useCallback(() => setRound((previous) => previous + 1), []);
  return { languages, error, reload };
}

import { useEffect, useState } from 'react';

import { drillsApi } from './api';

/** The language of a course's practice profile, once it has been asked for; none when it names none or cannot be read. */
export function usePracticeLanguage(programId: string): { settled: boolean; language: string | null } {
  const [found, setFound] = useState<{ programId: string; language: string | null } | null>(null);

  useEffect(() => {
    let current = true;
    const settle = (language: string | null) => {
      if (current) setFound({ programId, language });
    };
    drillsApi.practiceLanguage(programId).then(settle, () => settle(null));
    return () => {
      current = false;
    };
  }, [programId]);

  if (found?.programId !== programId) return { settled: false, language: null };
  return { settled: true, language: found.language };
}

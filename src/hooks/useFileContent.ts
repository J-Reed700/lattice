import { useState, useEffect } from 'react';

import { VaultAPI } from '../lib/api';

export function useFileContent(filePath: string | undefined, enabled = true) {
  const [content, setContent] = useState<string>('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!filePath || !enabled) {
      setContent('');
      setError(null);
      setIsLoading(false);
      return;
    }

    let active = true;
    setContent('');
    const fetchContent = async () => {
      setIsLoading(true);
      setError(null);

      try {
        const result = await VaultAPI.readFileContent(filePath);
        if (!result.ok) throw new Error(result.error);
        if (active) setContent(result.data);
      } catch (err) {
        if (active) {
          console.error('Failed to load file:', err);
          setError(err instanceof Error ? err.message : 'Failed to load file');
        }
      } finally {
        if (active) setIsLoading(false);
      }
    };

    fetchContent();
    return () => {
      active = false;
    };
  }, [filePath, enabled]);

  return { content, isLoading, error };
}

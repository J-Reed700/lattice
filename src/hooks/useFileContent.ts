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
      return;
    }

    const fetchContent = async () => {
      setIsLoading(true);
      setError(null);

      try {
        const result = await VaultAPI.readFileContent(filePath);
        if (!result.ok) throw new Error(result.error);
        setContent(result.data);
      } catch (err) {
        console.error('Failed to load file:', err);
        setError(err instanceof Error ? err.message : 'Failed to load file');
      } finally {
        setIsLoading(false);
      }
    };

    fetchContent();
  }, [filePath, enabled]);

  return { content, isLoading, error };
}

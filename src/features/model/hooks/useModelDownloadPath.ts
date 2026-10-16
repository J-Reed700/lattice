import { useCallback, useEffect, useState } from 'react';

import { VaultAPI } from '@/lib/api';

/** Where downloaded models are stored; empty when the backend cannot say. */
export function useModelDownloadPath() {
  const [modelDownloadPath, setModelDownloadPath] = useState('');
  const [isLoadingModelDownloadPath, setIsLoadingModelDownloadPath] = useState(true);

  const loadModelDownloadPath = useCallback(async () => {
    setIsLoadingModelDownloadPath(true);
    try {
      const result = await VaultAPI.getModelDownloadPath();
      setModelDownloadPath(result.ok ? result.data : '');
    } catch {
      // The command can be missing on older builds. Fall through to the
      // error copy instead of leaving the row stuck on "Loading…".
      setModelDownloadPath('');
    } finally {
      setIsLoadingModelDownloadPath(false);
    }
  }, []);

  useEffect(() => {
    void loadModelDownloadPath();
  }, [loadModelDownloadPath]);

  return { modelDownloadPath, isLoadingModelDownloadPath, loadModelDownloadPath };
}

import { useCallback, useEffect, useMemo } from 'react';

import {
  THEMES_MIN_DOCUMENTS,
  useClustersQuery,
  useRebuildProgress,
  useRebuildThemes,
} from '@/features/files/hooks/useClustersQuery';
import { type DocumentMetadata, type FileBrowserActions, type LibraryScope } from '@/types/fileBrowser';

/** The latest themes run, the rebuild that refreshes it, and the scope fallback when a theme vanishes. */
export function useLibraryThemes({
  documents,
  scope,
  setScope,
}: {
  documents: DocumentMetadata[];
  scope: LibraryScope;
  setScope: FileBrowserActions['setScope'];
}) {

  const themesEnabled = documents.length >= THEMES_MIN_DOCUMENTS;
  const themesQuery = useClustersQuery(themesEnabled);
  const themes = useMemo(() => themesQuery.data ?? [], [themesQuery.data]);
  const rebuildThemes = useRebuildThemes();
  const isFindingThemes = rebuildThemes.isPending;
  const themeProgress = useRebuildProgress(isFindingThemes);
  const hasRunThemes = themes.length > 0;

  const handleFindThemes = useCallback(() => {
    rebuildThemes.mutate();
  }, [rebuildThemes]);

  // A theme that vanished in the last run must not leave the list empty and
  // unexplained; fall back to the whole library.
  useEffect(() => {
    if (scope.kind === 'theme' && themes.length > 0 && !themes.some((theme) => theme.id === scope.id)) {
      setScope({ kind: 'all' });
    }
  }, [scope, setScope, themes]);

  return { themesEnabled, themes, isFindingThemes, themeProgress, hasRunThemes, handleFindThemes };
}

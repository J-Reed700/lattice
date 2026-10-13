import { useCallback, useState } from 'react';

import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import { toast } from '@/stores/toastStore';

import { type SavedSearchNameDialogMode } from './SavedSearchNameDialog';

/** State and handlers for the dialog that names a new or renamed saved search. */
export function useSavedSearchNameDialog() {
  const createSavedSearch = useFileBrowserStore(state => state.createSavedSearch);
  const updateSavedSearch = useFileBrowserStore(state => state.updateSavedSearch);
  const [savedSearchNameDialogMode, setSavedSearchNameDialogMode] = useState<SavedSearchNameDialogMode | null>(null);
  const [savedSearchNameDialogValue, setSavedSearchNameDialogValue] = useState('');
  const [savedSearchNameDialogTargetId, setSavedSearchNameDialogTargetId] = useState<string | null>(null);

  const openSavedSearchNameDialog = useCallback((
    mode: SavedSearchNameDialogMode,
    value: string,
    targetId: string | null = null
  ) => {
    setSavedSearchNameDialogMode(mode);
    setSavedSearchNameDialogValue(value);
    setSavedSearchNameDialogTargetId(targetId);
  }, []);

  const closeSavedSearchNameDialog = useCallback(() => {
    setSavedSearchNameDialogMode(null);
    setSavedSearchNameDialogValue('');
    setSavedSearchNameDialogTargetId(null);
  }, []);

  const submitSavedSearchNameDialog = useCallback(() => {
    const name = savedSearchNameDialogValue.trim();
    if (!name) {
      toast.warning('Search name cannot be empty');
      return;
    }

    if (savedSearchNameDialogMode === 'create') {
      const searchId = createSavedSearch(name);
      if (!searchId) {
        toast.warning('Saved search not created', {
          message: 'That name is already in use, or nothing is being searched.',
        });
        return;
      }
      closeSavedSearchNameDialog();
      return;
    }

    if (savedSearchNameDialogMode === 'rename' && savedSearchNameDialogTargetId) {
      updateSavedSearch(savedSearchNameDialogTargetId, { name });
      closeSavedSearchNameDialog();
    }
  }, [
    closeSavedSearchNameDialog,
    createSavedSearch,
    savedSearchNameDialogMode,
    savedSearchNameDialogTargetId,
    savedSearchNameDialogValue,
    updateSavedSearch,
  ]);

  return {
    savedSearchNameDialogMode,
    savedSearchNameDialogValue,
    setSavedSearchNameDialogValue,
    openSavedSearchNameDialog,
    closeSavedSearchNameDialog,
    submitSavedSearchNameDialog,
  };
}

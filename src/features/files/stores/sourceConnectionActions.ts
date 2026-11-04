import { type StoreApi } from 'zustand';

import { INDEXED_FOLDERS_QUERY_KEY } from '@/features/files/hooks/useIndexedFoldersQuery';
import { SETTINGS_QUERY_KEY } from '@/features/settings/hooks/useSettingsQuery';
import { queryClient } from '@/lib/queryClient';
import { type FileBrowserActions, type SourceConnection } from '@/types/fileBrowser';

import { loadSourceConnections, persistSourceConnections, sortSourceConnections } from './fileBrowserPersistence';
import { type FileBrowserStore } from './fileBrowserStore.types';

type SourceConnectionActions = Pick<
  FileBrowserActions,
  'createSourceConnection' | 'updateSourceConnection' | 'deleteSourceConnection'
> & {
  connectSource: (connection: Omit<SourceConnection, 'id' | 'createdAt' | 'updatedAt'>) => string;
  disconnectSource: (sourceId: string) => void;
  loadSourceConnections: () => void;
};

const invalidateFolderAndSettingsQueries = () => {
  void queryClient.invalidateQueries({ queryKey: INDEXED_FOLDERS_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
};

/** The client-side source-connection actions of the file browser store; each change is persisted. */
export const createSourceConnectionActions = (
  set: StoreApi<FileBrowserStore>['setState']
): SourceConnectionActions => {
  const actions: SourceConnectionActions = {
    createSourceConnection: (connection) => {
      if (connection.provider === 'local_folder') {
        throw new Error('Local folders are backend-owned and cannot be stored as client connections.');
      }

      const now = new Date().toISOString();
      const sourceId = `source:${connection.provider}:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;

      const newConnection: SourceConnection = {
        ...connection,
        id: sourceId,
        createdAt: now,
        updatedAt: now,
      };

      set((state) => {
        const sourceConnections = sortSourceConnections([newConnection, ...state.sourceConnections]);
        persistSourceConnections(sourceConnections);
        return { sourceConnections };
      });

      invalidateFolderAndSettingsQueries();

      return sourceId;
    },

    updateSourceConnection: (sourceId: string, updates: Partial<SourceConnection>) => {
      set((state) => {
        const sourceConnections = state.sourceConnections.map((connection) =>
          connection.id === sourceId && connection.provider !== 'local_folder'
            ? {
                ...connection,
                ...updates,
                id: connection.id,
                createdAt: connection.createdAt,
                updatedAt: new Date().toISOString(),
              }
            : connection
        );

        persistSourceConnections(sourceConnections);
        return { sourceConnections };
      });
    },

    deleteSourceConnection: (sourceId: string) => {
      set((state) => {
        const sourceConnections = state.sourceConnections.filter(connection => connection.id !== sourceId);
        persistSourceConnections(sourceConnections);
        return { sourceConnections };
      });

      invalidateFolderAndSettingsQueries();
    },

    connectSource: (connection) => actions.createSourceConnection(connection),
    disconnectSource: (sourceId) => actions.deleteSourceConnection(sourceId),
    loadSourceConnections: () => {
      const sourceConnections = loadSourceConnections();
      set({ sourceConnections });
    },
  };

  return actions;
};

export function connectSource(connection: Omit<SourceConnection, 'id' | 'createdAt' | 'updatedAt'>): string {
  invalidateFolderAndSettingsQueries();
  return `source:${connection.provider}:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
}

export function disconnectSource(_sourceId: string): void {
  invalidateFolderAndSettingsQueries();
}

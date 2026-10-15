import { type StoreApi } from 'zustand';

import { type FileBrowserActions, type SourceConnection } from '@/types/fileBrowser';

import { persistSourceConnections, sortSourceConnections } from './fileBrowserPersistence';
import { type FileBrowserStore } from './fileBrowserStore.types';

type SourceConnectionActions = Pick<
  FileBrowserActions,
  'createSourceConnection' | 'updateSourceConnection' | 'deleteSourceConnection'
>;

/** The client-side source-connection actions of the file browser store; each change is persisted. */
export const createSourceConnectionActions = (
  set: StoreApi<FileBrowserStore>['setState']
): SourceConnectionActions => ({
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
  },
});

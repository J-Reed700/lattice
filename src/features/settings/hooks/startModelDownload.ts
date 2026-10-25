/**
 * Starting a model download, in one place.
 *
 * Both the result row and the detail panel offer a Download button; the
 * command call, response contract, and toasts are identical, so
 * they share this instead of drifting apart.
 */

import { type QueryClient } from '@tanstack/react-query';

import { embeddingBlockReason } from '@/features/settings/components/ModelCatalog/catalogUtils';
import { VaultAPI, type ApiClient } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import type { ModelMetadata } from '@/types/modelCatalog';
import { voidAsync } from '@/utils/promiseHandlers';

type AddToast = ReturnType<typeof useToastStore.getState>['addToast'];

export interface StartModelDownloadArgs {
  metadata: ModelMetadata;
  addToast: AddToast;
  queryClient: QueryClient;
}

export interface StartModelDownloadResult {
  /** The backend reported the files were already on disk. */
  alreadyDownloaded: boolean;
}

export async function startModelDownload({
  metadata,
  addToast,
  queryClient,
}: StartModelDownloadArgs,
  api: ApiClient = VaultAPI,
): Promise<StartModelDownloadResult> {
  const blockReason = embeddingBlockReason(metadata);
  if (blockReason) {
    addToast({
      type: 'warning',
      title: 'Model not supported',
      message: blockReason,
      duration: 8000,
    });
    return { alreadyDownloaded: false };
  }

  const result = await api.downloadModel(metadata.id);
  if (!result.ok) {
    addToast({
      type: 'error',
      title: "Couldn't start the download",
      message: result.error,
    });
    return { alreadyDownloaded: false };
  }

  const response = result.data;
  if (response.status === 'started') {
    addToast({
      type: 'success',
      title: 'Download started',
      message: metadata.name,
    });
  } else if (response.status === 'already_downloaded') {
    addToast({
      type: 'info',
      title: 'Already downloaded',
      message: `${metadata.name} is on disk.`,
    });
    voidAsync(() => queryClient.invalidateQueries({ queryKey: ['downloaded-models'] }))();
    return { alreadyDownloaded: true };
  } else if (response.status === 'network_error') {
    addToast({
      type: 'error',
      title: "Couldn't reach the model host",
      message: 'Check your connection and try again.',
    });
  } else if (response.status === 'failed') {
    addToast({
      type: 'error',
      title: "Couldn't start the download",
      message: 'The download could not be started.',
    });
  }

  return { alreadyDownloaded: false };
}

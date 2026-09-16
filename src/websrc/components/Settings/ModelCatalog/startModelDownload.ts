/**
 * Starting a model download, in one place.
 *
 * Both the result row and the detail panel offer a Download button; the
 * command call, response contract, and toasts are identical, so
 * they share this instead of drifting apart.
 */

import { type QueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

import { getErrorMessage } from '../../../lib/errorUtils';
import { useToastStore } from '../../../stores/toastStore';

import type { DownloadModelResponse } from '../../../types/download';
import type { ModelMetadata } from '../../../types/modelCatalog';

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
}: StartModelDownloadArgs): Promise<StartModelDownloadResult> {
  try {
    const response = await invoke<DownloadModelResponse>('plugin:model|download_model', {
      modelId: metadata.id,
    });

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
      queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
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
  } catch (error) {
    const errorMessage = getErrorMessage(error);

    if (errorMessage.includes('Rate limit')) {
      addToast({
        type: 'warning',
        title: 'Too many downloads at once',
        message: 'Wait a moment, then try again.',
      });
    } else {
      addToast({
        type: 'error',
        title: "Couldn't start the download",
        message: errorMessage,
      });
    }
  }

  return { alreadyDownloaded: false };
}

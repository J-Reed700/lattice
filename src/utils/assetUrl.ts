import { convertFileSrc } from '@tauri-apps/api/core';

/** The URL the webview loads a file on disk from (Tauri's asset protocol). */
export function assetUrl(path: string): string {
  return convertFileSrc(path);
}

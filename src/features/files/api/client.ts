import { open } from '@tauri-apps/plugin-dialog';

import { documentApi } from './documents';

export const filesApi = {
  ...documentApi,
  /**
   * Opens a native file picker dialog for the user to select a single file.
   * Uses Tauri's dialog plugin to provide a platform-native file selection experience.
   *
   * @returns Selected file path or null if cancelled
   *
   * @example
   * const result = await VaultAPI.selectFile();
   * if (result) {
   *   console.log('Selected file:', result);
   *   await VaultAPI.indexFile(result);
   * }
   */
  selectFile: async (): Promise<string | null> => {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: 'Select File',
      });
      return selected;
    } catch (error) {
      console.error('File selection error:', error);
      return null;
    }
  },

  /**
   * Opens a native file picker dialog for the user to select multiple files.
   * Uses Tauri's dialog plugin to provide a platform-native file selection experience.
   *
   * @returns Array of selected file paths or empty array if cancelled
   *
   * @example
   * const result = await VaultAPI.selectMultipleFiles();
   * if (result.length > 0) {
   *   console.log('Selected files:', result);
   *   for (const filePath of result) {
   *     await VaultAPI.indexFile(filePath);
   *   }
   * }
   */
  selectMultipleFiles: async (): Promise<string[]> => {
    try {
      const selected = await open({
        directory: false,
        multiple: true,
        title: 'Select Files',
      });
      // open() returns string[] when multiple=true, string when multiple=false
      if (Array.isArray(selected)) {
        return selected;
      }
      // If null or undefined, return empty array
      return [];
    } catch (error) {
      console.error('Multiple file selection error:', error);
      return [];
    }
  },

  /**
   * Opens a native folder picker dialog for the user to select a folder.
   * Uses Tauri's dialog plugin to provide a platform-native folder selection experience.
   * Commonly used for adding folders to index or selecting output directories.
   *
   * @returns Selected folder path or null if cancelled
   *
   * @example
   * const result = await VaultAPI.selectFolder();
   * if (result) {
   *   console.log('Selected folder:', result);
   *   await VaultAPI.startIndexing(result, true);
   * }
   */
  selectFolder: async (): Promise<string | null> => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder',
      });
      return selected;
    } catch (error) {
      console.error('Folder selection error:', error);
      return null;
    }
  },
};

import { open } from '@tauri-apps/plugin-dialog';

import { documentApi } from './documents';

export const filesApi = {
  ...documentApi,

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

  selectMultipleFiles: async (): Promise<string[]> => {
    try {
      const selected = await open({
        directory: false,
        multiple: true,
        title: 'Select Files',
      });
      if (Array.isArray(selected)) {
        return selected;
      }
      return [];
    } catch (error) {
      console.error('Multiple file selection error:', error);
      return [];
    }
  },

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

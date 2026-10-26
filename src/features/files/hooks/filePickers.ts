import VaultAPI, { type ApiClient } from '@/lib/api';

/** Asks for files to add; answers with none when the dialog is dismissed. */
export const chooseFiles = (api: ApiClient = VaultAPI): Promise<string[]> => api.selectMultipleFiles();

/** Asks for a folder to add; answers with null when the dialog is dismissed. */
export const chooseFolder = (api: ApiClient = VaultAPI): Promise<string | null> => api.selectFolder();

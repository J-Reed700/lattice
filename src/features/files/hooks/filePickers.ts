import VaultAPI from '@/lib/api';

/** Asks for files to add; answers with none when the dialog is dismissed. */
export const chooseFiles = (): Promise<string[]> => VaultAPI.selectMultipleFiles();

/** Asks for a folder to add; answers with null when the dialog is dismissed. */
export const chooseFolder = (): Promise<string | null> => VaultAPI.selectFolder();

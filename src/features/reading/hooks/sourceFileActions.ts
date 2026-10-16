import VaultAPI from '@/lib/api';

/** A cited document's current path, by id; the path a citation kept can be stale. */
export const resolveSourcePath = (documentId: string) => VaultAPI.getFilePathById(documentId);

/** Opens a cited document in the system viewer, or says to render it in the app. */
export const openSourceDocument = (documentId: string) => VaultAPI.openFileById(documentId);

/** Opens the file at `path` in the system's default app, or says to render it in the app. */
export const openSourcePath = (path: string) => VaultAPI.openFile(path);

/** Reveals the file at `path` in Finder or Explorer. */
export const revealSourcePath = (path: string) => VaultAPI.showInFolder(path);

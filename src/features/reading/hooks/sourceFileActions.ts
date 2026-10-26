import VaultAPI, { type ApiClient } from '@/lib/api';

/** A cited document's current path, by id; the path a citation kept can be stale. */
export const resolveSourcePath = (documentId: string, api: ApiClient = VaultAPI) => api.getFilePathById(documentId);

/** Opens a cited document in the system viewer, or says to render it in the app. */
export const openSourceDocument = (documentId: string, api: ApiClient = VaultAPI) => api.openFileById(documentId);

/** Opens the file at `path` in the system's default app, or says to render it in the app. */
export const openSourcePath = (path: string, api: ApiClient = VaultAPI) => api.openFile(path);

/** Reveals the file at `path` in Finder or Explorer. */
export const revealSourcePath = (path: string, api: ApiClient = VaultAPI) => api.showInFolder(path);

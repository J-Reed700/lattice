import { useApi } from '@/contexts/ApiContext';

const JSON_FILTERS = [{ name: 'JSON', extensions: ['json'] }];

/**
 * Saves and opens JSON files in the system dialog. The backend shows the
 * dialog and writes or reads the file, so no path reaches the webview. A
 * cancel answers false from `saveJsonFile` and null from `openJsonFile`.
 */
export function useJsonFileDialogs() {
  const api = useApi();
  return {
    saveJsonFile: (suggestedName: string, contents: string) =>
      api.saveTextFile(suggestedName, JSON_FILTERS, contents),
    openJsonFile: () => api.openTextFile(JSON_FILTERS),
  };
}

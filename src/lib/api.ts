/** Public API facade. Feature clients own commands; shared IPC owns transport. */
import { backupApi } from '@/features/backup/api/client';
import { batchApi } from '@/features/batch/api/client';
import { cacheApi } from '@/features/cache/api/client';
import { chatApi } from '@/features/chat/api/client';
import { compareApi } from '@/features/compare/api/client';
import { corpusShapeApi } from '@/features/corpus-shape/api/client';
import { downloadApi } from '@/features/download/api/client';
import { embeddingsApi } from '@/features/embeddings/api/client';
import { explorerApi } from '@/features/explorer/api/client';
import { extractionApi } from '@/features/extraction/api/client';
import { favoritesApi } from '@/features/favorites/api/client';
import { filesApi } from '@/features/files/api/client';
import { functionsApi } from '@/features/functions/api/client';
import { healthApi } from '@/features/health/api/client';
import { huggingfaceApi } from '@/features/huggingface/api/client';
import { journalApi } from '@/features/journal/api/client';
import { learningApi } from '@/features/learning/api/client';
import { mentionApi } from '@/features/mention/api/client';
import { modelApi } from '@/features/model/api/client';
import { qaApi } from '@/features/qa/api/client';
import { referencesApi } from '@/features/references/api/client';
import { searchApi } from '@/features/search/api/client';
import { settingsApi } from '@/features/settings/api/client';
import { studyApi } from '@/features/study/api/client';
import { tagsApi } from '@/features/tags/api/client';
import { transcriptionApi } from '@/features/transcription/api/client';
import { updatesApi } from '@/features/updates/api/client';
import { vaultApi } from '@/features/vault/api/client';
import { webApi } from '@/features/web/api/client';

export { isUnknownCommandError } from '@/shared/ipc/transport';

export const VaultAPI = {
  ...healthApi,
  ...searchApi,
  ...backupApi,
  ...filesApi,
  ...settingsApi,
  ...webApi,
  ...journalApi,
  ...tagsApi,
  ...qaApi,
  ...referencesApi,
  ...studyApi,
  ...explorerApi,
  ...learningApi,
  ...compareApi,
  ...favoritesApi,
  ...cacheApi,
  ...updatesApi,
  ...huggingfaceApi,
  ...batchApi,
  ...mentionApi,
  ...corpusShapeApi,
  ...embeddingsApi,
  ...extractionApi,
  ...chatApi,
  ...functionsApi,
  ...downloadApi,
  ...modelApi,
  ...vaultApi,
  ...transcriptionApi,
};
export default VaultAPI;

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { learningCanvasKey } from '@/features/learning/api/learningQueryKeys';
import { unwrap } from '@/lib/api';
import type {
  CreateLearningCanvasRequestDto,
  CreateLearningCanvasSnapshotRequestDto,
  LearningCanvasWorkspaceDto,
  RestoreLearningCanvasSnapshotRequestDto,
  SaveLearningCanvasRequestDto,
} from '@/lib/bindings';
import type { ApiResult } from '@/types';

export { learningCanvasKey } from '@/features/learning/api/learningQueryKeys';

export function useLearningCanvas(programId: string) {
  const api = useApi();
  return useQuery<LearningCanvasWorkspaceDto>({
    queryKey: learningCanvasKey(programId),
    queryFn: async () => unwrap(await api.getLearningCanvasWorkspace(programId)),
    enabled: Boolean(programId),
  });
}

function useCanvasMutation<TRequest>(mutation: (request: TRequest) => Promise<ApiResult<LearningCanvasWorkspaceDto>>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: (workspace) => client.setQueryData(learningCanvasKey(workspace.programId), workspace),
  });
}

export function useCreateLearningCanvas() {
  const api = useApi();
  return useCanvasMutation<CreateLearningCanvasRequestDto>(api.createLearningCanvas);
}
export function useSaveLearningCanvas() {
  const api = useApi();
  return useCanvasMutation<SaveLearningCanvasRequestDto>(api.saveLearningCanvas);
}
export function useCreateLearningCanvasSnapshot() {
  const api = useApi();
  return useCanvasMutation<CreateLearningCanvasSnapshotRequestDto>(api.createLearningCanvasSnapshot);
}
export function useRestoreLearningCanvasSnapshot() {
  const api = useApi();
  return useCanvasMutation<RestoreLearningCanvasSnapshotRequestDto>(api.restoreLearningCanvasSnapshot);
}

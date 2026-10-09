import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningCanvasKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  CreateLearningCanvasRequestDto,
  CreateLearningCanvasSnapshotRequestDto,
  LearningCanvasWorkspaceDto,
  RestoreLearningCanvasSnapshotRequestDto,
  SaveLearningCanvasRequestDto,
} from '@/lib/bindings';

export { learningCanvasKey } from '@/features/learning/api/learningQueryKeys';

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningCanvas(programId: string) {
  return useQuery<LearningCanvasWorkspaceDto>({
    queryKey: learningCanvasKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningCanvasWorkspace(programId)),
    enabled: Boolean(programId),
  });
}

function useCanvasMutation<TRequest>(mutation: (request: TRequest) => Promise<{ ok: true; data: LearningCanvasWorkspaceDto } | { ok: false; error: string }>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: (workspace) => client.setQueryData(learningCanvasKey(workspace.programId), workspace),
  });
}

export function useCreateLearningCanvas() { return useCanvasMutation<CreateLearningCanvasRequestDto>(VaultAPI.createLearningCanvas); }
export function useSaveLearningCanvas() { return useCanvasMutation<SaveLearningCanvasRequestDto>(VaultAPI.saveLearningCanvas); }
export function useCreateLearningCanvasSnapshot() { return useCanvasMutation<CreateLearningCanvasSnapshotRequestDto>(VaultAPI.createLearningCanvasSnapshot); }
export function useRestoreLearningCanvasSnapshot() { return useCanvasMutation<RestoreLearningCanvasSnapshotRequestDto>(VaultAPI.restoreLearningCanvasSnapshot); }

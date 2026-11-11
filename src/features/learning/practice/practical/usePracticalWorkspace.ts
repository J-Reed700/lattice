import { useQuery } from "@tanstack/react-query";

import { useApi } from '@/contexts/ApiContext';
import { learningPracticalWorkspaceKey as workspaceKey } from "@/features/learning/api/learningQueryKeys";
import { unwrap } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import type { LearningPracticalWorkspaceDto } from "@/lib/bindings";

export function usePracticalWorkspace(programId: string) {
  const api = useApi();
  return useQuery<LearningPracticalWorkspaceDto>({
    queryKey: workspaceKey(programId),
    queryFn: async () =>
      unwrap(await api.getLearningPracticalWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 2_000,
    retry: false,
    refetchInterval: (query) =>
      query.state.data?.runs.some(
        (run) => run.status === "pending" || run.status === "running",
      )
        ? 1_000
        : false,
  });
}

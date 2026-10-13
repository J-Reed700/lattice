import { useQuery } from "@tanstack/react-query";

import { learningPracticalWorkspaceKey as workspaceKey } from "@/features/learning/api/learningQueryKeys";
import { unwrap } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import VaultAPI from "@/lib/api";
import type { LearningPracticalWorkspaceDto } from "@/lib/bindings";

export function usePracticalWorkspace(programId: string) {
  return useQuery<LearningPracticalWorkspaceDto>({
    queryKey: workspaceKey(programId),
    queryFn: async () =>
      unwrap(await VaultAPI.getLearningPracticalWorkspace(programId)),
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

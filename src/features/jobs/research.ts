import { useQueryClient } from '@tanstack/react-query';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { isJobFinished, useJobStatus, type JobDto } from './api';

/** The job kind of a deep research turn; its subject is the conversation. */
export const DEEP_RESEARCH_JOB = 'chat.deep_research';

/** The conversation whose thread a finished research job changed, unless this window is still waiting on it. */
export function researchToRefresh(job: JobDto, waitingOn: ReadonlyMap<string, string>): string | null {
  if (job.kind !== DEEP_RESEARCH_JOB || !isJobFinished(job) || !job.subjectId) return null;
  return waitingOn.has(job.subjectId) ? null : job.subjectId;
}

/**
 * A research turn resumed after a restart answers no request of this window:
 * when it ends, its conversation is read again so the answer, or the failed
 * question, shows. A turn this window is still waiting on is left to it.
 */
export function useDeepResearchJobs(): void {
  const client = useQueryClient();
  useJobStatus((job) => {
    const conversationId = researchToRefresh(job, conversationUiStore.getState().inFlightGenerations);
    if (!conversationId) return;
    void client.invalidateQueries({ queryKey: conversationKeys.messages(conversationId) });
    void client.invalidateQueries({ queryKey: conversationKeys.detail(conversationId) });
  });
}

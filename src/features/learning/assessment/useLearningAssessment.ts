import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningAssessmentWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type {
  DecideLearningFollowUpRequestDto,
  LearningAssessmentFormDto,
  LearningAssessmentWorkspaceDto,
  MutateLearningAssessmentFormRequestDto,
  SaveLearningAssessmentResponseRequestDto,
  StartLearningAssessmentFormRequestDto,
} from '@/lib/bindings';

export { learningAssessmentWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
export const learningAssessmentFormKey = (formId: string | null) => ['learning-assessment-form', formId] as const;

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningAssessmentWorkspace(programId: string) {
  return useQuery<LearningAssessmentWorkspaceDto>({
    queryKey: learningAssessmentWorkspaceKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningAssessmentWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 3_000,
    retry: false,
  });
}

export function useLearningAssessmentForm(formId: string | null) {
  return useQuery<LearningAssessmentFormDto>({
    queryKey: learningAssessmentFormKey(formId),
    queryFn: async () => unwrap(await VaultAPI.getLearningAssessmentForm(formId!)),
    enabled: Boolean(formId),
    staleTime: 0,
    retry: false,
  });
}

function useFormMutation<TRequest>(mutation: (request: TRequest) => ReturnType<typeof VaultAPI.getLearningAssessmentForm>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await mutation(request)),
    onSuccess: async (form) => {
      client.setQueryData(learningAssessmentFormKey(form.id), form);
      await client.invalidateQueries({ queryKey: learningAssessmentWorkspaceKey(form.programId) });
    },
  });
}

export function useStartLearningAssessmentForm() { return useFormMutation<StartLearningAssessmentFormRequestDto>(VaultAPI.startLearningAssessmentForm); }
export function useSaveLearningAssessmentResponse() { return useFormMutation<SaveLearningAssessmentResponseRequestDto>(VaultAPI.saveLearningAssessmentResponse); }
export function useInterruptLearningAssessmentForm() { return useFormMutation<MutateLearningAssessmentFormRequestDto>(VaultAPI.interruptLearningAssessmentForm); }
export function useSubmitLearningAssessmentForm() { return useFormMutation<MutateLearningAssessmentFormRequestDto>(VaultAPI.submitLearningAssessmentForm); }

function useFollowUpMutation(mutation: (request: DecideLearningFollowUpRequestDto) => ReturnType<typeof VaultAPI.getLearningAssessmentWorkspace>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: DecideLearningFollowUpRequestDto) => unwrap(await mutation(request)),
    onSuccess: (workspace) => client.setQueryData(learningAssessmentWorkspaceKey(workspace.programId), workspace),
  });
}

export function useAcceptLearningFollowUp() { return useFollowUpMutation(VaultAPI.acceptLearningFollowUp); }
export function useDismissLearningFollowUp() { return useFollowUpMutation(VaultAPI.dismissLearningFollowUp); }

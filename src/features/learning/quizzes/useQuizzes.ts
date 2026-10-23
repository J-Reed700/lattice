import { useMemo, useRef } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { learningPlanKey, learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import { learningQuizFormKey, learningQuizOverviewKey } from '@/features/learning/quizzes/quizKeys';
import VaultAPI from '@/lib/api';
import type {
  AddLearningQuizMissesToRecallRequestDto,
  ContestLearningQuizGradeRequestDto,
  DecideLearningFollowUpRequestDto,
  LearningAssessmentFormDto,
  LearningGenerationJobActionRequestDto,
  LearningQuizOverviewDto,
  LearningQuizStatusDto,
  MutateLearningAssessmentFormRequestDto,
  OverrideLearningQuizGradeRequestDto,
  PrepareLearningQuizBankRequestDto,
  ReportLearningQuizItemRequestDto,
  ResumeLearningQuizGradingRequestDto,
  SaveLearningAssessmentResponseRequestDto,
  StartLearningQuizRequestDto,
  UpdateLearningQuizSettingsRequestDto,
} from '@/lib/bindings';

type Result<T> = { ok: true; data: T } | { ok: false; error: string };

const POLL_MS = 2_000;

function unwrap<T>(result: Result<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function newOperationId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `op-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

/** One operation id per action until it succeeds, so retrying after a lost
 * response sends the same idempotent request instead of a second one. */
export function useOperationIds() {
  const ids = useRef(new Map<string, string>());
  return useMemo(() => ({
    take(key: string) {
      let id = ids.current.get(key);
      if (!id) { id = newOperationId(); ids.current.set(key, id); }
      return id;
    },
    done(key: string) { ids.current.delete(key); },
  }), []);
}

/** The course exam, then each module's test and its lesson quizzes, in course order. */
export function allQuizStatuses(overview: LearningQuizOverviewDto): LearningQuizStatusDto[] {
  return [overview.exam, ...overview.modules.flatMap((module) => [module.test, ...module.lessons.map((lesson) => lesson.quiz)])];
}

export function lessonQuizStatus(overview: LearningQuizOverviewDto | undefined, lessonId: string) {
  for (const module of overview?.modules ?? []) {
    const found = module.lessons.find((lesson) => lesson.lessonId === lessonId);
    if (found) return found.quiz;
  }
  return undefined;
}

export function moduleTestStatus(overview: LearningQuizOverviewDto | undefined, moduleId: string) {
  return overview?.modules.find((module) => module.moduleId === moduleId)?.test;
}

function overviewIsMoving(overview?: LearningQuizOverviewDto) {
  return Boolean(overview && allQuizStatuses(overview).some((status) => status.bankStatus === 'preparing' || status.verdict === 'pending'));
}

/** A submitted form whose written answers or regrades are still being worked on. */
export function formIsGrading(form?: LearningAssessmentFormDto) {
  const submission = form?.submission;
  if (!submission) return false;
  if (submission.pendingItems > 0 && submission.gradingJobId) return true;
  return submission.itemResults.some((result) => result.contest?.status === 'open');
}

export function useQuizOverview(programId: string, enabled = true) {
  return useQuery<LearningQuizOverviewDto>({
    queryKey: learningQuizOverviewKey(programId),
    queryFn: async () => unwrap(await VaultAPI.getLearningQuizOverview(programId)),
    enabled: enabled && Boolean(programId),
    staleTime: 3_000,
    retry: false,
    refetchInterval: (query) => (overviewIsMoving(query.state.data) ? POLL_MS : false),
  });
}

export function useQuizForm(formId: string | null) {
  return useQuery<LearningAssessmentFormDto>({
    queryKey: learningQuizFormKey(formId),
    queryFn: async () => unwrap(await VaultAPI.getLearningAssessmentForm(formId!)),
    enabled: Boolean(formId),
    staleTime: 0,
    retry: false,
    refetchInterval: (query) => (formIsGrading(query.state.data) ? POLL_MS : false),
  });
}

function useFormMutation<TRequest>(call: (request: TRequest) => Promise<Result<LearningAssessmentFormDto>>, refreshOverview = true) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: TRequest) => unwrap(await call(request)),
    onSuccess: async (form) => {
      client.setQueryData(learningQuizFormKey(form.id), form);
      if (refreshOverview) await client.invalidateQueries({ queryKey: learningQuizOverviewKey(form.programId) });
    },
  });
}

export const useStartQuiz = () => useFormMutation<StartLearningQuizRequestDto>(VaultAPI.startLearningQuiz);
// Autosave changes nothing the overview shows, so it only refreshes the form.
export const useSaveQuizResponse = () => useFormMutation<SaveLearningAssessmentResponseRequestDto>(VaultAPI.saveLearningAssessmentResponse, false);
export const useInterruptQuiz = () => useFormMutation<MutateLearningAssessmentFormRequestDto>(VaultAPI.interruptLearningAssessmentForm);
export const useSubmitQuiz = () => useFormMutation<MutateLearningAssessmentFormRequestDto>(VaultAPI.submitLearningAssessmentForm);
export const useResumeQuizGrading = () => useFormMutation<ResumeLearningQuizGradingRequestDto>(VaultAPI.resumeLearningQuizGrading);
export const useContestQuizGrade = () => useFormMutation<ContestLearningQuizGradeRequestDto>(VaultAPI.contestLearningQuizGrade);
export const useOverrideQuizGrade = () => useFormMutation<OverrideLearningQuizGradeRequestDto>(VaultAPI.overrideLearningQuizGrade);
export const useReportQuizItem = () => useFormMutation<ReportLearningQuizItemRequestDto>(VaultAPI.reportLearningQuizItem, false);

export function useUpdateQuizSettings() {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: UpdateLearningQuizSettingsRequestDto) => unwrap(await VaultAPI.updateLearningQuizSettings(request)),
    onSuccess: (overview) => client.setQueryData(learningQuizOverviewKey(overview.programId), overview),
  });
}

export function usePrepareQuizBank() {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: PrepareLearningQuizBankRequestDto) => unwrap(await VaultAPI.prepareLearningQuizBank(request)),
    onSuccess: async (_job, request) => {
      await Promise.all([
        client.invalidateQueries({ queryKey: learningQuizOverviewKey(request.programId) }),
        client.invalidateQueries({ queryKey: learningPlanKey(request.programId) }),
      ]);
    },
  });
}

/** Stops a grading job; its items stay pending, never failed. */
export function useStopQuizGrading() {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: LearningGenerationJobActionRequestDto & { formId: string }) => {
      const { operationId, programId, jobId, expectedRevision } = request;
      return unwrap(await VaultAPI.cancelLearningGenerationJob({ operationId, programId, jobId, expectedRevision }));
    },
    onSuccess: async (_job, request) => {
      await Promise.all([
        client.invalidateQueries({ queryKey: learningQuizFormKey(request.formId) }),
        client.invalidateQueries({ queryKey: learningQuizOverviewKey(request.programId) }),
        client.invalidateQueries({ queryKey: learningPlanKey(request.programId) }),
      ]);
    },
  });
}

export function useAddQuizMissesToRecall() {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: AddLearningQuizMissesToRecallRequestDto) => unwrap(await VaultAPI.addLearningQuizMissesToRecall(request)),
    onSuccess: async (_result, request) => {
      await client.invalidateQueries({ queryKey: learningRecallKey(request.programId) });
    },
  });
}

function useFollowUpDecision(call: (request: DecideLearningFollowUpRequestDto) => Promise<Result<unknown>>) {
  const client = useQueryClient();
  return useMutation({
    retry: false,
    mutationFn: async (request: DecideLearningFollowUpRequestDto) => unwrap(await call(request)),
    onSuccess: async (_result, request) => {
      await client.invalidateQueries({ queryKey: learningQuizOverviewKey(request.programId) });
    },
  });
}

export const useAcceptFollowUp = () => useFollowUpDecision(VaultAPI.acceptLearningFollowUp);
export const useDismissFollowUp = () => useFollowUpDecision(VaultAPI.dismissLearningFollowUp);

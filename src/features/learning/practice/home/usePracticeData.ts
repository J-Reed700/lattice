import { useMemo } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useCourseDrills } from '@/features/learning/drills/useCourseDrills';
import { usePracticalWorkspace } from '@/features/learning/practice/practical/usePracticalWorkspace';
import { useLearningPracticeWorkspace } from '@/features/learning/practice/useLearningPractice';
import { getErrorMessage } from '@/lib/errorUtils';

import { buildHistory, pickContinue } from './historyItems';
import { practiceHomeApi } from './practiceHomeApi';

export const practiceProfileKey = (programId: string) => ['learning-practice-profile', programId] as const;
const fitKey = (programId: string) => ['learning-practice-fit', programId] as const;
const dueKey = (programId: string) => ['learning-practice-due', programId] as const;
const codingKey = (programId: string) => ['learning-exercise-sessions', programId] as const;

export function usePracticeProfile(programId: string) {
  const client = useQueryClient();
  const query = useQuery({ queryKey: practiceProfileKey(programId), queryFn: () => practiceHomeApi.getProfile(programId), retry: false });
  return {
    profile: query.data,
    loading: query.isPending,
    error: query.isError ? getErrorMessage(query.error) : null,
    retry: () => void query.refetch(),
    reload: async () => (await query.refetch()).data,
    set: (profile: NonNullable<typeof query.data>) => client.setQueryData(practiceProfileKey(programId), profile),
  };
}

/** Whether the course teaches program code. A verdict still being decided is asked for, once, here. */
export function usePracticeFit(programId: string) {
  const query = useQuery({
    queryKey: fitKey(programId),
    queryFn: async () => {
      const fit = await practiceHomeApi.getFit(programId);
      return fit.code === 'checking' ? practiceHomeApi.checkFit(programId) : fit;
    },
    retry: false,
  });
  return {
    fit: query.data,
    checking: query.isPending,
    error: query.isError ? getErrorMessage(query.error) : null,
    retry: () => void query.refetch(),
  };
}

export function usePracticeDue(programId: string) {
  const query = useQuery({ queryKey: dueKey(programId), queryFn: () => practiceHomeApi.due(programId), retry: false });
  return {
    items: query.data?.items,
    loading: query.isPending,
    error: query.isError ? getErrorMessage(query.error) : null,
    retry: () => void query.refetch(),
  };
}

/** Coding sessions, simulations, written attempts and drills of a course as one list. */
export function usePracticeHistory(programId: string) {
  const coding = useQuery({ queryKey: codingKey(programId), queryFn: () => practiceHomeApi.codingSessions(programId), retry: false });
  const practical = usePracticalWorkspace(programId);
  const written = useLearningPracticeWorkspace(programId);
  const drills = useCourseDrills(programId, null);

  const items = useMemo(
    () =>
      buildHistory({
        coding: coding.data ?? [],
        activities: practical.data?.activities ?? [],
        simulations: practical.data?.simulations ?? [],
        attempts: written.data?.sessions ?? [],
        drills: drills.view?.recent ?? [],
      }),
    [coding.data, practical.data, written.data, drills.view],
  );

  const loading = coding.isPending || practical.isPending || written.isPending || (drills.view === null && drills.error === null);
  const failure = coding.error ?? practical.error ?? written.error ?? (drills.error ? new Error(drills.error) : null);

  return {
    items,
    continueItem: pickContinue(items),
    loading,
    error: failure ? getErrorMessage(failure) : null,
    retry: () => {
      void coding.refetch();
      void practical.refetch();
      void written.refetch();
      drills.reload();
    },
  };
}

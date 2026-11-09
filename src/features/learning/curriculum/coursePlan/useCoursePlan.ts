import type { CoursePlanViewDto } from '@/lib/bindings';

const idle = { mutate: () => undefined, mutateAsync: async () => undefined, isPending: false, error: null as Error | null, reset: () => undefined };

export function useCoursePlan(_programId: string): { data: CoursePlanViewDto | undefined; isError: boolean; error: Error | null } {
  return { data: undefined, isError: false, error: null };
}
export const usePlanCourse = () => idle;
export const useEditCoursePlanEntry = () => idle;
export const useChangeCoursePlanEntries = () => idle;
export const useUndoCoursePlanEntry = () => idle;

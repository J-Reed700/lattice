import { Button } from '@/components/ui/button';
import { TopicLists } from '@/features/learning/curriculum/coursePlan/TopicLists';
import { useCoursePlan, usePlanCourse } from '@/features/learning/curriculum/coursePlan/useCoursePlan';
import type { CoursePlanEntryDto } from '@/lib/bindings';

export const IDEAS_SEPARATOR = ' · ';

export function LessonTopics({ entry }: { entry: CoursePlanEntryDto | undefined }) {
  if (!entry) return null;
  const { terms, actions, contrast } = entry.topics;
  const hasMore = terms.length > 0 || actions.length > 0 || contrast.trim() !== '';
  return (
    <div className="mt-1 text-xs leading-5 text-text-muted">
      <p>{entry.topics.ideas.join(IDEAS_SEPARATOR)}</p>
      {hasMore && (
        <details className="mt-1">
          <summary className="cursor-pointer text-text-secondary">More topics</summary>
          <TopicLists topics={entry.topics} showIdeas={false} className="mt-1 space-y-1" />
        </details>
      )}
    </div>
  );
}

export function CoursePlanStatus({ programId, className = '' }: { programId: string; className?: string }) {
  const plan = useCoursePlan(programId).data;
  const planCourse = usePlanCourse();
  const planNow = () => planCourse.mutate({ programId });
  if (plan?.state === 'planned') return null;
  if (planCourse.isPending || plan?.state === 'planning') return <p role="status" className={`text-xs text-text-secondary ${className}`}>Planning topics…</p>;
  if (planCourse.error) return <FailedPlan message={planCourse.error.message} onRetry={planNow} className={className} />;
  if (plan?.state === 'failed') return <FailedPlan message={plan.error ?? ''} onRetry={planNow} className={className} />;
  if (plan?.state === 'none') return <Button type="button" size="sm" variant="outline" className={className} onClick={planNow}>Plan topics</Button>;
  return null;
}

function FailedPlan({ message, onRetry, className }: { message: string; onRetry: () => void; className: string }) {
  return (
    <div role="alert" className={`flex flex-wrap items-center gap-2 text-xs text-danger-fg ${className}`}>
      <span>{message.trim() ? `Couldn't plan topics: ${message}` : "Couldn't plan topics."}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>Try again</Button>
    </div>
  );
}

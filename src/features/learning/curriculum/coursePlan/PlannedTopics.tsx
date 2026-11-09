import { Button } from '@/components/ui/button';
import { TopicLists } from '@/features/learning/curriculum/coursePlan/TopicLists';
import { useCoursePlan, usePlanCourse } from '@/features/learning/curriculum/coursePlan/useCoursePlan';

export const IDEAS_SEPARATOR = ' · ';

export function LessonTopics({ programId, lessonId }: { programId: string; lessonId: string }) {
  const plan = useCoursePlan(programId).data;
  const entry = plan?.state === 'planned' ? plan.entries.find((candidate) => candidate.topics.lessonId === lessonId) : undefined;
  if (!entry) return null;
  return (
    <div className="mt-1 text-xs leading-5 text-text-muted">
      <p>{entry.topics.ideas.join(IDEAS_SEPARATOR)}</p>
      <details className="mt-1">
        <summary className="cursor-pointer text-text-secondary">More topics</summary>
        <TopicLists topics={entry.topics} showIdeas={false} className="mt-1 space-y-1" />
      </details>
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
      <span>{`Couldn't plan topics: ${message}`}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>Try again</Button>
    </div>
  );
}

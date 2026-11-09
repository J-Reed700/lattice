import type { PlannedLesson } from '@/lib/bindings';

const LISTS = [['ideas', 'Ideas'], ['terms', 'Terms'], ['actions', 'Actions']] as const;

export function TopicLists({ topics, showIdeas = true, className }: { topics: PlannedLesson; showIdeas?: boolean; className?: string }) {
  return (
    <div className={className}>
      {LISTS.filter(([key]) => showIdeas || key !== 'ideas').map(([key, label]) => topics[key].length > 0 && (
        <div key={key}>
          <h4 className="text-xs font-semibold text-text-secondary">{label}</h4>
          <ul className="list-disc pl-4">{topics[key].map((item, index) => <li key={`${index}-${item}`}>{item}</li>)}</ul>
        </div>
      ))}
      {topics.contrast.trim() && (
        <div>
          <h4 className="text-xs font-semibold text-text-secondary">Contrast</h4>
          <p>{topics.contrast}</p>
        </div>
      )}
    </div>
  );
}

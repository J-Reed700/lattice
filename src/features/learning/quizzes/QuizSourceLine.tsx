import { BookOpen } from 'lucide-react';

import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningSourceDto } from '@/lib/bindings';

/** "Based on: …" under a question or answer; hidden when citations are switched off. */
export function QuizSourceLine({ ids, sources, label = 'Based on' }: { ids: string[]; sources: LearningSourceDto[]; label?: string }) {
  const visible = useCitationDisplayStore((state) => state.visible);
  if (!visible || ids.length === 0) return null;
  const titles = [...new Set(ids.map((id) => sources.find((source) => source.id === id)?.title).filter((title): title is string => Boolean(title)))];
  return <p className="mt-5 flex items-start gap-1.5 text-xs leading-5 text-text-muted">
    <BookOpen size={13} aria-hidden="true" className="mt-0.5 shrink-0" />
    <span>{label}: {titles.length ? titles.join(' · ') : 'the lesson'}</span>
  </p>;
}

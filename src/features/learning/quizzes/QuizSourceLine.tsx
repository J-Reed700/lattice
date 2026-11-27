import { BookOpen } from 'lucide-react';

import { SavedSourceCitations } from '@/features/learning/sources/SavedSourceCitations';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningPracticeCitationDto, LearningSourceDto } from '@/lib/bindings';

/**
 * "Based on: …" under a question or answer; hidden when citations are switched off.
 *
 * Once the quiz is graded (`programId` given) the sources are citation chips
 * that open in the docked reader, as everywhere else. While it is being taken
 * they are named only: opening the material mid-quiz would change the test.
 */
export function QuizSourceLine({ citations = [], sources, label = 'Based on', programId, ownerKey }: {
  citations?: readonly LearningPracticeCitationDto[];
  sources: LearningSourceDto[];
  label?: string;
  programId?: string;
  /** The answer these sources belong to, when they can be opened. */
  ownerKey?: string;
}) {
  const visible = useCitationDisplayStore((state) => state.visible);
  if (!visible || citations.length === 0) return null;
  const resolved = citations.flatMap((citation) => {
    const source = sources.find((candidate) => candidate.id === citation.versionId);
    if (!source) return [];
    return [{
      number: typeof citation.number === 'number' && Number.isInteger(citation.number) && citation.number > 0
        ? citation.number
        : undefined,
      versionId: citation.versionId,
      sourceId: citation.sourceId || null,
      quote: citation.quote,
      support: citation.support,
      title: source.title,
    }];
  });
  if (programId && ownerKey && resolved.length > 0) {
    return (
      <SavedSourceCitations
        programId={programId}
        ownerKey={ownerKey}
        citations={resolved}
        label={label}
        className="mt-5"
      />
    );
  }
  const titles = [...new Set(resolved.map(({ title }) => title).filter(Boolean))];
  return <p className="mt-5 flex items-start gap-1.5 text-xs leading-5 text-text-muted">
    <BookOpen size={13} aria-hidden="true" className="mt-0.5 shrink-0" />
    <span>{label}: {titles.length ? titles.join(' · ') : 'the lesson'}</span>
  </p>;
}

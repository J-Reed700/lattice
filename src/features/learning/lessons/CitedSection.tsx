import { useMemo, type ReactNode } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { LessonClaimHoverCard } from '@/features/learning/lessons/LessonClaimHoverCard';
import { citationsByNumber, claimMarkVerdict } from '@/features/learning/model/evidencePresentation';
import { anchoredOccurrences } from '@/features/reading/citations';
import { CitationHoverCard } from '@/features/reading/components/CitationHoverCard';
import { CitationSourcesRow } from '@/features/reading/components/CitationSourcesRow';
import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import { useReaderClosesWithOwner } from '@/features/reading/hooks/useReaderClosesWithOwner';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningLessonEvidenceDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';
import { markReply } from '@/utils/replyPieces';

/** One lesson section drawn as a chat answer is: chips after the sentences that stand on a source, each checked sentence marked, and the sources it cites listed beneath. */
export function CitedSection({ lessonId, sectionIndex, body, report, sources, showCitations, wrap }: {
  lessonId: string;
  sectionIndex: number;
  body: string;
  report: LearningLessonEvidenceDto | null | undefined;
  /** The lesson's sources, numbered as the report numbers its passages. */
  sources: SourceWithMetadata[];
  showCitations: boolean;
  wrap: (viewer: ReactNode) => ReactNode;
}) {
  const resolvedLocations = useReaderStore((state) => state.resolvedLocations);
  const pieces = report?.sections.find((section) => section.sectionIndex === sectionIndex)?.pieces;
  const marked = useMemo(() => (pieces ? markReply(pieces) : null), [pieces]);
  const claims = useMemo(
    () => (marked ? (report?.teachingClaims ?? []).filter((claim) => claim.sectionIndex === sectionIndex) : []),
    [marked, report, sectionIndex],
  );
  const claimMarks = useMemo(
    () => (marked ? claims.map((claim) => ({ sentence: claim.contentQuote, verdict: claimMarkVerdict(claim) })) : undefined),
    [marked, claims],
  );
  const anchoredSources = useMemo(() => {
    const sectionOccurrences = anchoredOccurrences(pieces ?? []);
    const lessonOccurrences = anchoredOccurrences(report?.sections.flatMap((section) => section.pieces) ?? []);
    return sources.map((source) => {
      const anchoredBy =
        source.citationId === undefined
          ? undefined
          : (sectionOccurrences.get(source.citationId) ?? lessonOccurrences.get(source.citationId));
      return anchoredBy ? { ...source, anchoredBy } : source;
    });
  }, [pieces, report?.sections, sources]);
  const ownerKey = `lesson:${lessonId}:${sectionIndex}`;
  const citationClaims = useMemo(() => claims.map((claim) => ({ citationIds: claim.cites ?? [] })), [claims]);

  const {
    citationMap, citationNumbers, openSource, articleRef, citationHover, claimHover,
    handleBodyClick, handleBodyMouseOver, handleBodyMouseLeave, hoveredSource,
  } = useCitationInteractions({
    sources: anchoredSources,
    ownerKey,
    numbered: anchoredSources.length > 0,
    showCitations,
    claims: citationClaims,
  });

  useReaderClosesWithOwner(ownerKey);

  const cited = useMemo(() => {
    const numbers = new Set<number>();
    for (const piece of pieces ?? []) {
      if (piece.type === 'reference' && piece.reference.kind === 'source') numbers.add(piece.reference.number);
    }
    return [...numbers].sort((a, b) => a - b).flatMap((number) => {
      const source = citationMap.get(number);
      return source ? [{ number, source }] : [];
    });
  }, [pieces, citationMap]);

  return (
    <>
      <section
        ref={articleRef}
        onClick={handleBodyClick}
        onMouseOver={showCitations ? handleBodyMouseOver : undefined}
        onMouseLeave={showCitations ? handleBodyMouseLeave : undefined}
      >
        {wrap(
          <TiptapViewer
            content={marked ? marked.raw : body}
            citationNumbers={citationNumbers}
            marks={marked?.numbers}
            claims={claimMarks}
            showEvidence={showCitations}
          />,
        )}
        {showCitations && cited.length > 0 && (
          <CitationSourcesRow
            entries={cited.map(({ number, source }) => ({
              number,
              source,
              resolvedLocation: resolvedLocations.get(source.chunkId),
              onView: () => openSource(source),
            }))}
          />
        )}
      </section>
      <CitationHoverCard
        hover={showCitations ? citationHover : null}
        source={hoveredSource}
        location={hoveredSource ? resolvedLocations.get(hoveredSource.chunkId) ?? null : null}
      />
      <LessonClaimHoverCard
        hover={showCitations ? claimHover : null}
        claim={claimHover ? claims[claimHover.index] ?? null : null}
        citations={citationsByNumber(report)}
      />
    </>
  );
}

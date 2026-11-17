import { useMemo, type ReactNode } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { LessonClaimHoverCard } from '@/features/learning/lessons/LessonClaimHoverCard';
import { citationsByNumber, claimMarkVerdict } from '@/features/learning/model/evidencePresentation';
import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import { CitationHoverCard } from '@/features/reading/components/CitationHoverCard';
import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningLessonEvidenceDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';
import { drawReply } from '@/utils/replyPieces';

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
  const drawn = useMemo(() => (pieces ? drawReply(pieces) : null), [pieces]);
  const claims = useMemo(
    () => (drawn ? (report?.teachingClaims ?? []).filter((claim) => claim.sectionIndex === sectionIndex) : []),
    [drawn, report, sectionIndex],
  );
  const claimMarks = useMemo(
    () => (drawn ? claims.map((claim) => ({ sentence: claim.contentQuote, verdict: claimMarkVerdict(claim) })) : undefined),
    [drawn, claims],
  );
  const citationClaims = useMemo(() => claims.map((claim) => ({ citationIds: claim.cites ?? [] })), [claims]);

  const {
    citationMap, citationNumbers, openSource, articleRef, citationHover, claimHover,
    handleBodyClick, handleBodyMouseOver, handleBodyMouseLeave, hoveredSource,
  } = useCitationInteractions({
    sources,
    ownerKey: `lesson:${lessonId}:${sectionIndex}`,
    numbered: sources.length > 0,
    showCitations,
    claims: citationClaims,
  });

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
            content={drawn ? drawn.markdown : body}
            citationNumbers={citationNumbers}
            marks={drawn?.numbers}
            claims={claimMarks}
            showEvidence={showCitations}
          />,
        )}
        {showCitations && cited.length > 0 && (
          <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
            <span className="text-xxs text-text-muted">Sources</span>
            {cited.map(({ number, source }) => (
              <CitationFootnote
                key={`cite-${number}`}
                number={number}
                source={source}
                resolvedLocation={resolvedLocations.get(source.chunkId)}
                onViewFile={() => openSource(source)}
              />
            ))}
          </div>
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

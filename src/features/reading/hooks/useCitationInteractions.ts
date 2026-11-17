import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';

import type { CitationHover } from '@/features/reading/components/CitationHoverCard';
import type { ClaimHover } from '@/features/reading/components/SentenceHoverCard';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { SourceWithMetadata } from '@/types/conversation';
import { createCitationMap } from '@/utils/citations';

interface CitationInteractionOptions {
  sources: SourceWithMetadata[];
  ownerKey: string;
  /** Whether the text's numbers stand for these sources (chat: an assistant message with sources). */
  numbered: boolean;
  showCitations: boolean;
  /** The checked sentences, by the index the viewer's claim marks carry; only their cited numbers are read here. */
  claims: readonly { citationIds: readonly number[] }[];
  /** Line references that open files (Explorer threads); absent elsewhere. */
  codeRefs?: {
    reveal: (path: string, range: { startLine: number; endLine: number }) => void;
    open: (path: string) => void;
  };
}

/** Owns citation navigation and the text's delegated interactions. */
export function useCitationInteractions({
  sources, ownerKey, numbered, showCitations, claims, codeRefs,
}: CitationInteractionOptions) {
  const openReader = useReaderStore((state) => state.open);
  const readerSession = useReaderStore((state) => state.session);
  const citationMap = useMemo(() => createCitationMap(sources), [sources]);
  // The ordered list behind the citation numbers — what `[` / `]` travel over.
  const citationSources = useMemo(
    () =>
      Array.from(citationMap.entries())
        .sort(([a], [b]) => a - b)
        .map(([, source]) => source),
    [citationMap]
  );

  const openCitation = useCallback(
    (index: number, occurrence: number | null = null) => {
      if (index < 0 || !ownerKey) return;
      openReader(ownerKey, citationSources, index, occurrence);
    },
    [ownerKey, openReader, citationSources]
  );

  /**
   * Open the reader on a source by identity, whatever its position.
   *
   * `occurrence` is the mark that was clicked, when one was: the answer cites a
   * source from several sentences and the reader opens on the one that was meant.
   */
  const openSource = useCallback(
    (source: SourceWithMetadata, occurrence: number | null = null) => {
      const index = citationSources.findIndex((candidate) => candidate.chunkId === source.chunkId);
      // A passage the answer never numbered — two chunks that share a citation
      // id keep one of them out of the map — still opens, on its own.
      if (index < 0) {
        if (ownerKey) openReader(ownerKey, [source], 0, occurrence);
        return;
      }
      openCitation(index, occurrence);
    },
    [citationSources, openCitation, openReader, ownerKey]
  );

  // Inline `[n]` chips live inside the rendered answer, so they are handled by
  // delegation: one listener on the body, the chip found by its data attribute.
  const [citationHover, setCitationHover] = useState<CitationHover | null>(null);
  const citationNumbers = useMemo(
    () => (numbered ? Array.from(citationMap.keys()) : []),
    [numbered, citationMap]
  );

  const chipFromEvent = (event: ReactMouseEvent<HTMLElement>): HTMLElement | null =>
    event.target instanceof Element ? event.target.closest<HTMLElement>('[data-cite]') : null;

  const articleRef = useRef<HTMLElement>(null);
  const [claimHover, setClaimHover] = useState<ClaimHover | null>(null);
  const claimTimerRef = useRef<number | null>(null);
  const cancelPendingClaim = () => {
    if (claimTimerRef.current !== null) window.clearTimeout(claimTimerRef.current);
    claimTimerRef.current = null;
  };
  useEffect(() => cancelPendingClaim, []);
  useEffect(() => {
    if (showCitations) return;
    cancelPendingClaim();
    setCitationHover(null);
    setClaimHover(null);
    setClaimActions(null);
  }, [showCitations]);

  /** True where the thread is wide enough that the evidence sits beside the answer. */
  const hasEvidenceMargin = () =>
    Boolean(articleRef.current?.querySelector<HTMLElement>('.evidence-margin')?.offsetParent);

  /**
   * The citation the reader is showing, if it is showing one of ours.
   *
   * Only the answer the reader was opened from lights up: the same number in
   * the answer above means a different passage.
   */
  const readerCitationNumber = useMemo(() => {
    if (!readerSession || !ownerKey || readerSession.ownerKey !== ownerKey) return null;
    const shown = readerSession.citations[readerSession.index];
    if (!shown) return null;
    for (const [number, source] of citationMap) {
      if (source.chunkId === shown.chunkId) return number;
    }
    return null;
  }, [readerSession, ownerKey, citationMap]);

  /** What stays lit when nothing is under the pointer. */
  const restingLitRef = useRef<string | null>(null);

  /** Light every piece of one citation or one sentence; decorations split at each mark. */
  const setLit = useCallback((selector: string | null) => {
    const article = articleRef.current;
    if (!article) return;
    // Nothing hovered falls back to what the reader is showing, so moving the
    // pointer away does not put the open citation out.
    const next = selector ?? restingLitRef.current;
    article.querySelectorAll('.is-lit').forEach((element) => element.classList.remove('is-lit'));
    if (next) article.querySelectorAll(next).forEach((element) => element.classList.add('is-lit'));
  }, []);

  // The open citation is lit in the text as long as the reader shows it: the
  // one mark that was clicked, since the same number further down stands for a
  // different passage — or every mark of the source, when it was opened whole.
  const readerOccurrence = readerSession?.occurrence ?? null;
  useEffect(() => {
    if (readerCitationNumber === null) restingLitRef.current = null;
    else if (readerOccurrence === null) restingLitRef.current = `[data-cite="${readerCitationNumber}"]`;
    else {
      restingLitRef.current = `[data-cite="${readerCitationNumber}"][data-cite-at="${readerOccurrence}"]`;
    }
    setLit(null);
  }, [readerCitationNumber, readerOccurrence, setLit]);

  // Resting on a sentence says why it is trusted; clicking it is how the
  // sentence leaves the chat.
  const [claimActions, setClaimActions] = useState<{ index: number; rect: DOMRect; maxRight: number } | null>(null);

  const handleBodyClick = (event: ReactMouseEvent<HTMLElement>) => {
    const chip = showCitations ? chipFromEvent(event) : null;
    if (chip) {
      const source = citationMap.get(Number(chip.dataset.cite));
      if (!source) return;
      event.preventDefault();
      setCitationHover(null);
      const at = Number(chip.dataset.citeAt);
      openSource(source, Number.isInteger(at) ? at : null);
      return;
    }

    const codeRef =
      codeRefs && event.target instanceof Element ? event.target.closest<HTMLElement>('[data-code-ref]') : null;
    if (codeRefs && codeRef) {
      const path = codeRef.dataset.codeRef;
      if (!path) return;
      // A chip may sit inside a link whose address is the file; never follow it.
      event.preventDefault();
      const startLine = Number(codeRef.dataset.codeRefStart);
      const endLine = Number(codeRef.dataset.codeRefEnd);
      if (codeRef.dataset.codeRefStart && Number.isInteger(startLine) && Number.isInteger(endLine)) {
        codeRefs.reveal(path, { startLine, endLine });
      } else {
        codeRefs.open(path);
      }
      return;
    }

    // A click that ends a text selection is a selection, not a request.
    if (!showCitations || window.getSelection()?.toString()) return;
    const claim =
      event.target instanceof Element ? event.target.closest<HTMLElement>('[data-claim]') : null;
    if (!claim) return;
    const index = Number(claim.dataset.claim);
    if (!claims[index]) return;
    cancelPendingClaim();
    setClaimHover(null);
    const lines = Array.from(claim.getClientRects());
    const rect =
      lines.find((line) => event.clientY >= line.top && event.clientY <= line.bottom) ??
      claim.getBoundingClientRect();
    setClaimActions({ index, rect, maxRight: event.currentTarget.getBoundingClientRect().right });
  };

  const handleBodyMouseOver = (event: ReactMouseEvent<HTMLElement>) => {
    const chip = chipFromEvent(event);
    if (chip) {
      cancelPendingClaim();
      if (claimHover) setClaimHover(null);
      setLit(null);
      const number = Number(chip.dataset.cite);
      if (citationHover?.number === number) return;
      setCitationHover({ number, rect: chip.getBoundingClientRect() });
      return;
    }
    if (citationHover) setCitationHover(null);

    const claim =
      event.target instanceof Element ? event.target.closest<HTMLElement>('[data-claim]') : null;
    if (!claim) {
      cancelPendingClaim();
      if (claimHover) setClaimHover(null);
      setLit(null);
      return;
    }
    const index = Number(claim.dataset.claim);
    if (claimHover?.index === index) return;
    setLit(`[data-claim="${index}"]`);
    // The line under the pointer, not the box around a sentence that wraps.
    const lines = Array.from(claim.getClientRects());
    const rect =
      lines.find((line) => event.clientY >= line.top && event.clientY <= line.bottom) ??
      claim.getBoundingClientRect();
    const next: ClaimHover = { index, rect, maxRight: event.currentTarget.getBoundingClientRect().right };
    cancelPendingClaim();
    // Sweeping the pointer across a paragraph should not flash a card per
    // sentence; once one is open, the next opens at once.
    if (claimHover) {
      setClaimHover(next);
      return;
    }
    claimTimerRef.current = window.setTimeout(() => setClaimHover(next), 280);
  };

  const handleBodyMouseLeave = () => {
    cancelPendingClaim();
    setCitationHover(null);
    setClaimHover(null);
    setLit(null);
  };

  const hoveredSource = citationHover ? citationMap.get(citationHover.number) ?? null : null;
  const hoveredClaim = claimHover ? claims[claimHover.index] ?? null : null;
  // What the margin lights: the citation under the pointer, or every citation
  // of the sentence under it.
  const activeEvidence = useMemo(
    () => (citationHover ? [citationHover.number] : hoveredClaim?.citationIds ?? []),
    [citationHover, hoveredClaim]
  );

  return {
    citationMap, citationNumbers, openCitation, openSource,
    articleRef, citationHover, claimHover, claimActions, setClaimActions,
    handleBodyClick, handleBodyMouseOver, handleBodyMouseLeave,
    hoveredSource, activeEvidence, hasEvidenceMargin, setLit,
  };
}

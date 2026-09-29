import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { ChevronDown, ChevronUp } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { SourceWithMetadata } from '@/types/conversation';
import { formatRelativeTime } from '@/utils/formatters';

import { sentencesByOccurrence, sentencesCiting } from './answerSentences';
import {
  findQuoteSpan,
  findSourcedPassages,
  mergePassages,
  stripCitationMarkers,
  type SourcedPassage,
} from './sourcedPassages';

/**
 * A cited web page, read.
 *
 * The reader shows the immutable page text saved with the citation and marks
 * passages whose wording the answer's cited sentences share. Older citations
 * without a snapshot can still show their saved source text, clearly labelled.
 *
 * Those marks are a wording match and are described as one. The snapshot
 * records what was read, not what the model leaned on, and a highlight that
 * claimed otherwise would be inventing a provenance nobody recorded.
 */

export interface WebArticleViewProps {
  /** The address to read. Null when the source carries none. */
  url: string | null;
  /** The source as the answer cited it: its name, its number, its snippet. */
  source: SourceWithMetadata;
  /** The message whose citations the reader is showing, when it came from one. */
  ownerKey?: string;
  /** Citation-bearing journal text, independent of the loaded chat history. */
  citationContent?: string;
  /**
   * Which of the answer's marks for this source was clicked. The answer cites a
   * page from several sentences; the reader opens on the passage for this one.
   */
  occurrence?: number | null;
  /** Import and Open URL. The compact readers keep them in the footer instead. */
  actions?: ReactNode;
  /** The passage in view, so "Open URL" can point the browser at it too. */
  onActivePassageChange?: (_text: string | null) => void;
}

/** The sentences of an answer that cite one source, and any quoted evidence. */
interface CitedSentences {
  sentences: string[];
  quotes: { text: string; sentenceIndex: number }[];
  /** The sentence the clicked mark sits in, as an index into `sentences`. */
  focus: number | null;
}

const EMPTY_SENTENCES: CitedSentences = { sentences: [], quotes: [], focus: null };

/** Letters and digits only: the verifier and the splitter cut markdown differently. */
function comparable(sentence: string): string {
  return stripCitationMarkers(sentence)
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '');
}

/** Where `target` is among `sentences`, exactly or as the larger part of one. */
function indexOfSentence(sentences: readonly string[], target: string | undefined): number | null {
  if (!target) return null;
  const wanted = comparable(target);
  if (!wanted) return null;
  const keys = sentences.map(comparable);
  const exact = keys.indexOf(wanted);
  if (exact >= 0) return exact;
  const partial = keys.findIndex((key) => key.length > 0 && (key.includes(wanted) || wanted.includes(key)));
  return partial >= 0 ? partial : null;
}

/**
 * Which sentences of the answer cite this source.
 *
 * A verified turn already knows: its claim verdicts name the citations each
 * sentence rests on, and some carry the quote the judge matched. Everything
 * else falls back to the `[n]` markers in the answer text, which is the only
 * record an unverified turn leaves.
 */
function useCitedSentences(
  ownerKey: string | undefined,
  citationNumber: number | undefined,
  occurrence: number | null,
  citationContent: string | undefined,
): CitedSentences {
  const messageVerification = useConversationsStore((state) => state.messageVerification);
  const conversations = useConversationsStore((state) => state.conversations);

  return useMemo(() => {
    if (!citationNumber) return EMPTY_SENTENCES;

    let content = citationContent;
    if (content === undefined && ownerKey) {
      for (const conversation of conversations) {
        content = conversation.messages?.find((candidate) => candidate.id === ownerKey)?.content;
        if (content !== undefined) break;
      }
    }
    // The chips are drawn from the answer's text, so that is where a clicked
    // one is looked up, whichever record then supplies the sentences.
    const clicked =
      occurrence === null || content === undefined
        ? undefined
        : sentencesByOccurrence(content, citationNumber)[occurrence];

    const verdicts = (ownerKey && citationContent === undefined ? messageVerification.get(ownerKey)?.claimVerdicts ?? [] : []).filter((verdict) =>
      verdict.citationIds.includes(citationNumber)
    );
    if (verdicts.length > 0) {
      const sentences = verdicts.map((verdict) => verdict.sentence);
      return {
        sentences,
        quotes: verdicts.flatMap((verdict, sentenceIndex) =>
          verdict.evidenceQuote ? [{ text: verdict.evidenceQuote, sentenceIndex }] : []
        ),
        focus: indexOfSentence(sentences, clicked),
      };
    }

    if (content === undefined) return EMPTY_SENTENCES;
    const sentences = sentencesCiting(content, citationNumber);
    return { sentences, quotes: [], focus: indexOfSentence(sentences, clicked) };
  }, [ownerKey, citationNumber, occurrence, citationContent, messageVerification, conversations]);
}

interface Span {
  start: number;
  end: number;
}

/** Paragraphs, as the page cache separates them: a blank line apart. */
function paragraphSpans(text: string): Span[] {
  const spans: Span[] = [];
  const push = (from: number, to: number) => {
    let start = from;
    let end = to;
    while (start < end && /\s/.test(text[start]!)) start += 1;
    while (end > start && /\s/.test(text[end - 1]!)) end -= 1;
    if (end > start) spans.push({ start, end });
  };

  let cursor = 0;
  for (const match of text.matchAll(/\n\s*\n/g)) {
    const at = match.index ?? 0;
    push(cursor, at);
    cursor = at + match[0].length;
  }
  push(cursor, text.length);
  return spans;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

function prefersReducedMotion(): boolean {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
}

/**
 * Bring a mark into view by scrolling the article, and nothing else.
 *
 * `scrollIntoView` would take every scrollable ancestor with it, including the
 * window, which would move the answer the reader is checking this against.
 */
function scrollPassageIntoView(mark: HTMLElement): void {
  let scroller = mark.parentElement;
  while (scroller && scroller !== document.body) {
    if (scroller.scrollHeight > scroller.clientHeight + 8) break;
    scroller = scroller.parentElement;
  }
  if (!scroller || scroller === document.body) return;

  scrollArticleTo(
    scroller,
    scroller.scrollTop +
      (mark.getBoundingClientRect().top - scroller.getBoundingClientRect().top) -
      scroller.clientHeight / 3
  );
}

function scrollArticleTo(scroller: HTMLElement, top: number): void {
  const target = Math.max(0, top);
  if (typeof scroller.scrollTo === 'function') {
    scroller.scrollTo({ top: target, behavior: prefersReducedMotion() ? 'auto' : 'smooth' });
    return;
  }
  scroller.scrollTop = target;
}

export function WebArticleView({
  url,
  source,
  ownerKey,
  citationContent,
  occurrence = null,
  actions,
  onActivePassageChange,
}: WebArticleViewProps) {
  const snapshot = source.webSnapshot;
  const hasSnapshotText = Boolean(snapshot?.text.trim());
  const savedExcerpt = source.excerpt?.trim() || source.content || '';
  const pageText = hasSnapshotText && snapshot ? snapshot.text : savedExcerpt;
  const { sentences, quotes, focus } = useCitedSentences(ownerKey, source.citationId, occurrence, citationContent);

  const passages = useMemo<SourcedPassage[]>(() => {
    if (!pageText) return [];
    // A quote the judge took off this page is where the answer came from, as
    // exactly as anything here can say; merging lets it win the span it shares
    // with a weaker lexical match.
    const quoted = quotes.flatMap((quote) => {
      const span = findQuoteSpan(pageText, quote.text);
      return span ? [{ ...span, sentenceIndex: quote.sentenceIndex, score: 1 }] : [];
    });
    return mergePassages([...quoted, ...findSourcedPassages(pageText, sentences)], pageText);
  }, [pageText, sentences, quotes]);

  /**
   * The passage to open on: the clicked sentence's, or the first.
   *
   * Looked for on its own rather than by `sentenceIndex`, because passages that
   * touch are merged and the merged one remembers only its strongest sentence.
   * -1 when a sentence was clicked and the page has nothing like it — lighting
   * some other sentence's passage would answer a question nobody asked.
   */
  const openingIndex = useMemo(() => {
    if (focus === null) return 0;
    const quote = quotes.find((candidate) => candidate.sentenceIndex === focus);
    const span =
      (quote && findQuoteSpan(pageText, quote.text)) ??
      findSourcedPassages(pageText, [sentences[focus] ?? ''])[0];
    if (!span) return -1;
    return passages.findIndex((passage) => passage.start <= span.start && span.start < passage.end);
  }, [focus, quotes, sentences, pageText, passages]);

  const paragraphs = useMemo(() => paragraphSpans(pageText), [pageText]);
  const [activeIndex, setActiveIndex] = useState(openingIndex);
  const markRefs = useRef(new Map<number, HTMLElement>());
  const articleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setActiveIndex(openingIndex);
  }, [passages, openingIndex]);

  useEffect(() => {
    const mark = markRefs.current.get(activeIndex);
    if (mark) {
      scrollPassageIntoView(mark);
    } else if (activeIndex < 0 && articleRef.current) {
      // Nothing matched the clicked sentence. Staying where the last click left
      // the page would read as "this is its passage", so go back to the top.
      scrollArticleTo(articleRef.current, 0);
    }
    // `occurrence` too: another mark of the same sentence is a new request to
    // be shown the passage, even though the passage has not changed.
  }, [activeIndex, passages, occurrence]);

  const activePassage = passages[activeIndex];
  const activeText = activePassage ? pageText.slice(activePassage.start, activePassage.end) : null;
  useEffect(() => {
    onActivePassageChange?.(activeText);
    return () => onActivePassageChange?.(null);
  }, [activeText, onActivePassageChange]);

  const registerMark = useCallback((index: number, element: HTMLElement | null) => {
    if (element) markRefs.current.set(index, element);
    else markRefs.current.delete(index);
  }, []);

  const title = snapshot?.title?.trim() || source.fileName;
  const meta = [
    snapshot
      ? `Saved snapshot${snapshot.fetchedAt ? ` · captured ${formatRelativeTime(snapshot.fetchedAt).toLowerCase()}` : ' · capture date unavailable'}`
      : 'Saved source text · full page snapshot unavailable',
    snapshot ? `${pageText.trim() ? pageText.trim().split(/\s+/).length.toLocaleString() : '0'} words` : null,
  ].filter((entry): entry is string => Boolean(entry));

  const renderParagraph = (paragraph: Span): ReactNode => {
    const marked = passages
      .map((passage, index) => ({ passage, index }))
      .filter(({ passage }) => passage.end > paragraph.start && passage.start < paragraph.end);
    if (marked.length === 0) return pageText.slice(paragraph.start, paragraph.end);

    const nodes: ReactNode[] = [];
    let cursor = paragraph.start;
    for (const { passage, index } of marked) {
      const start = Math.max(passage.start, paragraph.start);
      const end = Math.min(passage.end, paragraph.end);
      if (start > cursor) {
        nodes.push(<Fragment key={`text-${cursor}`}>{pageText.slice(cursor, start)}</Fragment>);
      }
      // A passage that runs over a paragraph break is drawn once per paragraph,
      // and only the first of those is where it begins. Registering them all
      // would leave the last one as the scroll target and open the reader at
      // the passage's end, several paragraphs past the sentence it matched.
      const opensHere = passage.start >= paragraph.start;
      nodes.push(
        <mark
          key={`mark-${index}`}
          ref={opensHere ? (element) => registerMark(index, element) : undefined}
          className={
            index === activeIndex
              ? 'source-reader-passage is-lit'
              : 'source-reader-passage'
          }
          title={`Matches: ${sentences[passage.sentenceIndex] ?? ''}`}
        >
          {pageText.slice(start, end)}
        </mark>
      );
      cursor = end;
    }
    if (cursor < paragraph.end) {
      nodes.push(<Fragment key={`text-${cursor}`}>{pageText.slice(cursor, paragraph.end)}</Fragment>);
    }
    return nodes;
  };

  const renderBody = (): ReactNode => (
      <div className="mx-auto max-w-[68ch] py-5">
        {!hasSnapshotText && (
          <p className="mb-4 rounded-md border border-subtle px-3 py-2 text-xs text-[hsl(var(--text-secondary))]">
            {snapshot
              ? 'The saved page snapshot contains no text; showing the saved source excerpt instead.'
              : 'This is the saved source excerpt. A full page snapshot was not saved with this citation.'}
          </p>
        )}
        {snapshot?.truncated && (
          <p className="mb-4 rounded-md border border-subtle px-3 py-2 text-xs text-[hsl(var(--text-secondary))]">
            This saved snapshot was truncated when captured; it may not contain the full page.
          </p>
        )}
        {!pageText && <p className="text-sm text-[hsl(var(--text-muted))]">No saved source text is available.</p>}
        {paragraphs.map((paragraph) => (
          <p
            key={paragraph.start}
            className="mb-4 font-serif text-[15px] leading-7 text-[hsl(var(--text-primary))]"
          >
            {renderParagraph(paragraph)}
          </p>
        ))}
      </div>
  );

  const showMatchNotice = Boolean(pageText) && sentences.length > 0;
  // A verdict's sentence is raw markdown; a reader is shown words.
  const focusedSentence =
    pageText && focus !== null
      ? stripCitationMarkers(sentences[focus] ?? '')
          .replace(/[*_`#>|]+/g, '')
          .replace(/\s+/g, ' ')
          .trim()
      : '';

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-start justify-between gap-4 border-b border-subtle pb-4">
        <div className="min-w-0">
          <p className="truncate text-xs text-[hsl(var(--text-muted))]">
            {url ? hostOf(url) : 'Web source'}
          </p>
          <h3 className="mt-1 break-words font-serif text-lg font-semibold leading-snug text-[hsl(var(--text-primary))]">
            {title}
          </h3>
          {meta.length > 0 && (
            <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">{meta.join(' · ')}</p>
          )}
        </div>
        {actions}
      </div>

      {focusedSentence && (
        <p
          className="shrink-0 truncate border-b border-subtle py-2 text-xs text-[hsl(var(--text-secondary))]"
          title={focusedSentence}
        >
          <span className="text-[hsl(var(--text-muted))]">For the sentence </span>“{focusedSentence}”
        </p>
      )}

      {showMatchNotice && (
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-subtle py-2">
          <p
            className={`text-xs ${
              // A click that found nothing has to say so louder than a count does.
              focus !== null && openingIndex < 0
                ? 'font-medium text-[hsl(var(--text-primary))]'
                : 'text-[hsl(var(--text-muted))]'
            }`}
          >
            {passages.length === 0
              ? 'No passage on this page closely matches the sentences that cite it.'
              : focus !== null && openingIndex < 0
                ? `No passage on this page closely matches that sentence · ${passages.length} ${
                    passages.length === 1 ? 'passage matches' : 'passages match'
                  } the answer's other sentences citing it`
              : `${passages.length} passage${passages.length === 1 ? '' : 's'} match${
                  passages.length === 1 ? 'es' : ''
                } what the answer says · matched on ${
                  quotes.length > 0 && passages.every((passage) => passage.score === 1)
                    ? 'the quoted evidence'
                    : 'wording'
                }`}
          </p>
          {(passages.length > 1 || (passages.length > 0 && activeIndex < 0)) && (
            <div className="flex items-center gap-1">
              <IconButton
                label="Previous matching passage"
                onClick={() => setActiveIndex((index) => Math.max(0, index - 1))}
                disabled={activeIndex <= 0}
              >
                <ChevronUp />
              </IconButton>
              <IconButton
                label="Next matching passage"
                onClick={() => setActiveIndex((index) => Math.min(passages.length - 1, index + 1))}
                disabled={activeIndex === passages.length - 1}
              >
                <ChevronDown />
              </IconButton>
            </div>
          )}
        </div>
      )}

      <div ref={articleRef} className="min-h-0 flex-1 overflow-y-auto">{renderBody()}</div>
    </div>
  );
}

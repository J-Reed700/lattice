import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronUp, Loader2 } from 'lucide-react';

import { IconButton } from '@/components/ui';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { SourceWithMetadata } from '@/types/conversation';
import { formatRelativeTime } from '@/utils/formatters';

import { sentencesCiting } from './answerSentences';
import {
  findQuoteSpan,
  findSourcedPassages,
  mergePassages,
  type SourcedPassage,
} from './sourcedPassages';

/**
 * A cited web page, read.
 *
 * The search engine's one-line snippet is not a source — it is an
 * advertisement for one — so the reader shows the article the turn actually
 * read, out of the page cache, and marks the passages whose wording the
 * answer's cited sentences share.
 *
 * Those marks are a wording match and are described as one. The page cache
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
  /** Import and Open URL. The compact readers keep them in the footer instead. */
  actions?: ReactNode;
  /** The passage in view, so "Open URL" can point the browser at it too. */
  onActivePassageChange?: (_text: string | null) => void;
}

/** The sentences of an answer that cite one source, and any quoted evidence. */
interface CitedSentences {
  sentences: string[];
  quotes: { text: string; sentenceIndex: number }[];
}

const EMPTY_SENTENCES: CitedSentences = { sentences: [], quotes: [] };

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
  citationNumber: number | undefined
): CitedSentences {
  const messageVerification = useConversationsStore((state) => state.messageVerification);
  const conversations = useConversationsStore((state) => state.conversations);

  return useMemo(() => {
    if (!ownerKey || !citationNumber) return EMPTY_SENTENCES;

    const verdicts = (messageVerification.get(ownerKey)?.claimVerdicts ?? []).filter((verdict) =>
      verdict.citationIds.includes(citationNumber)
    );
    if (verdicts.length > 0) {
      return {
        sentences: verdicts.map((verdict) => verdict.sentence),
        quotes: verdicts.flatMap((verdict, sentenceIndex) =>
          verdict.evidenceQuote ? [{ text: verdict.evidenceQuote, sentenceIndex }] : []
        ),
      };
    }

    for (const conversation of conversations) {
      const message = conversation.messages?.find((candidate) => candidate.id === ownerKey);
      if (!message) continue;
      return { sentences: sentencesCiting(message.content, citationNumber), quotes: [] };
    }
    return EMPTY_SENTENCES;
  }, [ownerKey, citationNumber, messageVerification, conversations]);
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

  const top =
    scroller.scrollTop +
    (mark.getBoundingClientRect().top - scroller.getBoundingClientRect().top) -
    scroller.clientHeight / 3;
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
  actions,
  onActivePassageChange,
}: WebArticleViewProps) {
  const {
    data: page,
    error,
    isLoading,
  } = useQuery({
    queryKey: ['web-page', url],
    enabled: Boolean(url),
    queryFn: async () => {
      const result = await VaultAPI.readWebPage(url!);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 5 * 60_000,
    retry: false,
  });

  const pageText = page?.text ?? '';
  const { sentences, quotes } = useCitedSentences(ownerKey, source.citationId);

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

  const paragraphs = useMemo(() => paragraphSpans(pageText), [pageText]);
  const [activeIndex, setActiveIndex] = useState(0);
  const markRefs = useRef(new Map<number, HTMLElement>());

  useEffect(() => {
    setActiveIndex(0);
  }, [passages]);

  useEffect(() => {
    const mark = markRefs.current.get(activeIndex);
    if (mark) scrollPassageIntoView(mark);
  }, [activeIndex, passages]);

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

  const snippet = source.excerpt || source.content || '';
  const title = page?.title?.trim() || source.fileName;
  const meta = [
    page ? `Read ${formatRelativeTime(page.fetchedAt).toLowerCase()}` : null,
    page ? `${page.wordCount.toLocaleString()} words` : null,
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
      nodes.push(
        <mark
          key={`mark-${index}`}
          ref={(element) => registerMark(index, element)}
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

  const renderBody = (): ReactNode => {
    if (isLoading) {
      return (
        <div className="flex h-40 items-center justify-center gap-3 text-sm text-[hsl(var(--text-muted))]">
          <Loader2 size={16} className="animate-spin" />
          Reading the page…
        </div>
      );
    }

    if (error || !page) {
      return (
        <div className="py-6">
          <p className="text-sm text-[hsl(var(--warning-fg))]">
            {url
              ? `Couldn't read this page — ${error instanceof Error ? error.message : 'the page cache has no copy of it.'}`
              : 'This source carries no address to read.'}
          </p>
          <p className="mt-4 font-serif text-[15px] leading-7 text-[hsl(var(--text-primary))]">
            {snippet || 'No preview available.'}
          </p>
          {snippet && (
            <p className="mt-2 text-xs text-[hsl(var(--text-muted))]">
              The search result's summary, not the article.
            </p>
          )}
        </div>
      );
    }

    return (
      <div className="mx-auto max-w-[68ch] py-5">
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
  };

  const showMatchNotice = Boolean(page) && sentences.length > 0;

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

      {showMatchNotice && (
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-subtle py-2">
          <p className="text-xs text-[hsl(var(--text-muted))]">
            {passages.length === 0
              ? 'No passage on this page closely matches the sentences that cite it.'
              : `${passages.length} passage${passages.length === 1 ? '' : 's'} match${
                  passages.length === 1 ? 'es' : ''
                } what the answer says · matched on ${
                  quotes.length > 0 && passages.every((passage) => passage.score === 1)
                    ? 'the quoted evidence'
                    : 'wording'
                }`}
          </p>
          {passages.length > 1 && (
            <div className="flex items-center gap-1">
              <IconButton
                label="Previous matching passage"
                onClick={() => setActiveIndex((index) => Math.max(0, index - 1))}
                disabled={activeIndex === 0}
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

      <div className="min-h-0 flex-1 overflow-y-auto">{renderBody()}</div>
    </div>
  );
}

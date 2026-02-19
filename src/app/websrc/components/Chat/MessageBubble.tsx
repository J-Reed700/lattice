import { useState, useMemo } from 'react';

import {
  Copy,
  Check,
  Bot,
  User,
  AlertCircle,
  Loader2,
  FileText,
  ChevronDown,
  ChevronRight,
  Bookmark,
  ShieldCheck,
  ShieldAlert,
  ShieldOff,
} from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';
import remarkGfm from 'remark-gfm';

import { CitationFootnote } from './CitationFootnote';
import { FilePreviewModal } from './FilePreviewModal';
import { useConversationsStore } from '../../stores/conversationsStore';
import { parseCitations, createCitationMap } from '../../utils/citations';

import type { DisplayMessage, SourceWithMetadata } from '../../types/conversation';


interface MarkdownCodeProps {
  inline?: boolean;
  className?: string;
  children?: React.ReactNode;
}

interface MarkdownComponentProps {
  children?: React.ReactNode;
  href?: string;
}

interface MessageBubbleProps {
  message: DisplayMessage;
}

interface GroupedSourceChunk {
  key: string;
  chunkId: string;
  excerpt: string;
  section?: string;
  chunkIndex?: number;
  score: number;
  highlights?: string[];
}

interface GroupedSourceEntry {
  key: string;
  primarySource: SourceWithMetadata;
  chunks: GroupedSourceChunk[];
  firstSeenIndex: number;
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const [copied, setCopied] = useState(false);
  const [previewSource, setPreviewSource] = useState<SourceWithMetadata | null>(null);
  const [expandedPreviews, setExpandedPreviews] = useState<Set<string>>(new Set());
  const [isSourcesPanelExpanded, setIsSourcesPanelExpanded] = useState(false);
  const [isVerificationPanelExpanded, setIsVerificationPanelExpanded] = useState(false);
  const [showAllVerifiedClaims, setShowAllVerifiedClaims] = useState(false);
  const [showAllUnverifiedClaims, setShowAllUnverifiedClaims] = useState(false);
  const isUser = message.role === 'user';
  const isPending = message.status === 'pending';
  const isFailed = message.status === 'failed';
  const errorMessage = 'error' in message ? message.error : undefined;
  const activeConversationId = useConversationsStore(s => s.activeConversationId);
  const messageBookmarkMap = useConversationsStore(s => s.messageBookmarkMap);
  const messageVerificationMap = useConversationsStore(s => s.messageVerification);
  const bookmarkMessage = useConversationsStore(s => s.bookmarkMessage);
  const unbookmarkMessage = useConversationsStore(s => s.unbookmarkMessage);

  // Get sources for this message from the store
  const lastMessageSources = useConversationsStore(s => s.lastMessageSources);
  const messageId = 'id' in message ? message.id : undefined;
  const messageBookmark = messageId ? messageBookmarkMap.get(messageId) : undefined;
  const verificationSummary = messageId ? messageVerificationMap.get(messageId) : undefined;
  const isMessageBookmarked = Boolean(messageBookmark);
  const canBookmarkMessage = Boolean(activeConversationId && messageId);
  const domMessageId = messageId ? `message-${messageId}` : undefined;
  const sources: SourceWithMetadata[] = useMemo(() => {
    // Check if message has direct sources (from ConversationMessage.sources)
    if ('sources' in message && message.sources && message.sources.length > 0) {
      return message.sources;
    }
    // Fall back to store-tracked sources by message ID
    if (messageId) {
      return lastMessageSources.get(messageId) ?? [];
    }
    return [];
  }, [message, messageId, lastMessageSources]);

  const citationMap = useMemo(() => createCitationMap(sources), [sources]);
  const isAssistantWithSources = !isUser && sources.length > 0;
  const claimsEvaluated = verificationSummary?.claimsEvaluated ?? 0;
  const supportedCountRaw = verificationSummary?.supportedClaims ?? 0;
  const supportedClaims = verificationSummary?.supportedClaimNotes ?? [];
  const unsupportedClaims = verificationSummary?.unsupportedClaims ?? [];
  const unsupportedCount = unsupportedClaims.length;
  const supportedCount = Math.max(
    supportedCountRaw,
    Math.max(0, claimsEvaluated - unsupportedCount)
  );
  const groundedRatio =
    verificationSummary?.groundedRatio ??
    (claimsEvaluated > 0 ? supportedCount / claimsEvaluated : 1);
  const canExpandVerification =
    verificationSummary?.enabled === true &&
    claimsEvaluated > 0;

  const visibleVerifiedClaims = showAllVerifiedClaims
    ? supportedClaims
    : supportedClaims.slice(0, 5);
  const visibleUnverifiedClaims = showAllUnverifiedClaims
    ? unsupportedClaims
    : unsupportedClaims.slice(0, 5);

  const verificationBadge = useMemo(() => {
    if (isUser || !verificationSummary) return null;
    if (!verificationSummary.enabled) {
      return {
        label: 'Verification off',
        icon: ShieldOff,
        className: 'text-white/45 bg-white/5 border-white/15',
      };
    }
    if (claimsEvaluated === 0) {
      return {
        label: 'No verifiable claims',
        icon: ShieldOff,
        className: 'text-white/55 bg-white/5 border-white/15',
      };
    }
    if (unsupportedCount === 0) {
      return {
        label: 'Verified',
        icon: ShieldCheck,
        className: 'text-emerald-200 bg-emerald-500/15 border-emerald-400/35',
      };
    }
    return {
      label: `Partially verified (${unsupportedCount})`,
      icon: ShieldAlert,
      className: 'text-amber-200 bg-amber-500/15 border-amber-400/35',
    };
  }, [claimsEvaluated, isUser, verificationSummary, unsupportedCount]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleBookmarkToggle = async () => {
    if (!activeConversationId || !messageId) return;

    if (isMessageBookmarked) {
      await unbookmarkMessage(activeConversationId, messageId);
      return;
    }

    const compact = message.content.replace(/\s+/g, ' ').trim();
    const title = compact.length > 80 ? `${compact.slice(0, 80)}...` : compact;
    await bookmarkMessage(activeConversationId, messageId, title || null, null);
  };

  const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const termEntropy = (term: string) => {
    if (!term) return 0;
    const counts = new Map<string, number>();
    for (const ch of term) {
      counts.set(ch, (counts.get(ch) || 0) + 1);
    }
    let entropy = 0;
    for (const count of counts.values()) {
      const p = count / term.length;
      entropy -= p * Math.log2(p);
    }
    return entropy;
  };

  const termSalience = (term: string) => {
    const entropy = termEntropy(term);
    const lengthFactor = Math.log(term.length + 1);
    return entropy * (0.65 + 0.35 * lengthFactor);
  };

  const selectInformativeTerms = (terms: string[], maxTerms: number) => {
    const normalized = Array.from(
      new Set(
        terms
          .map((term) => term.trim().toLowerCase())
          .filter((term) => term.length >= 3 && /[a-z]/i.test(term))
      )
    );
    if (normalized.length === 0) return [];

    const ranked = normalized
      .map((term) => ({ term, score: termSalience(term) }))
      .sort((a, b) => b.score - a.score || a.term.localeCompare(b.term));

    const bestScore = ranked[0]?.score ?? 0;
    if (bestScore <= Number.EPSILON) {
      return ranked.slice(0, maxTerms).map((entry) => entry.term);
    }

    const selected = ranked
      .filter((entry) => entry.score >= bestScore * 0.45)
      .slice(0, maxTerms)
      .map((entry) => entry.term);

    if (selected.length > 0) return selected;
    return ranked.slice(0, maxTerms).map((entry) => entry.term);
  };

  const renderHighlightedText = (text: string, highlights?: string[]) => {
    if (!highlights || highlights.length === 0) return text;

    const selected = selectInformativeTerms(highlights, 10);
    if (selected.length === 0) return text;

    const ordered = [...selected].sort((a, b) => b.length - a.length);
    const pattern = new RegExp(`\\b(${ordered.map(escapeRegExp).join('|')})\\b`, 'gi');
    const parts = text.split(pattern);
    const lookup = new Set(selected.map((term) => term.toLowerCase()));

    return parts.map((part, idx) =>
      lookup.has(part.toLowerCase()) ? (
        <mark
          key={`hl-${idx}`}
          className="bg-blue-500/20 text-blue-100 rounded px-0.5"
        >
          {part}
        </mark>
      ) : (
        <span key={`hl-${idx}`}>{part}</span>
      )
    );
  };

  // Render text with inline citation footnotes
  const renderContentWithCitations = (text: string) => {
    if (sources.length === 0) return text;
    
    const segments = parseCitations(text, sources.length);
    const hasAnyCitation = segments.some(s => s.citationNumber !== undefined);
    if (!hasAnyCitation) return text;

    return segments.map((segment, idx) => {
      if (segment.citationNumber !== undefined) {
        const source = citationMap.get(segment.citationNumber);
        if (source) {
          return (
            <CitationFootnote
              key={`cite-${idx}`}
              number={segment.citationNumber}
              source={source}
              onViewFile={() => setPreviewSource(source)}
            />
          );
        }
        // Citation number doesn't match a source, render as text
        return (
          <span key={`cite-${idx}`} className="break-words whitespace-pre-wrap">
            [{segment.citationNumber}]
          </span>
        );
      }
      return (
        <span key={`text-${idx}`} className="break-words whitespace-pre-wrap">
          {segment.text}
        </span>
      );
    });
  };

  const renderNodeWithCitations = (children?: React.ReactNode): React.ReactNode => {
    if (!isAssistantWithSources || children === undefined || children === null) {
      return children;
    }

    if (typeof children === 'string') {
      return renderContentWithCitations(children);
    }

    if (Array.isArray(children)) {
      const onlyText = children.every((child) => typeof child === 'string');
      if (onlyText) {
        return renderContentWithCitations(children.join(''));
      }
    }

    return children;
  };

  const toggleSourcePreview = (sourceKey: string) => {
    setExpandedPreviews((prev) => {
      const next = new Set(prev);
      if (next.has(sourceKey)) {
        next.delete(sourceKey);
      } else {
        next.add(sourceKey);
      }
      return next;
    });
  };

  const isWebSource = (source: SourceWithMetadata): boolean => {
    const category = source.category?.toLowerCase() ?? '';
    return source.documentId.startsWith('web:') || category.includes('web article');
  };

  const maxKbScore = useMemo(() => {
    const kbScores = sources
      .filter((source) => !isWebSource(source))
      .map((source) => source.score)
      .filter((score) => Number.isFinite(score) && score > 0);
    if (kbScores.length === 0) return 0;
    return Math.max(...kbScores);
  }, [sources]);

  const groupedSources = useMemo<GroupedSourceEntry[]>(() => {
    const normalizeExcerpt = (value?: string): string | null => {
      if (!value) return null;
      const trimmed = value.trim();
      return trimmed.length > 0 ? trimmed : null;
    };

    const grouped = new Map<string, GroupedSourceEntry>();

    sources.forEach((source, sourceIdx) => {
      const sourceKey = source.documentId || `${source.filePath}:${source.fileName}`;
      const existing = grouped.get(sourceKey);

      const chunkCandidates = source.chunkExcerpts && source.chunkExcerpts.length > 0
        ? source.chunkExcerpts.map((chunk, chunkIdx) => ({
            chunkId: chunk.chunkId || `${source.chunkId || sourceIdx}-${chunkIdx}`,
            excerpt: chunk.excerpt,
            section: chunk.section,
            chunkIndex: chunk.chunkIndex,
            score: chunk.score,
            highlights: chunk.highlights ?? source.highlights,
          }))
        : [{
            chunkId: source.chunkId || `${sourceKey}:chunk-${sourceIdx}`,
            excerpt: source.excerpt ?? source.content,
            section: source.section,
            chunkIndex: source.chunkIndex,
            score: source.score,
            highlights: source.highlights,
          }];

      const mappedChunks = chunkCandidates
        .map((chunk) => {
          const normalizedExcerpt = normalizeExcerpt(chunk.excerpt);
          if (!normalizedExcerpt) return null;
          const chunkKey = `${sourceKey}:${chunk.chunkId}:${normalizedExcerpt.slice(0, 64)}`;
          return {
            key: chunkKey,
            chunkId: chunk.chunkId,
            excerpt: normalizedExcerpt,
            section: chunk.section,
            chunkIndex: chunk.chunkIndex,
            score: chunk.score,
            highlights: chunk.highlights,
          } as GroupedSourceChunk;
        })
        .filter((chunk): chunk is GroupedSourceChunk => chunk !== null);

      if (!existing) {
        grouped.set(sourceKey, {
          key: sourceKey,
          primarySource: source,
          chunks: mappedChunks,
          firstSeenIndex: sourceIdx,
        });
        return;
      }

      if (source.score > existing.primarySource.score) {
        existing.primarySource = source;
      }

      const seenChunks = new Set(existing.chunks.map((chunk) => chunk.key));
      for (const chunk of mappedChunks) {
        if (!seenChunks.has(chunk.key)) {
          existing.chunks.push(chunk);
          seenChunks.add(chunk.key);
        }
      }
    });

    const groupedSourcesArray = Array.from(grouped.values());

    for (const group of groupedSourcesArray) {
      group.chunks.sort((a, b) => {
        if (a.chunkIndex !== undefined && b.chunkIndex !== undefined) {
          return a.chunkIndex - b.chunkIndex;
        }
        if (a.chunkIndex !== undefined) return -1;
        if (b.chunkIndex !== undefined) return 1;
        return b.score - a.score;
      });
    }

    groupedSourcesArray.sort((a, b) => {
      if (a.primarySource.score === b.primarySource.score) {
        return a.firstSeenIndex - b.firstSeenIndex;
      }
      return b.primarySource.score - a.primarySource.score;
    });

    return groupedSourcesArray;
  }, [sources]);

  const totalExcerptCount = useMemo(
    () => groupedSources.reduce((total, group) => total + group.chunks.length, 0),
    [groupedSources]
  );

  const formatSourceMetric = (source: SourceWithMetadata, fallbackRank: number): string => {
    if (isWebSource(source)) {
      const rank = source.chunkIndex !== undefined ? source.chunkIndex : fallbackRank;
      return `Rank #${rank}`;
    }
    if (maxKbScore <= 0) {
      return `Relevance N/A`;
    }
    const normalized = Math.min(1, Math.max(0, source.score / maxKbScore));
    return `Relevance ${(normalized * 100).toFixed(1)}%`;
  };

  return (
    <>
      <div
        id={domMessageId}
        className={`group flex gap-4 px-6 py-5 ${
          isUser ? 'bg-white/[0.01]' : 'bg-white/[0.03]'
        } hover:bg-white/[0.05] transition-all duration-200`}
      >
        <div className="flex-shrink-0">
          <div
            className={`w-9 h-9 rounded-xl flex items-center justify-center shadow-lg ${
              isUser
                ? 'bg-gradient-to-br from-blue-500 to-indigo-500 shadow-blue-500/20'
                : 'bg-gradient-to-br from-emerald-500 to-teal-500 shadow-emerald-500/20'
            }`}
          >
            {isUser ? (
              <User className="w-4 h-4 text-white" />
            ) : (
              <Bot className="w-4 h-4 text-white" />
            )}
          </div>
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <span className="text-sm font-medium text-white/90">
              {isUser ? 'You' : 'Assistant'}
            </span>
            <span className="text-xs text-white/40">
              {new Date(message.createdAt).toLocaleTimeString([], {
                hour: '2-digit',
                minute: '2-digit',
              })}
            </span>
            {isPending && (
              <div className="flex items-center gap-1.5 text-xs text-blue-400">
                <Loader2 className="w-3 h-3 animate-spin" />
                <span>Sending...</span>
              </div>
            )}
            {isFailed && (
              <div className="flex items-center gap-1.5 text-xs text-red-400">
                <AlertCircle className="w-3 h-3" />
                <span>{errorMessage || 'Failed to send'}</span>
              </div>
            )}
            {verificationBadge && (
              <button
                type="button"
                onClick={() => {
                  if (canExpandVerification) {
                    setIsVerificationPanelExpanded((prev) => !prev);
                  }
                }}
                className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] ${verificationBadge.className} ${
                  canExpandVerification ? 'hover:brightness-110' : 'cursor-default'
                }`}
                aria-expanded={canExpandVerification ? isVerificationPanelExpanded : undefined}
                title={
                  !verificationSummary?.enabled
                    ? 'Grounding verification is disabled in settings.'
                    : canExpandVerification
                      ? 'Click to view verification details.'
                      : 'No evaluable claims for this message.'
                }
              >
                <verificationBadge.icon className="h-3 w-3" />
                {verificationBadge.label}
                {canExpandVerification && (
                  isVerificationPanelExpanded
                    ? <ChevronDown className="h-3 w-3" />
                    : <ChevronRight className="h-3 w-3" />
                )}
              </button>
            )}
          </div>

          <div className="prose prose-invert prose-sm max-w-none break-words [overflow-wrap:anywhere]">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{
                code({ inline, className, children, ...props }: MarkdownCodeProps) {
                  const match = /language-(\w+)/.exec(className || '');
                  return !inline && match ? (
                    <div className="relative group/code">
                      <SyntaxHighlighter
                        style={vscDarkPlus}
                        language={match[1]}
                        PreTag="div"
                        className="rounded-lg !bg-black/40 !mt-2 !mb-2"
                        wrapLongLines
                        customStyle={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}
                        {...props}
                      >
                        {String(children).replace(/\n$/, '')}
                      </SyntaxHighlighter>
                    </div>
                  ) : (
                    <code
                      className="px-1.5 py-0.5 rounded bg-white/10 text-blue-300 font-mono text-sm break-words whitespace-pre-wrap"
                      {...props}
                    >
                      {children}
                    </code>
                  );
                },
                p({ children }: MarkdownComponentProps) {
                  return (
                    <p className="text-white/80 leading-relaxed mb-3 last:mb-0 break-words whitespace-pre-wrap">
                      {renderNodeWithCitations(children)}
                    </p>
                  );
                },
                ul({ children }: MarkdownComponentProps) {
                  return (
                    <ul className="list-disc list-inside space-y-1 text-white/80 break-words">
                      {children}
                    </ul>
                  );
                },
                ol({ children }: MarkdownComponentProps) {
                  return (
                    <ol className="list-decimal list-inside space-y-1 text-white/80 break-words">
                      {children}
                    </ol>
                  );
                },
                li({ children }: MarkdownComponentProps) {
                  return <li className="text-white/80 break-words">{renderNodeWithCitations(children)}</li>;
                },
                blockquote({ children }: MarkdownComponentProps) {
                  return (
                    <blockquote className="border-l-4 border-blue-500/50 pl-4 italic text-white/60 my-3 break-words">
                      {children}
                    </blockquote>
                  );
                },
                a({ children, href }: MarkdownComponentProps) {
                  return (
                    <a
                      href={href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-blue-400 hover:text-blue-300 underline transition-colors"
                    >
                      {children}
                    </a>
                  );
                },
                h1({ children }: MarkdownComponentProps) {
                  return <h1 className="text-2xl font-bold text-white/90 mb-3 mt-4">{children}</h1>;
                },
                h2({ children }: MarkdownComponentProps) {
                  return <h2 className="text-xl font-bold text-white/90 mb-2 mt-3">{children}</h2>;
                },
                h3({ children }: MarkdownComponentProps) {
                  return <h3 className="text-lg font-bold text-white/90 mb-2 mt-2">{children}</h3>;
                },
                table({ children }: MarkdownComponentProps) {
                  return (
                    <div className="overflow-x-auto my-3">
                      <table className="min-w-full border border-white/10 rounded-lg">
                        {children}
                      </table>
                    </div>
                  );
                },
                th({ children }: MarkdownComponentProps) {
                  return (
                    <th className="px-4 py-2 bg-white/5 border-b border-white/10 text-left text-white/90 font-semibold">
                      {children}
                    </th>
                  );
                },
                td({ children }: MarkdownComponentProps) {
                  return (
                    <td className="px-4 py-2 border-b border-white/5 text-white/80">
                      {children}
                    </td>
                  );
                },
              }}
            >
              {message.content}
            </ReactMarkdown>
          </div>

          {/* Verification details panel for assistant messages */}
          {!isUser && verificationSummary && verificationSummary.enabled && claimsEvaluated > 0 && isVerificationPanelExpanded && (
            <div className="mt-3 rounded-xl border border-white/12 bg-white/[0.03] p-3">
              <div className="mb-3 flex flex-wrap items-center gap-2">
                <span className="inline-flex items-center gap-1 rounded-md border border-emerald-400/35 bg-emerald-500/10 px-2 py-1 text-[11px] text-emerald-200">
                  <ShieldCheck className="h-3.5 w-3.5" />
                  Verified: {supportedCount}
                </span>
                <span className="inline-flex items-center gap-1 rounded-md border border-amber-400/35 bg-amber-500/10 px-2 py-1 text-[11px] text-amber-200">
                  <ShieldAlert className="h-3.5 w-3.5" />
                  Unverified: {unsupportedCount}
                </span>
                <span className="inline-flex items-center gap-1 rounded-md border border-white/20 bg-white/5 px-2 py-1 text-[11px] text-white/70">
                  Coverage: {(groundedRatio * 100).toFixed(0)}% ({claimsEvaluated} claim{claimsEvaluated !== 1 ? 's' : ''})
                </span>
              </div>

              <div className="grid gap-3 lg:grid-cols-2">
                <div className="rounded-lg border border-emerald-400/20 bg-emerald-500/5 p-2.5">
                  <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-emerald-200/90">
                    Verified Claims
                  </p>
                  {supportedClaims.length > 0 ? (
                    <>
                      <ul className="space-y-1.5">
                        {visibleVerifiedClaims.map((claim, idx) => (
                          <li
                            key={`verified-${idx}`}
                            className="rounded-md border border-emerald-400/20 bg-black/20 px-2 py-1.5 text-xs text-white/80"
                          >
                            {claim}
                          </li>
                        ))}
                      </ul>
                      {supportedClaims.length > 5 && (
                        <button
                          type="button"
                          onClick={() => setShowAllVerifiedClaims((prev) => !prev)}
                          className="mt-2 text-[11px] text-emerald-200/80 hover:text-emerald-100 transition-colors"
                        >
                          {showAllVerifiedClaims
                            ? 'Show fewer verified claims'
                            : `Show all verified claims (${supportedClaims.length})`}
                        </button>
                      )}
                    </>
                  ) : (
                    <p className="text-xs text-white/55">
                      Verified claim text is not available for this message.
                    </p>
                  )}
                </div>

                <div className="rounded-lg border border-amber-400/20 bg-amber-500/5 p-2.5">
                  <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-amber-200/90">
                    Unverified Claims
                  </p>
                  {unsupportedClaims.length > 0 ? (
                    <>
                      <ul className="space-y-1.5">
                        {visibleUnverifiedClaims.map((claim, idx) => (
                          <li
                            key={`unsupported-${idx}`}
                            className="rounded-md border border-amber-400/20 bg-black/20 px-2 py-1.5 text-xs text-white/80"
                          >
                            {claim}
                          </li>
                        ))}
                      </ul>
                      {unsupportedClaims.length > 5 && (
                        <button
                          type="button"
                          onClick={() => setShowAllUnverifiedClaims((prev) => !prev)}
                          className="mt-2 text-[11px] text-amber-200/80 hover:text-amber-100 transition-colors"
                        >
                          {showAllUnverifiedClaims
                            ? 'Show fewer unverified claims'
                            : `Show all unverified claims (${unsupportedClaims.length})`}
                        </button>
                      )}
                    </>
                  ) : (
                    <p className="text-xs text-white/55">
                      All evaluated claims were grounded in the cited context.
                    </p>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Source citations panel for assistant messages */}
          {isAssistantWithSources && (
            <div className="mt-3 pt-3 border-t border-white/10">
              <button
                onClick={() => setIsSourcesPanelExpanded((prev) => !prev)}
                className="mb-2 inline-flex items-center gap-1.5 text-xs text-white/50 hover:text-white/80 transition-colors"
                aria-expanded={isSourcesPanelExpanded}
              >
                {isSourcesPanelExpanded ? (
                  <ChevronDown className="w-3.5 h-3.5" />
                ) : (
                  <ChevronRight className="w-3.5 h-3.5" />
                )}
                <FileText className="w-3.5 h-3.5" />
                <span>
                  {groupedSources.length} cited document{groupedSources.length !== 1 ? 's' : ''}
                  {totalExcerptCount > groupedSources.length
                    ? ` • ${totalExcerptCount} excerpts`
                    : ''}
                </span>
              </button>
              {isSourcesPanelExpanded && (
                <div className="grid gap-3">
                  {groupedSources.map((group, idx) => {
                    const source = group.primarySource;
                    const sourceKey = group.key;
                    const isExpanded = expandedPreviews.has(sourceKey);
                    const metaParts = [
                      source.category,
                      group.chunks.length > 0
                        ? `${group.chunks.length} excerpt${group.chunks.length !== 1 ? 's' : ''}`
                        : undefined,
                    ].filter(Boolean);

                    return (
                      <div
                        key={sourceKey}
                        className="rounded-xl bg-white/[0.03] border border-white/10 p-4 hover:bg-white/[0.06] transition-all duration-200"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-semibold text-blue-400">
                                [{idx + 1}]
                              </span>
                              <span className="text-sm font-medium text-white/90 truncate">
                                {source.fileName}
                              </span>
                            </div>
                            {metaParts.length > 0 && (
                              <div className="mt-1 text-xs text-white/50 truncate">
                                {metaParts.join(' • ')}
                              </div>
                            )}
                          </div>
                          <button
                            onClick={() => setPreviewSource(source)}
                            className="text-xs px-2 py-1 rounded-md bg-white/5 hover:bg-white/10 border border-white/10 text-white/70 hover:text-white/90 transition-colors"
                            title={`View ${source.fileName}`}
                          >
                            View
                          </button>
                        </div>

                        {group.chunks.length > 0 && (
                          <button
                            onClick={() => toggleSourcePreview(sourceKey)}
                            className="mt-3 inline-flex items-center gap-1.5 text-xs text-white/60 hover:text-white/90 transition-colors"
                            aria-expanded={isExpanded}
                          >
                            {isExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                            <span>{isExpanded ? 'Hide excerpts' : `Show excerpts (${group.chunks.length})`}</span>
                          </button>
                        )}

                        {group.chunks.length > 0 && isExpanded && (
                          <div className="mt-3 space-y-2">
                            {group.chunks.map((chunk) => {
                              const chunkMetaParts = [
                                chunk.section,
                                chunk.chunkIndex !== undefined ? `Chunk ${chunk.chunkIndex + 1}` : undefined,
                              ].filter(Boolean);

                              return (
                                <div
                                  key={chunk.key}
                                  className="rounded-lg border border-white/10 bg-black/20 px-3 py-2"
                                >
                                  {chunkMetaParts.length > 0 && (
                                    <div className="mb-1 text-[11px] text-white/45">
                                      {chunkMetaParts.join(' • ')}
                                    </div>
                                  )}
                                  <p className="text-sm text-white/80 leading-relaxed line-clamp-4 break-words">
                                    {renderHighlightedText(chunk.excerpt, chunk.highlights)}
                                  </p>
                                </div>
                              );
                            })}
                          </div>
                        )}

                        <div className="mt-2 flex items-center gap-3 text-[11px] text-white/40">
                          <span>{formatSourceMetric(source, idx + 1)}</span>
                          <button
                            onClick={() => setPreviewSource(source)}
                            className="text-blue-300 hover:text-blue-200 transition-colors"
                          >
                            Open source
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </div>

        <div className="flex-shrink-0 flex items-start gap-1">
          {canBookmarkMessage && (
            <button
              onClick={() => void handleBookmarkToggle()}
              aria-label={isMessageBookmarked ? 'Remove message bookmark' : 'Bookmark message'}
              className={`transition-all duration-200 p-2 rounded-lg ${
                isMessageBookmarked
                  ? 'opacity-100 bg-amber-500/15 text-amber-300'
                  : 'opacity-0 group-hover:opacity-100 bg-white/5 hover:bg-white/10 text-white/60 hover:text-amber-300'
              }`}
              title={isMessageBookmarked ? 'Remove bookmark' : 'Bookmark message'}
            >
              <Bookmark className={`w-4 h-4 ${isMessageBookmarked ? 'fill-current' : ''}`} />
            </button>
          )}
          <button
            onClick={handleCopy}
            aria-label="Copy message to clipboard"
            className="opacity-0 group-hover:opacity-100 transition-opacity duration-200 p-2 rounded-lg bg-white/5 hover:bg-white/10 text-white/60 hover:text-white/90"
            title="Copy message"
          >
            {copied ? (
              <Check className="w-4 h-4 text-green-400" />
            ) : (
              <Copy className="w-4 h-4" />
            )}
          </button>
        </div>
      </div>

      {/* File preview modal for citation clicks */}
      <FilePreviewModal
        isOpen={previewSource !== null}
        onClose={() => setPreviewSource(null)}
        source={previewSource}
      />
    </>
  );
}

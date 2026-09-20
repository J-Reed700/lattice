/**
 * What bounded conversation memory holds for one chat, read-only.
 *
 * Read-only is the design, not an omission: an editable item would be a
 * requirement with no source behind it, which is the exact failure this layer
 * exists to prevent. Every item here can be traced to a quotation, or says
 * plainly that its quotation is gone.
 *
 * Three things this view has to get right, each guarding a specific
 * misreading:
 *   - A generated `label` must never look like something the user wrote, so
 *     labels are UI chrome and quotations are quotations.
 *   - `evidence[].text === null` means the source message was edited or
 *     deleted. It is rendered as an absence; there is no cached text to fall
 *     back to and hiding the row would quietly turn an unsourced item into a
 *     sourced one.
 *   - `mode` and `featureEnabled` decide whether any of this is in the prompt
 *     at all, so neither may be summarized as "memory is working".
 *
 * Every surviving quotation is also a way back to where it came from: selecting
 * it closes the panel, scrolls the original message into view and marks the
 * validated span inside it. That path is only offered for quotations whose
 * source still resolves — offering it for a lost one would promise a message
 * that may no longer exist.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { ArrowUpRight, X } from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import {
  decodeValidatedSpan,
  findMessageElement,
  scrollToMessage,
} from '../../utils/chatMessageNavigation';

import type {
  ConversationMemoryDetailsDto,
  ConversationMemoryItemDto,
  MemoryEvidenceDto,
} from '../../lib/bindings';

interface ConversationMemoryPanelProps {
  conversationId: string | null;
  isOpen: boolean;
  onClose: () => void;
  /**
   * Stored content of the messages currently in the thread, by message id.
   *
   * Needed because `startByte`/`endByte` index the raw message content while the
   * DOM holds rendered markdown; without the original text there is nothing to
   * resolve a span against, and the panel scrolls without a highlight.
   */
  messageContentById?: ReadonlyMap<string, string>;
}

/**
 * The one sentence at the top that says whether anything here reaches the
 * model. Order matters: the rollout switch outranks `mode`, because a `ready`
 * store that is switched off is still not memory.
 */
function describeStatus(details: ConversationMemoryDetailsDto): {
  tone: 'off' | 'waiting' | 'blocked' | 'active';
  heading: string;
  body: string;
} {
  if (!details.featureEnabled) {
    return {
      tone: 'off',
      heading: 'Bounded memory is off',
      body:
        'Nothing below is being sent to the model. This is a record of what extraction found; none of it reaches a prompt. Turn it on in Settings → AI → Chat.',
    };
  }

  switch (details.mode) {
    case 'ready':
      return {
        tone: 'active',
        heading: 'Source-backed memory in use',
        body:
          'Required items below are added to every prompt in this conversation. Extraction is a model step, so treat this as what it noticed, not everything you said.',
      };
    case 'rebuild_required':
      return {
        tone: 'waiting',
        heading: 'Rebuilding',
        body:
          'Memory is not being added to prompts right now. This is temporary and clears itself the next time this conversation is processed.',
      };
    case 'unsupported_schema':
      return {
        tone: 'blocked',
        heading: 'Written by a newer version of Lattice',
        body:
          'This memory layout cannot be read by this build, so nothing is being added to prompts. Update the app to read it.',
      };
    default:
      // An unknown mode is still not a working one — say so rather than
      // guessing, or a future state silently reads as "ready".
      return {
        tone: 'blocked',
        heading: 'Memory state not recognized',
        body: `This build does not recognize the state "${details.mode}", so nothing is being added to prompts.`,
      };
  }
}

const TONE_CLASS: Record<'off' | 'waiting' | 'blocked' | 'active', string> = {
  off: 'border-[hsl(var(--border-default))] bg-[hsl(var(--text-primary)/0.04)]',
  waiting: 'border-[hsl(var(--warning)/0.4)] bg-[hsl(var(--warning)/0.08)]',
  blocked: 'border-[hsl(var(--danger)/0.4)] bg-[hsl(var(--danger)/0.08)]',
  active: 'border-[hsl(var(--accent)/0.35)] bg-[hsl(var(--accent)/0.07)]',
};

function readableKind(kind: string): string {
  return kind.replace(/_/g, ' ');
}

interface MemoryEvidenceProps {
  quote: MemoryEvidenceDto;
  onOpenSource: (_quote: MemoryEvidenceDto) => void;
  /** True once a jump to this quotation's message failed to find it. */
  isUnreachable: boolean;
}

function MemoryEvidence({ quote, onOpenSource, isUnreachable }: MemoryEvidenceProps) {
  const attribution = `${quote.role} · message ${quote.sequence} · ${readableKind(quote.purpose)}`;

  if (quote.text === null) {
    // No cached copy is shown and the row is not dropped: an item whose source
    // is gone has to look different from one that never had a source. It is
    // also not a link — the message it named may not exist any more, and a
    // control that leads nowhere reads as a bug in the thread, not as deletion.
    return (
      <li className="mt-2" data-testid="memory-evidence-missing">
        <p className="text-xs italic text-text-muted">
          Source no longer available — that message was edited or deleted.
        </p>
        <p className="mt-0.5 text-xxs uppercase tracking-wide text-text-tertiary">{attribution}</p>
      </li>
    );
  }

  return (
    <li className="mt-2" data-testid="memory-evidence-quote">
      <blockquote className="border-l-2 border-[hsl(var(--accent)/0.5)] pl-3">
        <p className="font-serif text-[15px] italic leading-relaxed text-text-secondary">
          “{quote.text}”
        </p>
      </blockquote>
      <button
        type="button"
        onClick={() => onOpenSource(quote)}
        aria-label={`Open message ${quote.sequence} in the conversation and highlight this quotation`}
        data-testid="memory-evidence-open-source"
        className="pressable mt-0.5 ml-3 inline-flex items-center gap-1 rounded text-xxs uppercase tracking-wide text-text-tertiary underline decoration-dotted underline-offset-2 hover:text-text-secondary"
      >
        <span>{attribution}</span>
        <ArrowUpRight className="h-3 w-3" strokeWidth={1.7} aria-hidden="true" />
      </button>
      {isUnreachable && (
        <p
          className="mt-1 ml-3 text-xs text-[hsl(var(--warning-fg))]"
          role="status"
          data-testid="memory-evidence-unreachable"
        >
          Couldn&apos;t find message {quote.sequence} in the thread on screen. It may not be loaded
          yet, or it is no longer there.
        </p>
      )}
    </li>
  );
}

interface MemoryItemProps {
  item: ConversationMemoryItemDto;
  onOpenSource: (_quote: MemoryEvidenceDto) => void;
  unreachableMessageId: string | null;
}

function MemoryItem({ item, onOpenSource, unreachableMessageId }: MemoryItemProps) {
  return (
    <li
      className="rounded-lg border border-[hsl(var(--border-subtle))] bg-surface p-3"
      data-testid="memory-item"
      data-mandatory={item.isMandatory || undefined}
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded bg-[hsl(var(--text-primary)/0.06)] px-1.5 py-0.5 text-xxs uppercase tracking-wide text-text-secondary">
          {readableKind(item.kind)}
        </span>
        {item.isMandatory ? (
          <span className="rounded bg-[hsl(var(--accent)/0.14)] px-1.5 py-0.5 text-xxs uppercase tracking-wide text-[hsl(var(--accent))]">
            In every prompt
          </span>
        ) : (
          <span className="rounded bg-[hsl(var(--text-primary)/0.04)] px-1.5 py-0.5 text-xxs uppercase tracking-wide text-text-tertiary">
            Only when relevant
          </span>
        )}
        {item.review === 'ambiguous' && (
          <span className="rounded bg-[hsl(var(--warning)/0.16)] px-1.5 py-0.5 text-xxs uppercase tracking-wide text-[hsl(var(--warning-fg))]">
            Unsettled
          </span>
        )}
        {item.state !== 'active' && (
          <span className="rounded bg-[hsl(var(--text-primary)/0.04)] px-1.5 py-0.5 text-xxs uppercase tracking-wide text-text-tertiary">
            {item.state}
          </span>
        )}
      </div>

      {/* Chrome type, not prose type: a generated sentence must not be able to
          pass for the user's own words two lines below it. */}
      <p
        className="mt-2 text-ui font-medium not-italic text-text-primary"
        data-testid="memory-item-label"
      >
        {item.label}
      </p>

      <p className="mt-2 text-xxs uppercase tracking-wide text-text-tertiary">
        {item.evidence.length === 1 ? 'Quoted source' : 'Quoted sources'}
      </p>
      {item.evidence.length === 0 ? (
        <p className="mt-1 text-xs italic text-text-muted">No quotation was recorded.</p>
      ) : (
        <ul className="mt-1 list-none">
          {item.evidence.map((quote, index) => (
            <MemoryEvidence
              key={`${quote.messageId}-${quote.startByte}-${index}`}
              quote={quote}
              onOpenSource={onOpenSource}
              isUnreachable={unreachableMessageId === quote.messageId}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export function ConversationMemoryPanel({
  conversationId,
  isOpen,
  onClose,
  messageContentById,
}: ConversationMemoryPanelProps) {
  const [details, setDetails] = useState<ConversationMemoryDetailsDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [includeHistory, setIncludeHistory] = useState(false);
  const [unreachableMessageId, setUnreachableMessageId] = useState<string | null>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (isOpen) {
      returnFocusRef.current = document.activeElement as HTMLElement;
    } else {
      // A fresh open re-reads rather than showing the previous conversation's
      // items while the request is in flight.
      setDetails(null);
      setError(null);
      setIncludeHistory(false);
      setUnreachableMessageId(null);
    }
  }, [isOpen]);

  const load = useCallback(
    async (withHistory: boolean) => {
      if (!conversationId) return;
      setIsLoading(true);
      const result = await VaultAPI.getConversationMemory(conversationId, withHistory);
      if (result.ok) {
        setDetails(result.data);
        setError(null);
      } else {
        setDetails(null);
        setError(result.error);
      }
      setIsLoading(false);
    },
    [conversationId]
  );

  useEffect(() => {
    if (!isOpen || !conversationId) return;
    void load(includeHistory);
  }, [conversationId, includeHistory, isOpen, load]);

  /**
   * Take the reader to the message a quotation came from and mark the span.
   *
   * The thread is checked before the panel closes: if the message is not on
   * screen the panel stays open and says so under that quotation, because
   * closing and scrolling nowhere looks like a dead control.
   */
  const openSource = useCallback(
    (quote: MemoryEvidenceDto) => {
      // Defensive: no control is rendered for a lost source, and following one
      // would lead to a message that may since have been deleted.
      if (quote.text === null) return;

      if (!findMessageElement(quote.messageId)) {
        setUnreachableMessageId(quote.messageId);
        return;
      }

      const content = messageContentById?.get(quote.messageId);
      const decoded =
        content === undefined
          ? null
          : decodeValidatedSpan({
              content,
              startByte: quote.startByte,
              endByte: quote.endByte,
            });
      // Mark only what the backend validated. `quote.text` is that exact byte
      // range resolved against the stored message, so a decode that does not
      // reproduce it means our copy of the message has moved on — scroll there
      // and mark nothing rather than highlighting words that are not evidence.
      const span = decoded?.text === quote.text ? decoded : null;

      setUnreachableMessageId(null);
      onClose();
      scrollToMessage(quote.messageId, { span });
    },
    [messageContentById, onClose]
  );

  const status = details ? describeStatus(details) : null;

  return (
    <Dialog.Root open={isOpen} onOpenChange={(open) => { if (!open) onClose(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-[hsl(var(--overlay))]" />
        <Dialog.Content
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
          }}
          className="fixed right-0 top-0 z-50 flex h-full w-full max-w-[560px] flex-col border-l border-[hsl(var(--border-default))] bg-bg shadow-sheet"
        >
          <header className="flex items-start gap-3 border-b border-[hsl(var(--border-subtle))] px-5 py-4">
            <div className="min-w-0 flex-1">
              <Dialog.Title className="font-serif text-[17px] font-medium tracking-[-0.01em] text-text-primary">
                Conversation memory
              </Dialog.Title>
              <Dialog.Description className="mt-1 text-xs leading-relaxed text-text-muted">
                Read-only. There is no editor here on purpose: an item you could
                type would be a requirement with nothing behind it.
              </Dialog.Description>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close conversation memory"
              className="pressable rounded-md p-1.5 text-text-tertiary hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-secondary"
            >
              <X className="h-4 w-4" strokeWidth={1.7} />
            </button>
          </header>

          <div className="flex-1 overflow-y-auto px-5 py-4">
            {isLoading && !details && (
              <p className="text-ui text-text-muted">Reading memory…</p>
            )}

            {error && (
              <div
                className="rounded-lg border border-[hsl(var(--danger)/0.4)] bg-[hsl(var(--danger)/0.08)] p-3"
                role="alert"
              >
                <p className="text-ui font-medium text-text-primary">Couldn&apos;t read memory</p>
                <p className="mt-1 text-xs leading-relaxed text-text-secondary">{error}</p>
                <button
                  type="button"
                  onClick={() => void load(includeHistory)}
                  className="pressable mt-2 inline-flex h-7 items-center rounded-md bg-action px-2.5 text-xs font-medium text-action-fg"
                >
                  Try again
                </button>
              </div>
            )}

            {details && status && (
              <>
                <div
                  className={`rounded-lg border p-3 ${TONE_CLASS[status.tone]}`}
                  data-testid="memory-status"
                  data-tone={status.tone}
                >
                  <p className="text-ui font-medium text-text-primary">{status.heading}</p>
                  <p className="mt-1 text-xs leading-relaxed text-text-secondary">{status.body}</p>
                </div>

                <dl className="mt-4 grid grid-cols-2 gap-2">
                  <div className="rounded-lg border border-[hsl(var(--border-subtle))] p-2.5">
                    <dt className="text-xxs uppercase tracking-wide text-text-tertiary">
                      Required (every prompt)
                    </dt>
                    <dd className="mt-0.5 text-ui font-medium text-text-primary">
                      {details.activeMandatoryCount}
                    </dd>
                  </div>
                  <div className="rounded-lg border border-[hsl(var(--border-subtle))] p-2.5">
                    <dt className="text-xxs uppercase tracking-wide text-text-tertiary">
                      Optional (when relevant)
                    </dt>
                    <dd className="mt-0.5 text-ui font-medium text-text-primary">
                      {details.activeOptionalCount}
                    </dd>
                  </div>
                  <div className="rounded-lg border border-[hsl(var(--border-subtle))] p-2.5">
                    <dt className="text-xxs uppercase tracking-wide text-text-tertiary">
                      Needs your attention
                    </dt>
                    <dd className="mt-0.5 text-ui font-medium text-text-primary">
                      {details.conflictCount}
                    </dd>
                  </div>
                  <div className="rounded-lg border border-[hsl(var(--border-subtle))] p-2.5">
                    <dt className="text-xxs uppercase tracking-wide text-text-tertiary">
                      Processed through message
                    </dt>
                    <dd className="mt-0.5 text-ui font-medium text-text-primary">
                      {details.processedThroughSequence}
                    </dd>
                    <dd className="mt-0.5 text-xxs leading-relaxed text-text-muted">
                      Everything up to here was submitted for extraction — not a
                      claim that every fact in it was noticed.
                    </dd>
                  </div>
                </dl>

                {details.lastErrorCode && (
                  <p className="mt-3 text-xs text-[hsl(var(--warning-fg))]" role="status">
                    Last extraction error: <code>{details.lastErrorCode}</code>
                  </p>
                )}

                <section className="mt-4">
                  <h3 className="text-xxs uppercase tracking-wide text-text-tertiary">
                    Working summary — generated, and can be wrong
                  </h3>
                  {details.summary ? (
                    <p className="mt-1.5 text-ui leading-relaxed text-text-secondary">
                      {details.summary}
                    </p>
                  ) : (
                    <p className="mt-1.5 text-xs italic text-text-muted">No summary yet.</p>
                  )}
                </section>

                <section className="mt-5">
                  <h3 className="text-xxs uppercase tracking-wide text-text-tertiary">
                    Items
                  </h3>
                  <p className="mt-1 text-xs leading-relaxed text-text-muted">
                    The line in plain type under each label is generated for
                    scanning. The indented quotations are your own words, taken
                    from the messages they name.
                  </p>
                  {details.items.length === 0 ? (
                    <p className="mt-2 text-xs italic text-text-muted">
                      Nothing has been recorded for this conversation.
                    </p>
                  ) : (
                    <ul className="mt-2 flex list-none flex-col gap-2">
                      {details.items.map((item) => (
                        <MemoryItem
                          key={item.id}
                          item={item}
                          onOpenSource={openSource}
                          unreachableMessageId={unreachableMessageId}
                        />
                      ))}
                    </ul>
                  )}
                </section>

                <section className="mt-5 border-t border-[hsl(var(--border-subtle))] pt-4">
                  <h3 className="text-xxs uppercase tracking-wide text-text-tertiary">
                    Superseded and resolved
                  </h3>
                  {includeHistory ? (
                    details.history.length === 0 ? (
                      <p className="mt-1.5 text-xs italic text-text-muted">
                        Nothing has been superseded or resolved yet.
                      </p>
                    ) : (
                      <ul className="mt-2 flex list-none flex-col gap-2">
                        {details.history.map((item) => (
                          <MemoryItem
                            key={item.id}
                            item={item}
                            onOpenSource={openSource}
                            unreachableMessageId={unreachableMessageId}
                          />
                        ))}
                      </ul>
                    )
                  ) : (
                    <>
                      <p className="mt-1 text-xs leading-relaxed text-text-muted">
                        Older items that were replaced or closed out. Not in any
                        prompt.
                      </p>
                      <button
                        type="button"
                        onClick={() => setIncludeHistory(true)}
                        className="pressable mt-2 inline-flex h-7 items-center rounded-md border border-[hsl(var(--border-default))] px-2.5 text-xs font-medium text-text-secondary hover:bg-[hsl(var(--text-primary)/0.05)]"
                      >
                        Show earlier versions
                      </button>
                    </>
                  )}
                </section>

                <p className="mt-5 text-xxs leading-relaxed text-text-tertiary">
                  Schema {details.schemaVersion} · memory revision{' '}
                  {details.memoryRevision} · transcript revision{' '}
                  {details.transcriptRevision}
                  {details.extractorModelIdentity
                    ? ` · extracted by ${details.extractorModelIdentity}`
                    : ''}
                </p>
              </>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

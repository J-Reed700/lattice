import { useState } from 'react';

import { format } from 'date-fns';
import { ChevronRight } from 'lucide-react';

import {
  Button,
  DatePicker,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from '@/components/ui';
import { useConversationKnowledgeQuery, useManageKnowledgeMutation } from '@/features/chat/hooks/useConversationKnowledge';
import type { KnowledgeItemDto, KnowledgeRequestDto } from '@/lib/bindings';

const requestFor = (conversationId: string, action: string): KnowledgeRequestDto => ({
  conversationId, action, itemId: null, text: null, scope: null, kind: null,
  validFrom: null, validUntil: null, offset: null,
});

const aboutTheUser = ['user_fact', 'preference', 'goal'];

const MEMORY_TYPES = [
  { value: 'user_fact', label: 'Fact' },
  { value: 'preference', label: 'Preference' },
  { value: 'constraint', label: 'Requirement' },
  { value: 'goal', label: 'Goal' },
  { value: 'decision', label: 'Decision' },
  { value: 'open_question', label: 'Open question' },
] as const;

function parseLocalDate(isoDate: string): Date | undefined {
  if (!isoDate) return undefined;
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate);
  if (!match) return undefined;
  return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
}

function formatLocalDate(date: Date | undefined): string {
  if (!date) return '';
  return format(date, 'yyyy-MM-dd');
}

function ScopeSelectItems({ pool, onChoose }: { pool: string; onChoose?: (value: string) => void }) {
  return (
    <>
      <SelectItem value="conversation" onPointerUp={() => onChoose?.('conversation')}>This conversation</SelectItem>
      <SelectItem value="pool" onPointerUp={() => onChoose?.('pool')}>This {pool}</SelectItem>
      <SelectItem value="personal" onPointerUp={() => onChoose?.('personal')}>All conversations</SelectItem>
    </>
  );
}

/** Shared facts retain their source conversation. Editing creates user evidence. */
export function KnowledgePanel({ conversationId, onOpenConversation }: { conversationId: string; onOpenConversation?: (id: string) => void }) {
  const [offset, setOffset] = useState(0);
  const [text, setText] = useState('');
  const [chosenScope, setScope] = useState<string | null>(null);
  const [kind, setKind] = useState('user_fact');
  const scope = chosenScope ?? (aboutTheUser.includes(kind) ? 'pool' : 'conversation');
  const [editing, setEditing] = useState<KnowledgeItemDto | null>(null);
  const [from, setFrom] = useState('');
  const [until, setUntil] = useState('');
  const query = useConversationKnowledgeQuery(conversationId, offset);
  const pool = query.data?.pool ?? 'space';
  const sharing = (item: KnowledgeItemDto) => item.scope === pool ? 'pool' : item.scope;
  const mutation = useManageKnowledgeMutation((request) => {
    if (request.action === 'remember' || request.action === 'correct') {
      setText(''); setEditing(null); setFrom(''); setUntil(''); setScope(null);
    }
  });
  const act = (item: KnowledgeItemDto, action: string, extras: Partial<KnowledgeRequestDto> = {}) =>
    mutation.mutate({ ...requestFor(conversationId, action), itemId: item.id, ...extras });
  const date = (value: string) => value ? `${value}T00:00:00Z` : null;
  const beginEdit = (item: KnowledgeItemDto) => {
    setEditing(item); setText(item.label); setScope(sharing(item));
    setFrom(item.validFrom?.slice(0, 10) ?? ''); setUntil(item.validUntil?.slice(0, 10) ?? '');
  };

  return (
    <section className="mt-5 space-y-4 border-t border-border-subtle pt-4">
      <div>
        <h3 className="text-sm font-medium text-text-primary">Saved knowledge</h3>
        <p className="mt-1 text-xs leading-relaxed text-text-muted">
          Facts, decisions and open work you can reuse. What you say about yourself reaches the chats in this {pool} when it bears on them, unless you keep it to this conversation. Decisions and instructions stay here unless you share them. Dates describe when a fact applies; verification records when you checked it.
        </p>
      </div>

      <form
        className="space-y-3 rounded-xl border border-border-subtle bg-surface-sunken/40 p-3"
        onSubmit={(event) => {
          event.preventDefault();
          mutation.mutate({
            ...requestFor(conversationId, editing ? 'correct' : 'remember'),
            itemId: editing?.id ?? null,
            text,
            scope,
            kind,
            validFrom: date(from),
            validUntil: date(until),
          });
        }}
      >
        <div>
          <label className="mb-1.5 block text-xs font-medium text-text-secondary">
            {editing ? 'Correction (saved as your exact words)' : 'Remember something'}
          </label>
          <Textarea
            aria-label="Memory text"
            value={text}
            maxLength={8000}
            onChange={(e) => setText(e.target.value)}
            rows={3}
            resize="vertical"
            placeholder="Add a fact, requirement, preference, or decision…"
          />
        </div>

        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          {!editing && (
            <div>
              <span className="mb-1.5 block text-xs font-medium text-text-secondary">Memory type</span>
              <Select value={kind} onValueChange={setKind}>
                <SelectTrigger aria-label="Memory type" className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {MEMORY_TYPES.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          <div className={editing ? 'sm:col-span-2' : ''}>
            <span className="mb-1.5 block text-xs font-medium text-text-secondary">Available in</span>
            <Select value={scope} onValueChange={setScope}>
              <SelectTrigger aria-label="Memory scope" className="h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <ScopeSelectItems pool={pool} onChoose={setScope} />
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          <div data-testid="knowledge-valid-from">
            <span className="mb-1.5 block text-xs font-medium text-text-secondary">Valid from</span>
            <DatePicker
              selected={parseLocalDate(from)}
              onSelect={(selected) => setFrom(formatLocalDate(selected))}
              placeholder="Start date"
              className="w-full"
            />
          </div>
          <div data-testid="knowledge-valid-until">
            <span className="mb-1.5 block text-xs font-medium text-text-secondary">Valid until</span>
            <DatePicker
              selected={parseLocalDate(until)}
              onSelect={(selected) => setUntil(formatLocalDate(selected))}
              placeholder="End date"
              className="w-full"
            />
          </div>
        </div>

        <div className="flex items-center gap-2 pt-0.5">
          <Button size="sm" disabled={!text.trim() || mutation.isPending} type="submit">
            {editing ? 'Save correction' : 'Remember'}
          </Button>
          {editing && (
            <Button
              size="sm"
              variant="ghost"
              type="button"
              onClick={() => {
                setEditing(null); setText(''); setFrom(''); setUntil(''); setScope(null);
              }}
            >
              Cancel correction
            </Button>
          )}
        </div>
      </form>

      {(query.error || mutation.error) && (
        <p role="alert" className="text-xs text-danger-fg">
          {(mutation.error ?? query.error)?.message}
        </p>
      )}
      {query.isPending && <p className="text-xs text-text-muted">Loading saved knowledge…</p>}

      <ul className="space-y-3">
        {query.data?.items.map((item) => {
          const owned = item.conversationId === conversationId;
          const expired = !!item.validUntil && new Date(item.validUntil).getTime() <= Date.now();
          return (
            <li key={item.id} className="rounded-xl border border-border-subtle bg-surface p-3.5 text-xs">
              <div className="text-text-muted">
                {item.kind.replace(/_/g, ' ')} · {item.scope} · {item.forgotten ? 'forgotten' : item.state}{expired ? ' · expired' : ''}
              </div>
              <p className="mt-1 text-ui font-medium text-text-primary">{item.label}</p>
              {query.data?.lastAnswerMemoryIds.includes(item.id) && (
                <p className="mt-1 text-accent">Included in the latest answer’s initial context</p>
              )}
              <p className="mt-1 text-text-muted">{item.availabilityReason} · {item.conversationTitle}</p>
              <p className="mt-1 text-text-muted tabular-nums">
                Learned {item.learnedAt.slice(0, 10)}
                {item.validFrom ? ` · From ${item.validFrom.slice(0, 10)}` : ''}
                {item.validUntil ? ` · Until ${item.validUntil.slice(0, 10)}` : ''}
                {item.verifiedAt ? ` · Checked ${item.verifiedAt.slice(0, 10)}` : ' · Not manually verified'}
              </p>

              <details className="group mt-2.5 rounded-lg border border-border-subtle bg-surface-sunken/50 px-2.5 py-2">
                <summary className="flex cursor-pointer list-none items-center gap-1.5 text-xs font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary [&::-webkit-details-marker]:hidden">
                  <ChevronRight
                    className="h-3.5 w-3.5 shrink-0 text-text-muted transition-transform duration-fast group-open:rotate-90"
                    strokeWidth={1.75}
                    aria-hidden="true"
                  />
                  Evidence and provenance
                </summary>
                <div className="mt-2 space-y-2 border-t border-border-subtle pt-2">
                  {item.evidence.map((e, i) => (
                    <blockquote key={`${e.messageId}-${i}`} className="border-l-2 border-border-default pl-2.5 text-text-secondary">
                      {e.text ?? 'Source changed or was removed.'}
                      <span className="mt-0.5 block text-[11px] text-text-muted">
                        {e.role} · message {e.sequence}
                      </span>
                    </blockquote>
                  ))}
                </div>
              </details>

              {item.state === 'active' && !item.forgotten && (
                <div className="mt-3 flex flex-wrap items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={mutation.isPending}
                    onClick={() => beginEdit(item)}
                  >
                    Correct
                  </Button>
                  {owned && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={mutation.isPending}
                      onClick={() => act(item, 'verify')}
                    >
                      Mark checked
                    </Button>
                  )}
                  {owned && (
                    <div className="w-40">
                      <Select
                        value={sharing(item)}
                        disabled={mutation.isPending}
                        onValueChange={(nextScope) => act(item, 'scope', { scope: nextScope })}
                      >
                        <SelectTrigger aria-label={`Sharing for ${item.label}`} className="h-7 text-xs">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <ScopeSelectItems pool={pool} />
                        </SelectContent>
                      </Select>
                    </div>
                  )}
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={mutation.isPending}
                    onClick={() => act(item, 'forget')}
                    className="text-text-tertiary hover:text-danger-fg"
                  >
                    Forget saved item
                  </Button>
                </div>
              )}

              {!owned && (
                <p className="mt-2.5 text-text-muted">
                  Correcting or forgetting this changes it in every chat that shares it. Open its source conversation to change its sharing, dates or check.
                  {onOpenConversation && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      className="ml-2 h-6 px-2 text-xs"
                      onClick={() => onOpenConversation(item.conversationId)}
                    >
                      Open source conversation
                    </Button>
                  )}
                </p>
              )}
            </li>
          );
        })}
      </ul>

      <p className="text-xs leading-relaxed text-text-muted">
        Forgetting stops using this saved item. Original conversation messages remain in history. Saved knowledge is evidence, not permission to take actions.
      </p>

      {(offset > 0 || query.data?.hasMore) && (
        <div className="flex items-center gap-2">
          {offset > 0 && (
            <Button type="button" variant="outline" size="sm" onClick={() => setOffset(Math.max(0, offset - 50))}>
              Previous
            </Button>
          )}
          {query.data?.hasMore && (
            <Button type="button" variant="outline" size="sm" onClick={() => setOffset(offset + 50)}>
              Next
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

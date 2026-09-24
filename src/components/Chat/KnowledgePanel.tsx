import { useState } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { VaultAPI } from '../../lib/api';

import type { KnowledgeItemDto, KnowledgeRequestDto } from '../../lib/bindings';

const requestFor = (conversationId: string, action: string): KnowledgeRequestDto => ({
  conversationId, action, itemId: null, text: null, scope: null, kind: null,
  validFrom: null, validUntil: null, offset: null,
});
const inputClass = 'w-full rounded-md border border-[hsl(var(--border-default))] bg-transparent p-2 text-xs';
const buttonClass = 'rounded-md border border-[hsl(var(--border-default))] px-2 py-1 text-xs disabled:opacity-50';

/** Shared facts retain their source conversation. Editing creates user evidence. */
export function KnowledgePanel({ conversationId, onOpenConversation }: { conversationId: string; onOpenConversation?: (id: string) => void }) {
  const queryClient = useQueryClient();
  const [offset, setOffset] = useState(0);
  const [text, setText] = useState('');
  const [scope, setScope] = useState('conversation');
  const [kind, setKind] = useState('user_fact');
  const [editing, setEditing] = useState<KnowledgeItemDto | null>(null);
  const [from, setFrom] = useState('');
  const [until, setUntil] = useState('');
  const query = useQuery({
    queryKey: ['conversationKnowledge', conversationId, offset],
    queryFn: async () => {
      const result = await VaultAPI.manageKnowledge({ ...requestFor(conversationId, 'list'), offset });
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    refetchInterval: 5000,
  });
  const mutation = useMutation({
    mutationFn: async (request: KnowledgeRequestDto) => {
      const result = await VaultAPI.manageKnowledge(request);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    onSuccess: async (_data, request) => {
      if (request.action === 'remember' || request.action === 'correct') {
        setText(''); setEditing(null); setFrom(''); setUntil('');
      }
      await queryClient.invalidateQueries({ queryKey: ['conversationKnowledge'] });
      await queryClient.invalidateQueries({ queryKey: ['conversationMemory'] });
      await queryClient.invalidateQueries({ queryKey: ['conversations'] });
    },
  });
  const act = (item: KnowledgeItemDto, action: string, extras: Partial<KnowledgeRequestDto> = {}) =>
    mutation.mutate({ ...requestFor(conversationId, action), itemId: item.id, ...extras });
  const date = (value: string) => value ? `${value}T00:00:00Z` : null;
  const beginEdit = (item: KnowledgeItemDto) => {
    setEditing(item); setText(item.label); setScope(item.scope);
    setFrom(item.validFrom?.slice(0, 10) ?? ''); setUntil(item.validUntil?.slice(0, 10) ?? '');
  };
  return (
    <section className="mt-5 space-y-3 border-t border-[hsl(var(--border-subtle))] pt-4">
      <h3 className="text-sm font-medium">Saved knowledge</h3>
      <p className="text-xs text-text-muted">Facts, decisions and open work you can reuse. Sharing with a space or all conversations is explicit. Dates describe when a fact applies; verification records when you checked it.</p>
      <form className="space-y-2" onSubmit={(event) => {
        event.preventDefault();
        mutation.mutate({ ...requestFor(conversationId, editing ? 'correct' : 'remember'), itemId: editing?.id ?? null, text, scope, kind, validFrom: date(from), validUntil: date(until) });
      }}>
        <label className="block text-xs">{editing ? 'Correction (saved as your exact words)' : 'Remember something'}
          <textarea aria-label="Memory text" className={`${inputClass} mt-1`} value={text} maxLength={8000} onChange={e => setText(e.target.value)} rows={3} />
        </label>
        {!editing && <label className="block text-xs">Memory type<select aria-label="Memory type" className={inputClass} value={kind} onChange={e => setKind(e.target.value)}>
          <option value="user_fact">Fact</option><option value="preference">Preference</option><option value="constraint">Requirement</option><option value="goal">Goal</option><option value="decision">Decision</option><option value="open_question">Open question</option>
        </select></label>}
        <label className="block text-xs">Available in
          <select aria-label="Memory scope" className={inputClass} value={scope} onChange={e => setScope(e.target.value)}>
            <option value="conversation">This conversation</option><option value="space">This space</option><option value="personal">All conversations</option>
          </select>
        </label>
        <div className="flex gap-2">
          <label className="flex-1 text-xs">Valid from<input aria-label="Valid from" type="date" className={inputClass} value={from} onChange={e => setFrom(e.target.value)} /></label>
          <label className="flex-1 text-xs">Valid until<input aria-label="Valid until" type="date" className={inputClass} value={until} onChange={e => setUntil(e.target.value)} /></label>
        </div>
        <button className={buttonClass} disabled={!text.trim() || mutation.isPending} type="submit">{editing ? 'Save correction' : 'Remember'}</button>
        {editing && <button className={`${buttonClass} ml-2`} type="button" onClick={() => {setEditing(null); setText(''); setFrom(''); setUntil('');}}>Cancel correction</button>}
      </form>
      {(query.error || mutation.error) && <p role="alert" className="text-xs text-[hsl(var(--error-fg))]">{(mutation.error ?? query.error)?.message}</p>}
      {query.isPending && <p className="text-xs">Loading saved knowledge…</p>}
      <ul className="space-y-3">
        {query.data?.items.map(item => {
          const owned = item.conversationId === conversationId;
          const expired = !!item.validUntil && new Date(item.validUntil).getTime() <= Date.now();
          return <li key={item.id} className="rounded-md border border-[hsl(var(--border-subtle))] p-3 text-xs">
            <div className="text-text-muted">{item.kind.replace(/_/g, ' ')} · {item.scope} · {item.forgotten ? 'forgotten' : item.state}{expired ? ' · expired' : ''}</div>
            <p className="mt-1 font-medium">{item.label}</p>
            {query.data?.lastAnswerMemoryIds.includes(item.id) && <p className="mt-1 text-text-muted">Included in the latest answer’s initial context</p>}
            <p className="mt-1 text-text-muted">{item.availabilityReason} · {item.conversationTitle}</p>
            <p className="mt-1 text-text-muted">Learned {item.learnedAt.slice(0, 10)}{item.validFrom ? ` · From ${item.validFrom.slice(0, 10)}` : ''}{item.validUntil ? ` · Until ${item.validUntil.slice(0, 10)}` : ''}{item.verifiedAt ? ` · Checked ${item.verifiedAt.slice(0, 10)}` : ' · Not manually verified'}</p>
            <details className="mt-2"><summary className="cursor-pointer">Evidence and provenance</summary>{item.evidence.map((e, i) => <blockquote key={`${e.messageId}-${i}`} className="mt-2 border-l-2 pl-2">{e.text ?? 'Source changed or was removed.'}<span className="block text-text-muted">{e.role} · message {e.sequence}</span></blockquote>)}</details>
            {owned && item.state === 'active' && !item.forgotten && <div className="mt-2 flex flex-wrap gap-2">
              <button className={buttonClass} disabled={mutation.isPending} onClick={() => beginEdit(item)}>Correct</button>
              <button className={buttonClass} disabled={mutation.isPending} onClick={() => act(item, 'verify')}>Mark checked</button>
              <select aria-label={`Sharing for ${item.label}`} className={buttonClass} value={item.scope} disabled={mutation.isPending} onChange={e => act(item, 'scope', {scope:e.target.value})}>
                <option value="conversation">This conversation</option><option value="space">This space</option><option value="personal">All conversations</option>
              </select>
              <button className={buttonClass} disabled={mutation.isPending} onClick={() => act(item, 'forget')}>Forget saved item</button>
            </div>}
            {!owned && <p className="mt-2 text-text-muted">Open its source conversation to change this item.
              {onOpenConversation && <button className={`${buttonClass} ml-2`} onClick={() => onOpenConversation(item.conversationId)}>Open source conversation</button>}
            </p>}
          </li>;
        })}
      </ul>
      <p className="text-xs text-text-muted">Forgetting stops using this saved item. Original conversation messages remain in history. Saved knowledge is evidence, not permission to take actions.</p>
      <div className="flex gap-2">
        {offset > 0 && <button className={buttonClass} onClick={() => setOffset(Math.max(0,offset-50))}>Previous</button>}
        {query.data?.hasMore && <button className={buttonClass} onClick={() => setOffset(offset+50)}>Next</button>}
      </div>
    </section>
  );
}

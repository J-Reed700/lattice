import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';

import { Check, ChevronRight } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { claimsCount, resolvedChip, teachingNotesForYou } from '@/features/learning/lessons/review/claimReviewCopy';
import { GROUP_MODE_LABELS, type ClaimGroup, type GroupMode } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { ClaimReviewItemDto } from '@/lib/bindings';

const NAVIGATION_KEYS = new Set(['ArrowDown', 'ArrowUp', 'Home', 'End']);

/** The id the screen focuses after an action: the row of the claim in focus. */
export const queueRowId = (claimId: string) => `claim-row-${claimId}`;

export const TEACHING_ROW = 'teaching-notes';

const shortPlace = (item: ClaimReviewItemDto) => item.presentation.place.split(' · ')[0] || item.presentation.place;

function Row({ item, active, opened, settled, tabIndex, onSelect }: {
  item: ClaimReviewItemDto; active: boolean; opened: boolean; settled: boolean; tabIndex: number; onSelect: () => void;
}) {
  return (
    <li>
      <button
        type="button" id={queueRowId(item.claimId)} data-queue-row="" tabIndex={tabIndex}
        aria-current={active ? 'true' : undefined} onClick={onSelect}
        className={`row-hover flex w-full items-start gap-2 rounded-lg px-2 py-2 text-left text-sm focus-visible:outline-2 focus-visible:outline-accent ${active ? 'bg-accent/10' : ''}`}
      >
        <span aria-hidden="true" className="mt-0.5 w-3 shrink-0 text-xs leading-5 text-accent">
          {settled ? <Check size={12} className="mt-1 text-success-fg" /> : opened ? '●' : '○'}
        </span>
        <span className={`min-w-0 flex-1 line-clamp-2 leading-5 ${settled ? 'text-text-muted' : 'text-text-primary'}`}>
          {item.presentation.statement}
          <span className="sr-only">{settled ? ` · ${resolvedChip(item)}` : opened ? '' : ' · not opened yet'}</span>
        </span>
        <span className="shrink-0 rounded-full border border-border px-1.5 text-[10px] leading-4 text-text-muted">{shortPlace(item)}</span>
      </button>
    </li>
  );
}

/**
 * The queue (§3.2): the waiting claims in groups with counts, the group of
 * the claim in focus open, bulk actions on a group's header, the teaching
 * notes last, and the claims resolved this session behind a toggle. Rows are
 * one tab stop; arrows, Home and End move between them.
 */
export function ClaimQueue({
  groups, mode, onModeChange, activeId, openedIds, settledIds, onSelect, onAcceptSource, onAddSource, onAcceptProposals,
  resolved, teachingWaiting, teachingActive, onOpenTeaching, busy,
}: {
  groups: ClaimGroup[];
  mode: GroupMode;
  onModeChange: (mode: GroupMode) => void;
  activeId: string | null;
  openedIds: ReadonlySet<string>;
  /** Claims resolved this session, still settling or under "Show resolved". */
  settledIds: ReadonlySet<string>;
  onSelect: (claimId: string) => void;
  onAcceptSource: (group: ClaimGroup) => void;
  onAddSource: (group: ClaimGroup) => void;
  /** Accepts every common-knowledge proposal of the lesson, in one action. */
  onAcceptProposals: (claims: ClaimReviewItemDto[]) => void;
  /** Claims that no longer wait, for "Show resolved". */
  resolved: ClaimReviewItemDto[];
  teachingWaiting: number;
  teachingActive: boolean;
  onOpenTeaching: () => void;
  busy: boolean;
}) {
  const id = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [showResolved, setShowResolved] = useState(false);
  const activeGroup = groups.find((group) => group.claims.some((claim) => claim.claimId === activeId))?.key;
  const rovingId = activeId ?? groups[0]?.claims[0]?.claimId ?? null;

  // The group of the claim in focus is always open.
  useEffect(() => {
    if (!activeGroup) return;
    setCollapsed((current) => {
      if (!current.has(activeGroup)) return current;
      const next = new Set(current);
      next.delete(activeGroup);
      return next;
    });
  }, [activeGroup]);

  const toggle = (key: string) => setCollapsed((current) => {
    const next = new Set(current);
    if (next.has(key)) next.delete(key); else next.add(key);
    return next;
  });

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!NAVIGATION_KEYS.has(event.key) || !(event.target instanceof HTMLElement) || !event.target.matches('[data-queue-row]')) return;
    const rows = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[data-queue-row]')];
    const index = rows.indexOf(event.target as HTMLButtonElement);
    const target = event.key === 'Home' ? 0 : event.key === 'End' ? rows.length - 1
      : Math.min(rows.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)));
    event.preventDefault();
    event.stopPropagation();
    rows[target]?.focus();
  };

  const total = groups.reduce((sum, group) => sum + group.claims.length, 0);
  const proposals = groups.flatMap((group) => group.claims).filter((claim) => claim.verdictKind?.kind === 'general_knowledge_proposed' && !settledIds.has(claim.claimId));
  return (
    <nav aria-label="Claims waiting for you" className="flex min-h-0 flex-col">
      <div className="flex flex-wrap items-center justify-between gap-2 px-1 pb-2">
        <label htmlFor={`${id}-mode`} className="flex items-center gap-2 text-xs text-text-secondary">
          Group by
          <select id={`${id}-mode`} value={mode} onChange={(event) => onModeChange(event.target.value as GroupMode)}
            className="min-h-8 rounded-md border border-border bg-surface px-2 text-xs text-text-primary">
            {(Object.keys(GROUP_MODE_LABELS) as GroupMode[]).map((key) => <option key={key} value={key}>{GROUP_MODE_LABELS[key]}</option>)}
          </select>
        </label>
        {total > 1 && <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => onAddSource({ key: 'queue', label: '', detail: null, claims: groups.flatMap((group) => group.claims), source: null })}>
          Add a source for these {total}
        </Button>}
        {proposals.length > 0 && <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => onAcceptProposals(proposals)}>
          Accept all {proposals.length} common-knowledge {proposals.length === 1 ? 'proposal' : 'proposals'}
        </Button>}
      </div>

      <div ref={listRef} onKeyDown={onKeyDown} className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
        {total === 0 && <p className="px-2 py-3 text-sm text-text-secondary">Everything is resolved.</p>}
        {groups.map((group) => {
          const open = !collapsed.has(group.key);
          const bodyId = `${id}-${group.key}`;
          return (
            <section key={group.key} aria-label={`${group.label} · ${claimsCount(group.claims.length)}`} className="rounded-xl border border-border bg-surface">
              <button type="button" aria-expanded={open} aria-controls={bodyId} onClick={() => toggle(group.key)}
                className="flex w-full items-start gap-2 px-3 py-2.5 text-left">
                <ChevronRight size={14} aria-hidden="true" className={`mt-0.5 shrink-0 text-text-muted transition-transform ${open ? 'rotate-90' : ''}`} />
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium leading-5 text-text-primary">{group.label}</span>
                  <span className="block text-xs leading-5 text-text-muted">{[group.detail, claimsCount(group.claims.length)].filter(Boolean).join(' · ')}</span>
                </span>
              </button>
              {open && <div id={bodyId} className="px-1.5 pb-2">
                {group.claims.length >= 2 && <div className="flex flex-wrap gap-1.5 px-1.5 pb-2">
                  {group.source && <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => onAcceptSource(group)}>Accept this source for all {group.claims.length}</Button>}
                  <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => onAddSource(group)}>Add a source for these {group.claims.length}</Button>
                </div>}
                <ul role="list" className="space-y-0.5">
                  {group.claims.map((item) => (
                    <Row key={item.claimId} item={item} active={item.claimId === activeId} opened={openedIds.has(item.claimId)} settled={settledIds.has(item.claimId)}
                      tabIndex={item.claimId === rovingId ? 0 : -1} onSelect={() => onSelect(item.claimId)} />
                  ))}
                </ul>
              </div>}
            </section>
          );
        })}

        {teachingWaiting > 0 && <section aria-label="Teaching notes" className="rounded-xl border border-border bg-surface">
          <button type="button" data-queue-row="" tabIndex={-1} aria-current={teachingActive ? 'true' : undefined} onClick={onOpenTeaching}
            className={`row-hover flex w-full items-center gap-2 rounded-xl px-3 py-2.5 text-left text-sm ${teachingActive ? 'bg-accent/10' : ''}`}>
            <ChevronRight size={14} aria-hidden="true" className="shrink-0 text-text-muted" />
            <span className="font-medium text-text-primary">Teaching notes</span>
            <span className="text-xs text-text-muted">· {teachingNotesForYou(teachingWaiting)}</span>
          </button>
        </section>}

        {resolved.length > 0 && <div className="pt-1">
          <Button type="button" size="sm" variant="ghost" aria-expanded={showResolved} onClick={() => setShowResolved((value) => !value)}>
            {showResolved ? 'Hide resolved' : `Show resolved (${resolved.length})`}
          </Button>
          {showResolved && <ul role="list" aria-label="Resolved claims" className="mt-1 space-y-0.5">
            {resolved.map((item) => (
              <li key={item.claimId}>
                <button type="button" data-queue-row="" tabIndex={-1} id={queueRowId(item.claimId)} aria-current={item.claimId === activeId ? 'true' : undefined} onClick={() => onSelect(item.claimId)}
                  className={`row-hover flex w-full items-start gap-2 rounded-lg px-2 py-2 text-left text-sm ${item.claimId === activeId ? 'bg-accent/10' : ''}`}>
                  <Check size={12} aria-hidden="true" className="mt-1 shrink-0 text-success-fg" />
                  <span className="min-w-0 flex-1 line-clamp-2 leading-5 text-text-secondary">{item.presentation.statement}</span>
                  <span className="shrink-0 text-[10px] leading-5 text-text-muted">{resolvedChip(item)}</span>
                </button>
              </li>
            ))}
          </ul>}
        </div>}
      </div>
    </nav>
  );
}

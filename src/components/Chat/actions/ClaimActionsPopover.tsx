import { useCallback, useEffect, useRef, useState } from 'react';

import { Columns3, Loader2, MessageCircleQuestion, NotebookPen, Quote } from 'lucide-react';
import { createPortal } from 'react-dom';
import { useNavigate } from 'react-router';

import { VaultAPI } from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { ClaimVerdict, SourceWithMetadata } from '@/types/conversation';
import {
  askWhyPrefill,
  claimToMarkdown,
  claimWithCitation,
  vaultDocumentIds,
} from '@/utils/conversationExport';

import { ACTION_HEADING_CLASS, ACTION_ROW_CLASS, ACTION_SURFACE_CLASS } from './actionSurface';

/**
 * What clicking a checked sentence offers.
 *
 * The hover card says whether a sentence is trusted; this says what to do
 * about it. Quoting a figure, filing it, or pushing back on it are the three
 * things a reader does next, and each one carries the citation with it so the
 * sentence never travels without its evidence.
 *
 * Interactive and keyboard-reachable, unlike the hover card: it is opened by a
 * click and closed by Escape, a click outside, or acting on it.
 */

export interface ClaimActionsPopoverProps {
  /** Viewport rect of the clicked line of the sentence. */
  anchor: DOMRect;
  verdict: ClaimVerdict;
  /** The answer's passages, keyed by the number the text cites them as. */
  citationMap: Map<number, SourceWithMetadata>;
  conversationId?: string | null;
  /** Right edge of the answer's text column; the panel stays left of it, clear of the evidence margin. */
  maxRight?: number;
  onClose: () => void;
}

const PANEL_WIDTH = 300;
const GAP = 8;
/** Below this much room above the line, the panel drops underneath it. */
const SPACE_FOR_ABOVE = 260;

function verdictHeading(verdict: ClaimVerdict): string {
  if (verdict.verdict === 'supported') return 'This sentence is backed by its source';
  if (verdict.verdict === 'contradicted') return 'Your sources say otherwise';
  return verdict.citationIds.length > 0
    ? 'Not found in the cited passage'
    : 'No source was cited for this';
}

export function ClaimActionsPopover({
  anchor,
  verdict,
  citationMap,
  conversationId = null,
  maxRight,
  onClose,
}: ClaimActionsPopoverProps) {
  const navigate = useNavigate();
  const panelRef = useRef<HTMLDivElement>(null);
  const [isSaving, setIsSaving] = useState(false);
  const conversationTitle = useConversationsStore(
    (state) => state.conversations.find((item) => item.id === conversationId)?.title ?? null,
  );

  // The first verb takes focus, so the panel is usable the moment it opens and
  // Escape has somewhere to return from.
  useEffect(() => {
    panelRef.current?.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus();
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
      const panel = panelRef.current;
      if (!panel) return;
      const items = Array.from(panel.querySelectorAll<HTMLButtonElement>('button:not([disabled])'));
      if (items.length === 0) return;
      event.preventDefault();
      const current = items.indexOf(document.activeElement as HTMLButtonElement);
      const step = event.key === 'ArrowDown' ? 1 : -1;
      const next = (current + step + items.length) % items.length;
      items[next]?.focus();
    };
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && panelRef.current?.contains(event.target)) return;
      onClose();
    };
    document.addEventListener('keydown', onKeyDown, true);
    document.addEventListener('pointerdown', onPointerDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      document.removeEventListener('pointerdown', onPointerDown, true);
    };
  }, [onClose]);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(claimWithCitation(verdict, citationMap));
      toast.success('Copied with its citation');
      onClose();
    } catch (error) {
      toast.error("Couldn't copy that sentence", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [citationMap, onClose, verdict]);

  const handleAddToJournal = useCallback(async () => {
    setIsSaving(true);
    try {
      const result = await VaultAPI.quickCapture(
        claimToMarkdown(verdict, citationMap, { conversationTitle }),
      );
      if (!result.ok) {
        toast.error('Could not write to the journal', { message: result.error });
        return;
      }
      const { noteId, noteTitle } = result.data;
      onClose();
      toast.success(`Saved to "${noteTitle}"`, {
        action: {
          label: 'Open',
          onClick: () => navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`),
        },
      });
    } catch (error) {
      toast.error('Could not write to the journal', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  }, [citationMap, conversationTitle, navigate, onClose, verdict]);

  const handleAskWhy = useCallback(() => {
    // `?quote=` is the composer prefill contract the reading surfaces already
    // use; ChatView turns it into a quoted draft and leaves the cursor below.
    const params = new URLSearchParams({ quote: askWhyPrefill(verdict) });
    onClose();
    navigate(`/chat?${params.toString()}`);
  }, [navigate, onClose, verdict]);

  const citedDocuments = vaultDocumentIds(
    verdict.citationIds.flatMap((id) => {
      const source = citationMap.get(id);
      return source ? [source] : [];
    }),
  );

  const rightLimit = Math.min(maxRight ?? window.innerWidth, window.innerWidth - 12);
  const left = Math.max(12, Math.min(anchor.left, rightLimit - PANEL_WIDTH));
  const placeAbove = anchor.top > SPACE_FOR_ABOVE;
  const style = placeAbove
    ? { left, bottom: window.innerHeight - anchor.top + GAP, width: PANEL_WIDTH }
    : { left, top: anchor.bottom + GAP, width: PANEL_WIDTH };

  return createPortal(
    <div
      ref={panelRef}
      role="dialog"
      aria-label="Actions for this sentence"
      style={style}
      className={`fixed animate-in fade-in-0 duration-fast ${ACTION_SURFACE_CLASS}`}
    >
      <p className={ACTION_HEADING_CLASS}>{verdictHeading(verdict)}</p>

      <button type="button" onClick={() => void handleCopy()} className={ACTION_ROW_CLASS}>
        <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
          <Quote className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-ui text-text-primary">Copy with citation</span>
          <span className="block text-xs leading-snug text-text-muted">
            The sentence and the file it came from
          </span>
        </span>
      </button>

      <button
        type="button"
        onClick={() => void handleAddToJournal()}
        disabled={isSaving}
        className={ACTION_ROW_CLASS}
      >
        <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
          {isSaving ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          ) : (
            <NotebookPen className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-ui text-text-primary">Add to journal</span>
          <span className="block text-xs leading-snug text-text-muted">
            With its evidence and this verdict
          </span>
        </span>
      </button>

      <button type="button" onClick={handleAskWhy} className={ACTION_ROW_CLASS}>
        <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
          <MessageCircleQuestion className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-ui text-text-primary">Ask why</span>
          <span className="block text-xs leading-snug text-text-muted">
            {verdict.verdict === 'supported'
              ? 'Put the question back, quoting this sentence'
              : 'Ask where in your documents this comes from'}
          </span>
        </span>
      </button>

      {citedDocuments.length >= 2 && (
        <button
          type="button"
          onClick={() => {
            onClose();
            navigate(
              `/compare?${new URLSearchParams({ ids: citedDocuments.join(',') }).toString()}`,
            );
          }}
          className={ACTION_ROW_CLASS}
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
            <Columns3 className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-ui text-text-primary">Compare its sources</span>
            <span className="block text-xs leading-snug text-text-muted">
              The {citedDocuments.length} documents this sentence cites
            </span>
          </span>
        </button>
      )}
    </div>,
    document.body,
  );
}

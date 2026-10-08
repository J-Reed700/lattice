import type { Ref } from 'react';

import { ArrowRight, Layers, Plus } from 'lucide-react';
import { useSearchParams } from 'react-router';

import { FlashcardGenerations } from '@/features/learning/recall/flashcards/FlashcardGenerations';
import { useFlashcardDecks, useFlashcardGenerations } from '@/features/learning/recall/flashcards/useFlashcards';

/**
 * Flashcards on the Studio landing page, below Programs.
 *
 * Every deck is listed, a program's recall deck included: it is the same deck
 * with the same review state as that program's Recall tab.
 */
export function FlashcardsSection({ ref }: { ref?: Ref<HTMLElement> }) {
  const [, setParams] = useSearchParams();
  const decks = useFlashcardDecks();
  const generations = useFlashcardGenerations();
  const generating = generations.some((item) => item.status === 'pending');

  return <section ref={ref} aria-labelledby="studio-flashcards-heading" className="mt-10 scroll-mt-6">
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-text-muted">Your memory</div><h2 id="studio-flashcards-heading" className="mt-1 font-serif text-2xl text-text-primary">Flashcards</h2></div>
      <div className="flex items-center gap-3">
        {decks.data?.length ? <span className="text-xs text-text-muted">{decks.data.length} {decks.data.length === 1 ? 'deck' : 'decks'}</span> : null}
        <button type="button" disabled={generating} onClick={() => setParams({ newDeck: '1' })} className="inline-flex items-center gap-1.5 rounded-full border border-border px-3.5 py-1.5 text-xs font-medium text-text-primary transition hover:border-accent/50 hover:text-accent disabled:opacity-50"><Plus size={14} />New deck</button>
      </div>
    </div>
    <div className="mt-5">
    <FlashcardGenerations />
    {decks.isPending ? <p role="status" className="text-sm text-text-muted">Loading decks…</p>
      : decks.isError ? <div role="alert" className="flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-rose-500/25 bg-surface p-6"><p className="text-sm text-text-secondary">{decks.error.message}</p><button type="button" onClick={() => void decks.refetch()} className="rounded-full border border-border px-4 py-2 text-sm">Retry</button></div>
        : decks.data.length ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{decks.data.map((deck) => <button key={deck.id} type="button" onClick={() => setParams({ deck: deck.id })} className="group flex flex-col rounded-2xl border border-border bg-surface p-5 text-left shadow-sm transition hover:border-accent/40 hover:shadow-md focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
          <span className="flex items-start justify-between gap-3"><span className="line-clamp-2 font-serif text-lg leading-snug text-text-primary">{deck.title}</span><ArrowRight size={15} className="mt-1 shrink-0 text-text-muted transition group-hover:translate-x-0.5 group-hover:text-accent" /></span>
          {deck.focus && <span className="mt-1.5 line-clamp-1 text-sm text-text-secondary">{deck.focus}</span>}
          <span className="mt-4 flex items-center gap-1.5 text-[11px] text-text-muted"><Layers size={13} />{deck.cardCount} cards · {deck.dueCount} due{deck.quizAttempts > 0 ? ` · ${Math.round(deck.quizCorrect / deck.quizAttempts * 100)}% correct` : ''}</span>
        </button>)}</div>
          : generations.length === 0 ? <div className="rounded-2xl border border-dashed border-border bg-surface p-6"><h3 className="font-serif text-xl text-text-primary">Put what you learn into practice.</h3><p className="mt-2 max-w-xl text-sm leading-6 text-text-secondary">Make a deck from library documents, or use the flashcard action on a conversation to turn its verified claims into cards. Every answer keeps its citations so you can check the evidence.</p></div> : null}
    </div>
  </section>;
}

import { useMemo, useState } from 'react';

import { useQueries } from '@tanstack/react-query';
import {
  Layers,
  Play,
  Shuffle,
  Sparkles,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  FlashcardSession,
  type FlashcardMode,
} from '@/features/learning/recall/flashcards/FlashcardSession';
import VaultAPI from '@/lib/api';
import type { StudyCardDto, StudyDeckSummaryDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

export interface InterleavedStudyPanelProps {
  decks: StudyDeckSummaryDto[];
}

function supportsQuiz(card: StudyCardDto) {
  return (card.format ?? 'multiple_choice') === 'multiple_choice' && card.options.length > 0;
}

export function InterleavedStudyPanel({ decks }: InterleavedStudyPanelProps) {
  const [selectedDeckIds, setSelectedDeckIds] = useState<string[]>(() =>
    decks.slice(0, 3).map((d) => d.id),
  );
  const [sessionMode, setSessionMode] = useState<FlashcardMode>('flashcard');
  const [onlyDueOrWeak, setOnlyDueOrWeak] = useState(false);
  const [inSession, setInSession] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);

  const isSessionActive = inSession;

  // Fetch full card data for each selected deck
  const deckQueries = useQueries({
    queries: selectedDeckIds.map((id) => ({
      queryKey: ['study', 'deck', id],
      queryFn: async () => {
        const res = await VaultAPI.getStudyDeck(id);
        if (!res.ok) throw new Error(res.error);
        return res.data;
      },
      enabled: Boolean(id) && (isExpanded || isSessionActive),
    })),
  });

  const isLoadingDecks = deckQueries.some((q) => q.isPending);

  // Collect candidate cards from loaded decks
  const candidateCards = useMemo(() => {
    const loadedDecks = deckQueries
      .map((q) => q.data)
      .filter((d): d is NonNullable<typeof d> => Boolean(d));

    const allCards = loadedDecks.flatMap((d) => d.cards);

    return allCards.filter((card) => {
      if (sessionMode === 'quiz' && !supportsQuiz(card)) {
        return false;
      }
      if (onlyDueOrWeak) {
        return card.dueAt <= Date.now() || card.lapses > 0;
      }
      return true;
    });
  }, [deckQueries, sessionMode, onlyDueOrWeak]);

  // Round-robin interleaving by topic
  const interleavedCards = useMemo(() => {
    if (candidateCards.length === 0) return [];

    const topicGroups = new Map<string, StudyCardDto[]>();
    for (const card of candidateCards) {
      const topicKey = card.topic?.trim() || card.deckId;
      const group = topicGroups.get(topicKey) ?? [];
      group.push(card);
      topicGroups.set(topicKey, group);
    }

    const groupArrays = Array.from(topicGroups.values()).map((cards) => [...cards]);
    const result: StudyCardDto[] = [];
    let remaining = candidateCards.length;

    while (remaining > 0) {
      for (const group of groupArrays) {
        if (group.length > 0) {
          result.push(group.shift()!);
          remaining--;
        }
      }
    }

    return result;
  }, [candidateCards]);

  const distinctTopicsCount = useMemo(() => {
    const topics = new Set(interleavedCards.map((c) => c.topic?.trim() || c.deckId));
    return topics.size;
  }, [interleavedCards]);

  const distinctDecksCount = useMemo(() => {
    const deckIds = new Set(interleavedCards.map((c) => c.deckId));
    return deckIds.size;
  }, [interleavedCards]);

  const toggleDeck = (id: string) => {
    setSelectedDeckIds((prev) =>
      prev.includes(id) ? prev.filter((dId) => dId !== id) : [...prev, id],
    );
  };

  const selectAll = () => {
    setSelectedDeckIds(decks.map((d) => d.id));
  };

  const clearSelection = () => {
    setSelectedDeckIds([]);
  };

  if (inSession && interleavedCards.length > 0) {
    return (
      <div className="rounded-2xl border border-accent/40 bg-surface p-6 shadow-sm">
        <div className="mb-6 flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle pb-4">
          <div className="inline-flex items-center gap-2 rounded-full border border-accent/30 bg-accent/10 px-3.5 py-1 text-xs font-semibold text-accent">
            <Shuffle size={13} />
            <span>
              Interleaved Topic Mix: {distinctTopicsCount} distinct {distinctTopicsCount === 1 ? 'topic' : 'topics'} · {distinctDecksCount} {distinctDecksCount === 1 ? 'deck' : 'decks'} mixed
            </span>
          </div>

          <button
            type="button"
            onClick={() => setInSession(false)}
            className="rounded-full border border-border-default px-3 py-1 text-xs text-text-muted hover:text-text-primary"
          >
            Exit session
          </button>
        </div>

        <FlashcardSession
          cards={interleavedCards}
          mode={sessionMode}
          title={`Interleaved Mix (${distinctTopicsCount} topics)`}
          onClose={() => setInSession(false)}
        />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-border-default bg-surface px-4 py-3 shadow-2xs flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent/10 text-accent">
            <Shuffle size={15} />
          </span>
          <div>
            <h3 className="font-serif text-sm font-semibold text-text-primary">
              Interleaved Topic Practice
            </h3>
            <p className="text-xs text-text-muted">
              Mix cards across {decks.length} decks to practice differentiating concepts
            </p>
          </div>
        </div>

        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-expanded={isExpanded}
          onClick={() => setIsExpanded((prev) => !prev)}
          className="inline-flex items-center gap-1.5 rounded-full text-xs"
        >
          <Shuffle size={12} />
          <span>{isExpanded ? 'Configure mix' : 'Open interleaved mixer'}</span>
        </Button>
      </div>

      {isExpanded && (
        <section
          aria-label="Interleaved Multi-Deck Study"
          className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6"
        >
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <div className="inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[.14em] text-accent">
                <Shuffle size={13} />
                <span>Interleaving · Cross-Topic Practice</span>
              </div>
              <h3 className="mt-1 font-serif text-xl font-medium text-text-primary">
                Interleaved Multi-Deck Study Session
              </h3>
            </div>

            <span className="inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-sunken px-3 py-1 text-xs font-medium text-text-secondary">
              <Layers size={12} className="text-accent" />
              <span>{decks.length} decks available</span>
            </span>
          </div>

          <p className="mt-2 text-xs leading-relaxed text-text-secondary">
            Mix up different sub-topics, decks, or problem types during a single study session rather than blocking hours for one topic. This forces your brain to constantly identify and differentiate concepts.
          </p>

          {/* Deck Selection & Options */}
          <div className="mt-5 rounded-xl border border-border-default bg-surface-sunken/40 p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-text-primary">Select decks to mix:</span>
                <button
                  type="button"
                  onClick={selectAll}
                  className="text-[11px] text-accent underline hover:text-accent-fg"
                >
                  Mix all decks
                </button>
                <span className="text-text-muted">·</span>
                <button
                  type="button"
                  onClick={clearSelection}
                  className="text-[11px] text-text-muted underline hover:text-text-primary"
                >
                  Clear
                </button>
              </div>

              <div className="flex flex-wrap items-center gap-3">
                {/* Mode selection */}
                <div className="inline-flex rounded-lg border border-border-default bg-surface p-0.5 text-xs">
                  <button
                    type="button"
                    onClick={() => setSessionMode('flashcard')}
                    className={cn(
                      'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
                      sessionMode === 'flashcard'
                        ? 'bg-accent text-accent-fg'
                        : 'text-text-secondary hover:text-text-primary',
                    )}
                  >
                    Flashcards
                  </button>
                  <button
                    type="button"
                    onClick={() => setSessionMode('quiz')}
                    className={cn(
                      'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
                      sessionMode === 'quiz'
                        ? 'bg-accent text-accent-fg'
                        : 'text-text-secondary hover:text-text-primary',
                    )}
                  >
                    Practice Quiz
                  </button>
                </div>

                {/* Filter Toggle */}
                <label className="flex cursor-pointer items-center gap-1.5 text-xs text-text-secondary">
                  <input
                    type="checkbox"
                    checked={onlyDueOrWeak}
                    onChange={(e) => setOnlyDueOrWeak(e.target.checked)}
                    className="rounded accent-accent"
                  />
                  <span>Due &amp; weak cards only</span>
                </label>
              </div>
            </div>

            {/* Deck List Checkboxes */}
            <div className="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {decks.map((deck) => {
                const isSelected = selectedDeckIds.includes(deck.id);
                return (
                  <label
                    key={deck.id}
                    className={cn(
                      'flex cursor-pointer items-start gap-2.5 rounded-lg border p-3 text-left transition-colors',
                      isSelected
                        ? 'border-accent/60 bg-accent/5'
                        : 'border-border-default bg-surface hover:border-border-strong',
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleDeck(deck.id)}
                      className="mt-0.5 accent-accent"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-xs font-medium text-text-primary">{deck.title}</span>
                      <span className="mt-0.5 block truncate text-[10px] text-text-muted">
                        {deck.cardCount} cards · {deck.dueCount} due
                        {deck.focus ? ` · ${deck.focus}` : ''}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          </div>

          {/* Launch Footer */}
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              {selectedDeckIds.length >= 2 ? (
                <span className="inline-flex items-center gap-1.5 rounded-full border border-accent/20 bg-accent/10 px-3 py-1 text-xs font-medium text-accent">
                  <Sparkles size={12} />
                  <span>
                    {interleavedCards.length} {sessionMode === 'quiz' ? 'quiz questions' : 'cards'} across {distinctTopicsCount} {distinctTopicsCount === 1 ? 'topic' : 'topics'} ready to interleave
                  </span>
                </span>
              ) : (
                <span className="text-xs text-text-muted">
                  Select 2 or more decks to enable interleaved practice.
                </span>
              )}
            </div>

            <Button
              disabled={selectedDeckIds.length < 2 || interleavedCards.length === 0 || isLoadingDecks}
              onClick={() => setInSession(true)}
              className="inline-flex items-center gap-1.5 rounded-full"
            >
              <Play size={13} />
              <span>
                {isLoadingDecks
                  ? 'Loading cards…'
                  : `Start Interleaved Session (${interleavedCards.length})`}
              </span>
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}

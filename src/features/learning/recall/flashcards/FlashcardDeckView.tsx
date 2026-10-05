import { useEffect, useState } from 'react';

import { ArrowLeft } from 'lucide-react';
import { useSearchParams } from 'react-router';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { PageHeader, SectionHeading } from '@/components/ui/PageHeader';
import { EditFlashcard } from '@/features/learning/recall/flashcards/EditFlashcard';
import { FlashcardSession, type FlashcardMode } from '@/features/learning/recall/flashcards/FlashcardSession';
import { FlashcardSource } from '@/features/learning/recall/flashcards/FlashcardSource';
import { NewFlashcardDeck } from '@/features/learning/recall/flashcards/NewFlashcardDeck';
import { useDeleteFlashcardDeck, useFlashcardDeck } from '@/features/learning/recall/flashcards/useFlashcards';
import type { StudyCardDto } from '@/lib/bindings';

interface Session { id: string; cards: StudyCardDto[]; mode: FlashcardMode }
function shuffled<T>(items: T[]): T[] {
  const result = [...items];
  for (let i = result.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [result[i], result[j]] = [result[j], result[i]]; }
  return result;
}

function supportsQuiz(card: StudyCardDto) {
  return (card.format ?? 'multiple_choice') === 'multiple_choice' && card.options.length > 0;
}

/**
 * One flashcard deck inside Studio, addressed by `/studio?deck=<id>` so a
 * link from a conversation opens it directly. `?newDeck=1` shows the form
 * that generates one from library documents.
 */
export function FlashcardDeckView() {
  const [params, setParams] = useSearchParams();
  const id = params.get('deck');
  const creating = params.get('newDeck') === '1';
  const deck = useFlashcardDeck(id);
  const remove = useDeleteFlashcardDeck();
  const [session, setSession] = useState<Session | null>(null);
  const [editing, setEditing] = useState<StudyCardDto | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const current = deck.data;
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  const due = current?.cards.filter(card => card.dueAt <= now).sort((a, b) => a.dueAt - b.dueAt) ?? [];
  const quizCards = current?.cards.filter(supportsQuiz) ?? [];
  const weakCards = current?.cards.filter(card => card.lapses > 0).sort((a, b) => b.lapses - a.lapses) ?? [];
  const weakQuizCards = weakCards.filter(supportsQuiz);
  const weakTopics = [...new Set(weakCards.map(card => card.topic))].slice(0, 5);
  const start = (cards: StudyCardDto[], mode: FlashcardMode) => setSession({ id: crypto.randomUUID(), cards, mode });
  const back = () => { setSession(null); setParams({}); };
  const backButton = <Button variant="ghost" className="mb-5 -ml-3 gap-2" onClick={back}><ArrowLeft className="h-4 w-4" />Studio</Button>;

  let content;
  if (creating) content = <NewFlashcardDeck onCreated={deckId => setParams({ deck: deckId })} onCancel={back} />;
  else if (session && current) content = <FlashcardSession key={session.id} title={current.title} cards={session.cards} mode={session.mode} onClose={() => setSession(null)} />;
  else content = <>
    {backButton}
    {deck.isPending && <p role="status" className="text-sm text-text-muted">Loading deck…</p>}
    {deck.isError && <div role="alert"><p className="text-sm text-danger-fg">{deck.error.message}</p><Button variant="secondary" className="mt-4" onClick={() => void deck.refetch()}>Retry</Button></div>}
    {current && <>
      <PageHeader title={current.title} meta={`${current.cards.length} cards · ${due.length} due${current.focus ? ` · ${current.focus}` : ''}`} />
      {current.studyGoal && <p className="mb-6 text-sm text-text-secondary">Learning goal · {current.studyGoal}</p>}
      <div className="mb-8 flex flex-wrap gap-2">
        <Button disabled={due.length === 0} onClick={() => start(due, 'flashcard')}>Review due{due.length > 0 ? ` (${due.length})` : ''}</Button>
        <Button variant="secondary" disabled={!quizCards.length} title={quizCards.length ? undefined : 'This deck has no multiple-choice cards'} onClick={() => start(shuffled(quizCards), 'quiz')}>Practice quiz</Button>
        <Button variant="ghost" disabled={!current.cards.length} onClick={() => start(current.cards, 'flashcard')}>Study all</Button>
      </div>
      {due.length === 0 && current.cards.length > 0 && <p className="mb-8 text-sm text-text-muted">Next review {new Date(Math.min(...current.cards.map(card => card.dueAt))).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}.</p>}
      {weakTopics.length > 0 && <section className="mb-8"><SectionHeading>Topics to revisit</SectionHeading><p className="text-sm leading-relaxed text-text-secondary">{weakTopics.join(' · ')}</p><div className="mt-3 flex flex-wrap gap-2"><Button variant="ghost" size="sm" className="-ml-3" onClick={() => start(shuffled(weakCards), 'flashcard')}>Review missed cards</Button>{weakQuizCards.length > 0 && <Button variant="ghost" size="sm" onClick={() => start(shuffled(weakQuizCards), 'quiz')}>Practice missed quiz cards</Button>}</div></section>}
      <SectionHeading>Cards</SectionHeading>
      <div className="border-t border-border-subtle">
        {current.cards.map((card, index) => <details key={card.id} className="border-b border-border-subtle py-4">
          <summary className="cursor-pointer text-sm leading-relaxed text-text-primary"><span className="mr-3 text-text-muted">{index + 1}.</span>{' '}{card.question}</summary>
          <div className="ml-6 mt-4"><p className="mb-3 text-sm font-medium text-text-primary">{card.answer}</p><p className="whitespace-pre-wrap text-sm leading-relaxed text-text-secondary">{card.explanation}</p><FlashcardSource sources={card.citations?.length ? card.citations : [card.source]} /><div className="mt-4 flex items-center justify-between gap-3"><span className="text-xs text-text-muted">{card.reviewCount} reviews{card.lapses > 0 ? ` · ${card.lapses} missed` : ''}</span><Button variant="ghost" size="sm" onClick={() => setEditing(card)}>Edit card</Button></div></div>
        </details>)}
      </div>
      <div className="mt-8 flex items-center justify-between gap-4"><p className="text-xs text-text-muted">Generated with {current.modelName}</p><Button variant="ghost" size="sm" onClick={() => setConfirmDelete(true)}>Delete deck</Button></div>
    </>}
  </>;

  return <main className="h-full overflow-y-auto bg-background"><div className="mx-auto w-full max-w-[800px] px-6 pb-16 pt-10">
    {content}
    {editing && <EditFlashcard card={editing} onClose={() => setEditing(null)} />}
    <Dialog open={confirmDelete} onOpenChange={open => { if (!remove.isPending) setConfirmDelete(open); }}><DialogContent><DialogTitle>Delete {current?.title}?</DialogTitle><DialogDescription>This removes the deck, its cards, and its review history. Your source documents stay in the library.</DialogDescription>{remove.isError && <p role="alert" className="text-sm text-danger-fg">{remove.error.message}</p>}<div className="flex justify-end gap-2"><Button variant="ghost" disabled={remove.isPending} onClick={() => setConfirmDelete(false)}>Cancel</Button><Button variant="destructive" disabled={remove.isPending} onClick={() => { if (id) remove.mutate(id, { onSuccess: () => { setConfirmDelete(false); back(); } }); }}>{remove.isPending ? 'Deleting…' : 'Delete deck'}</Button></div></DialogContent></Dialog>
  </div></main>;
}

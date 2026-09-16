import { useEffect, useState } from 'react';

import { ArrowLeft, Plus } from 'lucide-react';
import { useSearchParams } from 'react-router';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { PageHeader, SectionHeading } from '@/components/ui/PageHeader';
import type { StudyCardDto } from '@/lib/bindings';

import { EditStudyCard } from './EditStudyCard';
import { NewStudyDeck } from './NewStudyDeck';
import { StudyGenerations } from './StudyGenerations';
import { StudySession, type StudyMode } from './StudySession';
import { StudySource } from './StudySource';
import { useDeleteStudyDeck, useStudyDeck, useStudyDecks, useStudyGenerations } from './useStudy';

interface Session { id: string; cards: StudyCardDto[]; mode: StudyMode }
function shuffled<T>(items: T[]): T[] {
  const result = [...items];
  for (let i = result.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [result[i], result[j]] = [result[j], result[i]]; }
  return result;
}

export function StudyPage() {
  const [params, setParams] = useSearchParams();
  const id = params.get('deck');
  const creating = params.get('new') === '1';
  const decks = useStudyDecks();
  const generations = useStudyGenerations();
  const generating = generations.some(item => item.status === 'pending');
  const deck = useStudyDeck(id);
  const remove = useDeleteStudyDeck();
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
  const weakTopics = [...new Set(current?.cards.filter(card => card.lapses > 0).sort((a, b) => b.lapses - a.lapses).map(card => card.topic) ?? [])].slice(0, 5);
  const start = (cards: StudyCardDto[], mode: StudyMode) => setSession({ id: crypto.randomUUID(), cards, mode });
  const back = () => { setSession(null); setParams({}); };

  let content;
  if (creating) content = <NewStudyDeck onCreated={deckId => setParams({ deck: deckId })} onCancel={back} />;
  else if (session && current) content = <StudySession key={session.id} title={current.title} cards={session.cards} mode={session.mode} onClose={() => setSession(null)} />;
  else if (id) content = <>
    <Button variant="ghost" className="mb-5 -ml-3 gap-2" onClick={back}><ArrowLeft className="h-4 w-4" />All decks</Button>
    {deck.isPending && <p role="status" className="text-sm text-text-muted">Loading deck…</p>}
    {deck.isError && <div role="alert"><p className="text-sm text-danger-fg">{deck.error.message}</p><Button variant="secondary" className="mt-4" onClick={() => void deck.refetch()}>Retry</Button></div>}
    {current && <>
      <PageHeader title={current.title} meta={`${current.cards.length} cards · ${due.length} due${current.focus ? ` · ${current.focus}` : ''}`} />
      {current.studyGoal && <p className="mb-6 text-sm text-text-secondary">Learning goal · {current.studyGoal}</p>}
      <div className="mb-8 flex flex-wrap gap-2">
        <Button disabled={due.length === 0} onClick={() => start(due, 'flashcard')}>Review due{due.length > 0 ? ` (${due.length})` : ''}</Button>
        <Button variant="secondary" disabled={!current.cards.length} onClick={() => start(shuffled(current.cards), 'quiz')}>Practice quiz</Button>
        <Button variant="ghost" disabled={!current.cards.length} onClick={() => start(current.cards, 'flashcard')}>Study all</Button>
      </div>
      {due.length === 0 && current.cards.length > 0 && <p className="mb-8 text-sm text-text-muted">Next review {new Date(Math.min(...current.cards.map(card => card.dueAt))).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}.</p>}
      {weakTopics.length > 0 && <section className="mb-8"><SectionHeading>Topics to revisit</SectionHeading><p className="text-sm leading-relaxed text-text-secondary">{weakTopics.join(' · ')}</p><Button variant="ghost" size="sm" className="mt-3 -ml-3" onClick={() => start(shuffled(current.cards.filter(card => card.lapses > 0)), 'quiz')}>Practice missed cards</Button></section>}
      <SectionHeading>Cards</SectionHeading>
      <div className="border-t border-border-subtle">
        {current.cards.map((card, index) => <details key={card.id} className="border-b border-border-subtle py-4">
          <summary className="cursor-pointer text-sm leading-relaxed text-text-primary"><span className="mr-3 text-text-muted">{index + 1}.</span>{' '}{card.question}</summary>
          <div className="ml-6 mt-4"><p className="mb-3 text-sm font-medium text-text-primary">{card.answer}</p><p className="whitespace-pre-wrap text-sm leading-relaxed text-text-secondary">{card.explanation}</p><StudySource sources={card.citations?.length ? card.citations : [card.source]} /><div className="mt-4 flex items-center justify-between gap-3"><span className="text-xs text-text-muted">{card.reviewCount} reviews{card.lapses > 0 ? ` · ${card.lapses} missed` : ''}</span><Button variant="ghost" size="sm" onClick={() => setEditing(card)}>Edit card</Button></div></div>
        </details>)}
      </div>
      <div className="mt-8 flex items-center justify-between gap-4"><p className="text-xs text-text-muted">Generated with {current.modelName}</p><Button variant="ghost" size="sm" onClick={() => setConfirmDelete(true)}>Delete deck</Button></div>
    </>}
  </>;
  else content = <>
    <PageHeader title="Study" meta={decks.data?.length ? `${decks.data.length} decks` : undefined} actions={<Button className="gap-2" disabled={generating} onClick={() => setParams({ new: '1' })}><Plus className="h-4 w-4" />New deck</Button>} />
    <StudyGenerations />
    {decks.isPending && <p role="status" className="text-sm text-text-muted">Loading decks…</p>}
    {decks.isError && <div role="alert"><p className="text-sm text-danger-fg">{decks.error.message}</p><Button variant="secondary" className="mt-4" onClick={() => void decks.refetch()}>Retry</Button></div>}
    {decks.data?.length === 0 && generations.length === 0 && <div className="py-8"><h2 className="mb-3 font-serif text-2xl text-text-primary">Put what you learn into practice.</h2><p className="max-w-lg text-sm leading-relaxed text-text-secondary">Choose documents here, or use the flashcard action on a conversation to turn all of its verified claims into a deck. Every answer keeps its citations so you can check the evidence.</p></div>}
    {Boolean(decks.data?.length) && <><SectionHeading>Your decks</SectionHeading><div className="border-t border-border-subtle">{decks.data?.map(d => <button key={d.id} className="block w-full border-b border-border-subtle py-5 text-left transition-colors duration-fast hover:bg-surface-raised" onClick={() => setParams({ deck: d.id })}>
      <span className="block text-base font-medium text-text-primary">{d.title}</span>
      <span className="mt-2 block text-xs text-text-muted">{d.cardCount} cards · {d.dueCount} due{d.quizAttempts > 0 ? ` · ${Math.round(d.quizCorrect / d.quizAttempts * 100)}% correct across ${d.quizAttempts} practice answers` : ''}</span>
      {d.focus && <span className="mt-2 block text-sm text-text-secondary">{d.focus}</span>}
    </button>)}</div></>}
  </>;

  return <main className="h-full overflow-y-auto bg-bg"><div className="mx-auto w-full max-w-[800px] px-6 pb-16 pt-10">
    {content}
    {editing && <EditStudyCard card={editing} onClose={() => setEditing(null)} />}
    <Dialog open={confirmDelete} onOpenChange={open => { if (!remove.isPending) setConfirmDelete(open); }}><DialogContent><DialogTitle>Delete {current?.title}?</DialogTitle><DialogDescription>This removes the deck, its cards, and its review history. Your source documents stay in the library.</DialogDescription>{remove.isError && <p role="alert" className="text-sm text-danger-fg">{remove.error.message}</p>}<div className="flex justify-end gap-2"><Button variant="ghost" disabled={remove.isPending} onClick={() => setConfirmDelete(false)}>Cancel</Button><Button variant="destructive" disabled={remove.isPending} onClick={() => { if (id) remove.mutate(id, { onSuccess: () => { setConfirmDelete(false); back(); } }); }}>{remove.isPending ? 'Deleting…' : 'Delete deck'}</Button></div></DialogContent></Dialog>
  </div></main>;
}

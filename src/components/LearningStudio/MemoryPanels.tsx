import { useEffect, useRef, useState, type FormEvent } from 'react';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { BookOpen, Check, ExternalLink, FileText, LoaderCircle, PencilLine, RotateCcw, Save, Sparkles, Trash2, Undo2 } from 'lucide-react';
import { useNavigate } from 'react-router';

import { TiptapEditor } from '@/components/TiptapEditor';
import { settingsFieldClass } from '@/components/ui/SettingsSection';
import VaultAPI from '@/lib/api';
import type { LearningCardDraftDto, LearningCardOriginDto, LearningMemoryDto, LearningProgramDto, StudyCardDto, WorkspaceNoteDto } from '@/lib/bindings';
import { flushPendingSaves, registerPendingSave } from '@/lib/pendingSaves';

import { learningMemoryKey, useAcceptLearningCardDraft, useDiscardLearningCardDraft, useEnsureLearningLessonNote, useGenerateLearningCardDrafts, useSaveLearningCardDraft, useUpdateLearningStudyCard, useReviewLearningStudyCard } from './useLearningMemory';

function unwrap<T>(result: { ok: true; data: T } | { ok: false; error: string }): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

function safeHref(value?: string | null) {
  if (!value) return undefined;
  try { const url = new URL(value); return ['http:', 'https:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}

function SourceProvenance({ sourceIds, program }: { sourceIds: string[]; program: LearningProgramDto }) {
  if (!sourceIds.length) return <p className="mt-3 text-xs text-text-muted">Personal draft · no source selected</p>;
  const sources = sourceIds.map((id) => program.sources.find((source) => source.id === id)).filter((source) => source !== undefined);
  return <div className="mt-3 min-w-0 space-y-2 [overflow-wrap:anywhere]">{sources.map((source) => { const href = safeHref(source.url); return <details key={source.id} className="min-w-0 rounded-lg border border-border bg-background"><summary aria-label={`Toggle source excerpt for ${source.title}`} className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-xs text-text-secondary"><FileText size={13} className="shrink-0 text-accent" /><span className="min-w-0 flex-1 truncate">{source.title}</span><span className="hidden shrink-0 text-text-muted sm:inline">Source excerpt</span></summary><div className="min-w-0 border-t border-border px-3 py-2.5"><p className="whitespace-pre-wrap text-xs leading-5 text-text-secondary">{source.excerpt}</p><p className="mt-2 text-[10px] text-text-muted">Acquired {new Date(source.acquiredAt).toLocaleString()}{href && <> · <a href={href} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-accent hover:underline">Open reference <ExternalLink size={10} /></a></>}</p></div></details>; })}{!sources.length && <p className="text-xs text-text-muted">The source snapshot for this draft is not available.</p>}</div>;
}

type NoteDraft = { title: string; content: string };

export function NotebookPanel({ enabled, program, lessonId, memory, memoryLoading, memoryError, onRetryMemory, drafts, setDraft }: {
  program: LearningProgramDto; lessonId?: string; memory: LearningMemoryDto | undefined; memoryLoading: boolean; memoryError?: string;
  onRetryMemory: () => void; drafts: Record<string, NoteDraft>; setDraft: (key: string, draft: NoteDraft) => void; enabled: boolean;
}) {
  const navigate = useNavigate();
  const ensure = useEnsureLearningLessonNote();
  const client = useQueryClient();
  const saveQueue = useRef<Promise<unknown>>(Promise.resolve());
  const startedEnsure = useRef(new Set<string>());
  const latest = useRef({ memory, drafts });
  const flushAllRef = useRef<() => Promise<boolean>>(async () => true);
  latest.current = { memory, drafts };
  const noteItem = memory?.lessonNotes.find((item) => item.lessonId === lessonId);
  const note = noteItem?.note;
  const key = `${program.summary.id}:${lessonId ?? 'none'}`;
  const draft = note ? drafts[key] ?? { title: note.title, content: note.content } : undefined;
  const title = draft?.title ?? '';
  const content = draft?.content ?? '';

  const save = useMutation({
    mutationFn: (snapshot: WorkspaceNoteDto) => {
      const queued = saveQueue.current.then(async () => unwrap(await VaultAPI.updateWorkspaceNote(snapshot)));
      saveQueue.current = queued.then(() => undefined, () => undefined);
      return queued;
    },
    onSuccess: (updated) => {
      client.setQueryData<LearningMemoryDto>(learningMemoryKey(program.summary.id), (current) => current && ({
        ...current,
        lessonNotes: current.lessonNotes.map((item) => item.note.id === updated.id ? { ...item, note: updated } : item),
      }));
    },
  });

  useEffect(() => {
    if (!enabled || !lessonId || program.summary.status !== 'active' || !memory || note || memoryLoading) return;
    if (startedEnsure.current.has(key)) return;
    startedEnsure.current.add(key);
    ensure.mutate({ programId: program.summary.id, lessonId });
  }, [enabled, ensure, key, lessonId, memory, memoryLoading, note, program.summary.id, program.summary.status]);

  const dirty = Boolean(note && (title !== note.title || content !== note.content));
  const titleMissing = Boolean(note && !title.trim());
  const saveCurrent = () => {
    if (!note || !dirty || !title.trim()) return;
    save.mutate({ ...note, title: title.trim(), content });
  };

  flushAllRef.current = async () => {
    const latestMemory = latest.current.memory;
    if (!latestMemory) return true;
    for (const item of latestMemory.lessonNotes) {
      const draftKey = `${program.summary.id}:${item.lessonId}`;
      const candidate = latest.current.drafts[draftKey] ?? { title: item.note.title, content: item.note.content };
      if (candidate.title === item.note.title && candidate.content === item.note.content) continue;
      if (!candidate.title.trim()) return false;
      try {
        await save.mutateAsync({ ...item.note, title: candidate.title.trim(), content: candidate.content });
      } catch {
        return false;
      }
    }
    return true;
  };

  useEffect(() => registerPendingSave(() => flushAllRef.current()), []);

  useEffect(() => {
    if (!dirty || !note || !title.trim()) return;
    const timer = window.setTimeout(() => save.mutate({ ...note, title: title.trim(), content }), 750);
    return () => window.clearTimeout(timer);
  }, [dirty, note, title, content, save]);

  const retryEnsure = () => {
    if (!lessonId) return;
    startedEnsure.current.add(key);
    ensure.mutate({ programId: program.summary.id, lessonId });
  };

  if (program.summary.status === 'draft') return <div className="rounded-2xl border border-dashed border-border bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Notebook opens with an accepted program</h3><p className="mt-2 max-w-xl text-sm leading-6 text-text-secondary">Accept this outline first. Studio will then create one canonical, lesson-linked Journal page when you open a lesson’s notebook.</p></div>;
  if (!lessonId) return <div className="rounded-2xl border border-dashed border-border bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Choose a lesson for your notebook</h3><p className="mt-2 text-sm text-text-secondary">Select a lesson in the Lessons view to open its linked writing page.</p></div>;
  if (memoryLoading && !memory) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle size={15} className="mr-2 inline animate-spin" />Opening lesson notebook…</div>;
  if (memoryError && !memory) return <div className="rounded-2xl border border-rose-500/25 bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Notebook could not load</h3><p role="alert" className="mt-2 text-sm text-rose-700">{memoryError}</p><button type="button" onClick={onRetryMemory} className="mt-4 rounded-full border border-border px-4 py-2 text-sm">Retry</button></div>;
  if (ensure.isError && !note) return <div className="rounded-2xl border border-rose-500/25 bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Lesson page could not be created</h3><p role="alert" className="mt-2 text-sm text-rose-700">{ensure.error.message}</p><button type="button" onClick={retryEnsure} className="mt-4 inline-flex items-center gap-2 rounded-full border border-border px-4 py-2 text-sm"><RotateCcw size={14} /> Retry</button></div>;
  if (!note) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle size={15} className="mr-2 inline animate-spin" />Creating your lesson page…</div>;

  const query = new URLSearchParams({ noteId: note.id });
  if (memory?.journalId) query.set('journalSpaceId', memory.journalId);
  const journalHref = `/journals?${query.toString()}`;
  const openJournal = async () => {
    const flushed = await flushPendingSaves();
    if (flushed) navigate(journalHref);
  };
  return <section className="overflow-hidden rounded-2xl border border-border/70 bg-surface shadow-sm">
    <header className="flex flex-wrap items-center justify-between gap-4 border-b border-border px-6 py-5"><div><div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.16em] text-accent"><BookOpen size={14} /> Lesson notebook</div><h3 className="mt-1 font-serif text-xl text-text-primary">A place to make the idea your own</h3></div><button type="button" disabled={titleMissing} onClick={() => void openJournal()} className="inline-flex items-center gap-2 rounded-full border border-border px-3.5 py-2 text-xs font-medium text-text-secondary hover:border-accent/40 hover:text-accent disabled:cursor-not-allowed disabled:opacity-50">Open full Journal <ExternalLink size={13} /></button></header>
    <div className="px-6 py-6 sm:px-8"><label htmlFor={`note-title-${lessonId}`} className="sr-only">Notebook page title</label><input id={`note-title-${lessonId}`} value={title} onChange={(event) => setDraft(key, { title: event.target.value, content })} className="w-full border-b border-border bg-transparent pb-3 font-serif text-2xl text-text-primary outline-none focus:border-accent" /><label className="mb-2 mt-6 block text-xs font-semibold uppercase tracking-[.12em] text-text-muted">Your notes</label><div className="min-h-[280px] rounded-xl border border-border bg-background/70 px-4 py-3 focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/10"><TiptapEditor value={content} onChange={(markdown) => setDraft(key, { title, content: markdown })} ariaLabel="Your notes" placeholder="What stands out? What do you want to remember, question, or explain in your own words?" className="min-h-[250px] text-sm leading-7 text-text-primary" /></div>
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3"><div className="text-xs">{titleMissing ? <span role="alert" className="text-amber-700">Add a page title before saving or opening Journal.</span> : save.isPending ? <span role="status" className="text-text-muted"><LoaderCircle size={13} className="mr-1 inline animate-spin" />Saving…</span> : save.isError ? <span role="alert" className="text-rose-700">Could not save: {save.error.message}</span> : dirty ? <span className="text-text-muted">Unsaved changes · saves after you pause</span> : <span role="status" className="text-text-muted"><Check size={13} className="mr-1 inline text-emerald-700" />Saved to Journal</span>}</div>{save.isError && !titleMissing && <button type="button" onClick={saveCurrent} className="inline-flex items-center gap-2 rounded-full border border-border px-3 py-2 text-xs"><RotateCcw size={13} /> Retry save</button>}<p className="ml-auto text-[10px] text-text-muted">Lesson-linked canonical Journal page</p></div>
    </div>
  </section>;
}

function DraftEditor({ draftId, lessonId, program, initial, origin = 'manual', pending, error, onSave, onCancel }: {
  draftId: string | null; lessonId: string; program: LearningProgramDto; initial?: Pick<LearningCardDraftDto, 'question' | 'answer' | 'explanation' | 'sourceIds'>;
  origin?: 'generated' | 'manual';
  pending: boolean; error?: string; onSave: (value: Omit<NonNullable<typeof initial>, 'id'>) => void; onCancel?: () => void;
}) {
  const eligibleSources = origin === 'generated'
    ? program.sources.filter((source) => program.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === lessonId)?.blocks.some((block) => block.sourceIds.includes(source.id)))
    : program.sources;
  const eligibleSourceIds = new Set(eligibleSources.map((source) => source.id));
  const [question, setQuestion] = useState(initial?.question ?? '');
  const [answer, setAnswer] = useState(initial?.answer ?? '');
  const [explanation, setExplanation] = useState(initial?.explanation ?? '');
  const [sourceIds, setSourceIds] = useState<string[]>(initial?.sourceIds.filter((id) => eligibleSourceIds.has(id)) ?? []);
  const submit = (event: FormEvent) => { event.preventDefault(); onSave({ question: question.trim(), answer: answer.trim(), explanation: explanation.trim(), sourceIds }); };
  return <form onSubmit={submit} className="rounded-xl border border-border/70 bg-background p-4 sm:p-5">
    <label htmlFor={`draft-question-${draftId ?? 'manual'}-${lessonId}`} className="mb-1.5 block text-xs font-semibold text-text-primary">Question</label><textarea id={`draft-question-${draftId ?? 'manual'}-${lessonId}`} required maxLength={2000} value={question} onChange={(e) => setQuestion(e.target.value)} rows={2} className={`${settingsFieldClass} min-h-16 w-full`} />
    <label htmlFor={`draft-answer-${draftId ?? 'manual'}-${lessonId}`} className="mb-1.5 mt-4 block text-xs font-semibold text-text-primary">Answer</label><textarea id={`draft-answer-${draftId ?? 'manual'}-${lessonId}`} required maxLength={1000} value={answer} onChange={(e) => setAnswer(e.target.value)} rows={2} className={`${settingsFieldClass} min-h-16 w-full`} />
    <label htmlFor={`draft-explanation-${draftId ?? 'manual'}-${lessonId}`} className="mb-1.5 mt-4 block text-xs font-semibold text-text-primary">Why this answer works</label><textarea id={`draft-explanation-${draftId ?? 'manual'}-${lessonId}`} required maxLength={3000} value={explanation} onChange={(e) => setExplanation(e.target.value)} rows={3} className={`${settingsFieldClass} min-h-20 w-full`} />
    <fieldset className="mt-4"><legend className="text-xs font-semibold text-text-primary">Source provenance</legend><p className="mt-1 text-[10px] text-text-muted">{origin === 'generated' ? 'Generated cards must keep at least one source referenced in this lesson.' : 'Select excerpts that support this card, or leave it as a personal mnemonic.'}</p><div className="mt-2 flex flex-wrap gap-2">{eligibleSources.map((source) => <label key={source.id} className="inline-flex cursor-pointer items-center gap-1.5 rounded-full border border-border px-2.5 py-1.5 text-[10px] text-text-secondary"><input type="checkbox" checked={sourceIds.includes(source.id)} onChange={(event) => setSourceIds((current) => event.target.checked ? [...current, source.id] : current.filter((id) => id !== source.id))} className="accent-accent" />{source.title}</label>)}{eligibleSources.length === 0 && <span className="text-[10px] text-text-muted">{origin === 'generated' ? 'No source is attached to this prepared lesson.' : 'No program sources'}</span>}</div>{origin === 'generated' && !sourceIds.length && <p className="mt-2 text-[10px] text-amber-700">Choose at least one lesson source to save this generated draft.</p>}</fieldset>
    {error && <p role="alert" className="mt-3 text-xs text-rose-700">{error}</p>}
    <SourceProvenance sourceIds={sourceIds} program={program} />
    <div className="mt-4 flex justify-end gap-2">{onCancel && <button type="button" disabled={pending} onClick={onCancel} className="rounded-full px-3 py-2 text-xs text-text-secondary hover:bg-surface">Cancel</button>}<button type="submit" disabled={pending || !question.trim() || !answer.trim() || !explanation.trim() || (origin === 'generated' && !sourceIds.length)} className="inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"><Save size={13} />{pending ? 'Saving…' : draftId ? 'Save draft edits' : 'Save card draft'}</button></div>
  </form>;
}

function AcceptedCardEditor({ card, origin, program, pending, error, onSave }: {
  card: StudyCardDto; origin: LearningCardOriginDto; program: LearningProgramDto; pending: boolean; error?: string;
  onSave: (card: StudyCardDto, question: string, answer: string, explanation: string, onSuccess: () => void) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [question, setQuestion] = useState(card.question);
  const [answer, setAnswer] = useState(card.answer);
  const [explanation, setExplanation] = useState(card.explanation);
  if (!editing) return <article className="rounded-xl border border-border/70 bg-background p-4"><div className="flex items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-wider text-emerald-700">Accepted · {origin.origin} source</div><h4 className="mt-1 font-medium leading-6 text-text-primary">{card.question}</h4><p className="mt-2 text-xs text-text-muted">Answer and supporting excerpts are available after revealing this card during review.</p></div><button type="button" onClick={() => setEditing(true)} aria-label={`Edit accepted card: ${card.question}`} className="rounded-full border border-border p-2 text-text-muted hover:text-accent"><PencilLine size={14} /></button></div></article>;
  return <form onSubmit={(event) => { event.preventDefault(); onSave(card, question, answer, explanation, () => setEditing(false)); }} className="rounded-xl border border-accent/30 bg-background p-4">
    <div className="mb-3 flex items-center justify-between"><span className="text-xs font-semibold text-text-primary">Edit accepted card</span><button type="button" onClick={() => { setEditing(false); setQuestion(card.question); setAnswer(card.answer); setExplanation(card.explanation); }} aria-label="Cancel card edit" className="rounded-full p-1.5 text-text-muted hover:bg-surface"><Undo2 size={14} /></button></div>
    <label className="mb-1 block text-xs" htmlFor={`accepted-question-${card.id}`}>Question</label><textarea id={`accepted-question-${card.id}`} maxLength={2000} value={question} onChange={(e) => setQuestion(e.target.value)} required className={`${settingsFieldClass} w-full`} />
    <label className="mb-1 mt-3 block text-xs" htmlFor={`accepted-answer-${card.id}`}>Answer</label><textarea id={`accepted-answer-${card.id}`} maxLength={1000} value={answer} onChange={(e) => setAnswer(e.target.value)} required className={`${settingsFieldClass} w-full`} />
    <label className="mb-1 mt-3 block text-xs" htmlFor={`accepted-explanation-${card.id}`}>Explanation</label><textarea id={`accepted-explanation-${card.id}`} maxLength={3000} value={explanation} onChange={(e) => setExplanation(e.target.value)} required className={`${settingsFieldClass} w-full`} />
    {error && <p role="alert" className="mt-3 text-xs text-rose-700">{error}</p>}<SourceProvenance sourceIds={origin.sourceIds} program={program} />
    <button type="submit" disabled={pending || !question.trim() || !answer.trim() || !explanation.trim()} className="mt-3 inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-white disabled:opacity-50">{pending ? 'Saving…' : 'Save card'}</button>
  </form>;
}

export function RecallPanel({ program, lessonId, memory, memoryLoading, memoryError, onRetryMemory }: {
  program: LearningProgramDto; lessonId?: string; memory: LearningMemoryDto | undefined; memoryLoading: boolean; memoryError?: string; onRetryMemory: () => void;
}) {
  const generate = useGenerateLearningCardDrafts();
  const saveDraft = useSaveLearningCardDraft();
  const acceptDraft = useAcceptLearningCardDraft();
  const discardDraft = useDiscardLearningCardDraft();
  const updateCard = useUpdateLearningStudyCard();
  const review = useReviewLearningStudyCard();
  const [count, setCount] = useState(3);
  const [manualOpen, setManualOpen] = useState(false);
  const [draftSaveTarget, setDraftSaveTarget] = useState<string | null>(null);
  const [cardUpdateTarget, setCardUpdateTarget] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [reviewedIds, setReviewedIds] = useState<string[]>([]);
  const [reviewedCard, setReviewedCard] = useState<StudyCardDto | null>(null);
  const [reviewError, setReviewError] = useState('');
  const reviewRequest = useRef<{ cardId: string; rating: 'again' | 'hard' | 'good' | 'easy'; id: string } | null>(null);
  const dueCards = (memory?.studyDeck?.cards ?? []).filter((card) => card.dueAt <= Date.now() && !reviewedIds.includes(card.id)).sort((a, b) => a.dueAt - b.dueAt);
  const currentCard = dueCards[0];
  const selectedLesson = program.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === lessonId);
  const origins = memory?.acceptedCards.filter((origin) => origin.lessonId === lessonId) ?? [];
  const draftActionError = acceptDraft.error?.message || discardDraft.error?.message;
  const draftPending = saveDraft.isPending || acceptDraft.isPending || discardDraft.isPending;
  const runGeneration = () => { if (lessonId && selectedLesson?.preparation === 'ready') generate.mutate({ programId: program.summary.id, lessonId, count }); };

  const currentCardId = currentCard?.id;
  useEffect(() => {
    setRevealed(false);
    if (!currentCardId) return;
    setReviewError('');
    reviewRequest.current = null;
  }, [currentCardId]);

  const saveDraftValue = (draftId: string | null, value: { question: string; answer: string; explanation: string; sourceIds: string[] }) => {
    if (!lessonId) return;
    setDraftSaveTarget(draftId ?? 'manual');
    saveDraft.mutate({ draftId, programId: program.summary.id, lessonId, ...value }, { onSuccess: () => { setDraftSaveTarget(null); if (draftId === null) setManualOpen(false); } });
  };
  const editAccepted = (card: StudyCardDto, question: string, answer: string, explanation: string, onSuccess: () => void) => {
    setCardUpdateTarget(card.id);
    updateCard.reset();
    updateCard.mutate({ cardId: card.id, question, answer, explanation }, {
      onSuccess: () => {
        setCardUpdateTarget(null);
        onSuccess();
      },
    });
  };
  const acceptCard = (draftId: string) => {
    discardDraft.reset();
    acceptDraft.mutate({ programId: program.summary.id, draftId });
  };
  const discardCard = (draftId: string) => {
    acceptDraft.reset();
    discardDraft.mutate({ programId: program.summary.id, draftId });
  };
  const rateCard = (rating: 'again' | 'hard' | 'good' | 'easy') => {
    if (!currentCard || review.isPending) return;
    setReviewedCard(null);
    setReviewError('');
    if (reviewRequest.current?.cardId !== currentCard.id || reviewRequest.current?.rating !== rating) {
      reviewRequest.current = { cardId: currentCard.id, rating, id: crypto.randomUUID() };
    }
    const request = { reviewId: reviewRequest.current.id, cardId: currentCard.id, expectedReviews: currentCard.reviewCount, selectedOption: null, rating } as const;
    review.mutate(request, { onSuccess: (updated) => { setReviewedIds((ids) => [...ids, currentCard.id]); setReviewedCard(updated); setRevealed(false); reviewRequest.current = null; }, onError: (error) => setReviewError(error.message) });
  };

  if (memoryLoading && !memory) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"><LoaderCircle size={15} className="mr-2 inline animate-spin" />Loading recall data…</div>;
  if (memoryError && !memory) return <div className="rounded-2xl border border-rose-500/25 bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Recall data could not load</h3><p role="alert" className="mt-2 text-sm text-rose-700">{memoryError}</p><button type="button" onClick={onRetryMemory} className="mt-4 rounded-full border border-border px-4 py-2 text-sm">Retry</button></div>;
  if (!lessonId) return <div className="rounded-2xl border border-dashed border-border bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Choose a lesson to make recall cards</h3><p className="mt-2 text-sm text-text-secondary">Select a lesson to create a grounded card draft or review its accepted cards.</p></div>;

  return <div className="space-y-5">
    <section className="rounded-2xl border border-border bg-surface p-3 shadow-sm sm:p-6"><header className="flex flex-wrap items-start justify-between gap-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Recall studio · {memory?.schedulerVersion ?? 'loading schedule'}</div><h3 className="mt-1 font-serif text-2xl text-text-primary">Keep the ideas close</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Draft cards from a ready lesson, edit each one, and accept only what you want to remember.</p></div><div className="min-w-44 rounded-xl bg-background p-3"><div className="flex items-baseline justify-between"><span className="text-xs text-text-muted">Due now</span><strong className="font-serif text-2xl text-text-primary">{dueCards.length}</strong></div><div className="mt-1 text-[10px] text-text-muted">{memory?.studyDeck?.cards.length ?? 0} accepted cards total</div></div></header>
      <div className="mt-4 rounded-xl border border-[#d7c9b8] bg-[#f2ebe2] p-4 text-xs leading-5 text-text-secondary dark:border-white/10 dark:bg-white/5"><span className="font-semibold text-text-primary">Schedule: {memory?.schedulerVersion || 'expanding_v1'}.</span> This version spaces cards with expanding intervals. It is not FSRS and does not promise or measure a retention rate.</div>
      <div className="mt-5 flex flex-wrap items-center gap-3 border-t border-border pt-5"><label className="text-xs font-medium text-text-secondary" htmlFor="draft-card-count">Generate grounded drafts</label><select id="draft-card-count" value={count} onChange={(e) => setCount(Number(e.target.value))} className="rounded-lg border border-border bg-background px-3 py-2 text-sm">{Array.from({ length: 7 }, (_, index) => index + 2).map((value) => <option key={value} value={value}>{value} cards</option>)}</select><button type="button" disabled={generate.isPending || program.summary.status !== 'active' || selectedLesson?.preparation !== 'ready'} onClick={runGeneration} className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"><Sparkles size={14} />{generate.isPending ? 'Generating…' : 'Generate drafts'}</button><button type="button" disabled={program.summary.status !== 'active' || selectedLesson?.preparation !== 'ready'} onClick={() => setManualOpen((value) => !value)} className="inline-flex items-center gap-2 rounded-full border border-border px-4 py-2 text-xs font-medium text-text-secondary hover:border-accent/40 disabled:opacity-50"><PencilLine size={13} />{manualOpen ? 'Close manual draft' : 'Create manually'}</button></div>
      {program.summary.status === 'draft' && <p className="mt-3 text-xs text-text-muted">Accept the program before generating cards.</p>}
      {program.summary.status === 'active' && selectedLesson?.preparation !== 'ready' && <p className="mt-3 text-xs text-text-muted">Prepare this lesson before generating grounded card drafts.</p>}
      {generate.error && <div role="alert" className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl bg-rose-500/10 p-3 text-sm text-rose-700"><span>Draft generation failed: {generate.error.message}</span><button type="button" onClick={runGeneration} className="inline-flex items-center gap-1 text-xs underline"><RotateCcw size={12} /> Retry</button></div>}
      {manualOpen && <div className="mt-4"><DraftEditor draftId={null} lessonId={lessonId} program={program} origin="manual" pending={saveDraft.isPending} error={draftSaveTarget === 'manual' ? saveDraft.error?.message : undefined} onSave={(value) => saveDraftValue(null, value)} onCancel={() => setManualOpen(false)} /></div>}
      {draftActionError && <p role="alert" className="mt-3 text-xs text-rose-700">{draftActionError}</p>}
      <div className="mt-6"><h4 className="font-serif text-lg text-text-primary">Draft cards <span className="text-sm text-text-muted">{memory?.drafts.filter((draft) => draft.lessonId === lessonId).length ?? 0}</span></h4>
        {(memory?.drafts.filter((draft) => draft.lessonId === lessonId) ?? []).length ? <div className="mt-3 space-y-3">{memory?.drafts.filter((draft) => draft.lessonId === lessonId).map((draft) => <article key={draft.id} aria-label={`Draft card: ${draft.question}`} className="space-y-2"><DraftEditor draftId={draft.id} lessonId={lessonId} program={program} initial={draft} origin={draft.origin} pending={draftPending} error={draftSaveTarget === draft.id ? saveDraft.error?.message : undefined} onSave={(value) => saveDraftValue(draft.id, value)} /><div className="flex justify-end gap-2"><button type="button" disabled={draftPending || program.summary.status !== 'active' || selectedLesson?.preparation !== 'ready'} onClick={() => acceptCard(draft.id)} className="inline-flex items-center gap-1.5 rounded-full bg-emerald-700 px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"><Check size={13} />Accept card</button><button type="button" disabled={draftPending} onClick={() => discardCard(draft.id)} className="inline-flex items-center gap-1.5 rounded-full border border-border px-4 py-2 text-xs text-text-secondary disabled:opacity-50"><Trash2 size={13} />Discard</button></div></article>)}</div> : !manualOpen && !generate.isPending ? <p className="mt-2 rounded-xl border border-dashed border-border p-5 text-sm text-text-muted">No drafts for this lesson yet. Generate source-grounded suggestions or write one yourself.</p> : null}
      </div>
    </section>

    <section className="rounded-2xl border border-border bg-surface p-3 shadow-sm sm:p-6"><div className="flex flex-wrap items-end justify-between gap-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">In-Studio review</div><h3 className="mt-1 font-serif text-2xl text-text-primary">Due cards</h3><p className="mt-1 text-xs text-text-secondary">Prompt first, reveal when you are ready, then record a recall rating.</p></div><span className="rounded-full bg-background px-3 py-1.5 text-xs text-text-muted">{dueCards.length} due · {memory?.studyDeck?.cards.length ?? 0} total</span></div>
      {reviewedCard && <p role="status" className="mt-4 rounded-xl bg-emerald-500/10 px-4 py-3 text-sm text-emerald-800">Review saved. Next interval: {reviewedCard.intervalDays} days.</p>}
      {reviewError && <p role="alert" className="mt-4 rounded-xl bg-rose-500/10 px-4 py-3 text-sm text-rose-700">{reviewError}</p>}
      {currentCard ? <article aria-label="Current review card" className="mt-5 min-w-0 rounded-xl border border-border bg-background p-3 [overflow-wrap:anywhere] sm:p-7"><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Recall prompt</div><p className="mt-3 font-serif text-xl leading-7 text-text-primary">{currentCard.question}</p>{!revealed ? <button type="button" disabled={review.isPending} onClick={() => setRevealed(true)} className="mt-5 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-white">Reveal answer</button> : <div className="mt-5 border-t border-border pt-5"><div className="text-xs font-semibold uppercase tracking-[.12em] text-accent">Answer</div><p className="mt-2 text-base font-medium leading-6 text-text-primary">{currentCard.answer}</p><p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-text-secondary">{currentCard.explanation}</p><SourceProvenance sourceIds={memory?.acceptedCards.find((origin) => origin.cardId === currentCard.id)?.sourceIds ?? []} program={program} /><div className="mt-5"><div className="mb-2 text-xs text-text-muted">How did recall feel?</div><div className="flex flex-wrap gap-2">{(['again', 'hard', 'good', 'easy'] as const).map((rating) => <button key={rating} type="button" disabled={review.isPending} onClick={() => rateCard(rating)} className="rounded-full border border-border px-4 py-2 text-xs font-medium capitalize text-text-secondary hover:border-accent/50 hover:text-accent disabled:opacity-50">{review.isPending ? 'Saving…' : rating}</button>)}</div></div></div>}</article> : <div className="mt-5 rounded-xl border border-dashed border-border p-7 text-center"><h4 className="font-serif text-xl text-text-primary">{memory?.studyDeck?.cards.length ? 'You are caught up' : 'No accepted cards yet'}</h4><p className="mt-2 text-sm leading-6 text-text-secondary">{memory?.studyDeck?.cards.length ? 'There are no due cards in this program right now.' : 'Accepted cards will appear here when their scheduled review is due.'}</p></div>}
      {memory?.acceptedCards.length ? <div className="mt-7 border-t border-border pt-6"><h4 className="font-serif text-lg text-text-primary">Accepted cards for this lesson</h4><div className="mt-3 space-y-3">{origins.map((origin) => { const card = memory.studyDeck?.cards.find((item) => item.id === origin.cardId); return card ? <AcceptedCardEditor key={card.id} card={card} origin={origin} program={program} pending={updateCard.isPending} error={cardUpdateTarget === card.id ? updateCard.error?.message : undefined} onSave={editAccepted} /> : null; })}</div>{origins.length === 0 && <p className="mt-2 text-xs text-text-muted">No accepted cards yet for this lesson.</p>}</div> : null}
    </section>
  </div>;
}

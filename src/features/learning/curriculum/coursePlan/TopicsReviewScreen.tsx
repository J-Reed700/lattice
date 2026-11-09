import { useMemo, useState } from 'react';

import { ArrowLeft } from 'lucide-react';

import { PlainError } from '@/components/PlainError';
import { Button } from '@/components/ui/button';
import { CoursePlanStatus, IDEAS_SEPARATOR } from '@/features/learning/curriculum/coursePlan/PlannedTopics';
import { TopicLists } from '@/features/learning/curriculum/coursePlan/TopicLists';
import {
  useChangeCoursePlanEntries,
  useCoursePlan,
  useEditCoursePlanEntry,
  useUndoCoursePlanEntry,
} from '@/features/learning/curriculum/coursePlan/useCoursePlan';
import { ReviewCheckbox, SelectionBar } from '@/features/learning/lessons/review/SelectionBar';
import { useReviewSelection } from '@/features/learning/lessons/review/useReviewSelection';
import type { CoursePlanEntryDto, CoursePlanViewDto, LearningProgramDto, PlannedLesson } from '@/lib/bindings';

type Form = { lessonId: string; kind: 'edit' | 'ask'; outlineRevision: number; entryRevision: number };
type Failure = { at: string | 'selection'; error: unknown };
const SELECTION = 'selection';
type ListKey = 'ideas' | 'terms' | 'actions';

const LISTS: { key: ListKey; singular: string }[] = [
  { key: 'ideas', singular: 'idea' },
  { key: 'terms', singular: 'term' },
  { key: 'actions', singular: 'action' },
];
const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);
const INPUT = 'w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm';

function AskForChange({ busy, error, onSend, onCancel }: { busy: boolean; error: unknown; onSend: (instruction: string) => Promise<boolean>; onCancel: () => void }) {
  const [text, setText] = useState('');
  return (
    <form
      className="mt-3 space-y-2 rounded-xl border border-border bg-background p-3"
      onSubmit={(event) => { event.preventDefault(); void onSend(text.trim()); }}
    >
      <textarea aria-label="What should change?" rows={2} autoFocus value={text} onChange={(event) => setText(event.target.value)} className={INPUT} />
      {error ? <PlainError error={error} className="rounded-xl bg-danger-muted p-3 text-sm text-text-primary" /> : null}
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={busy || !text.trim()}>Send</Button>
        <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  );
}

function ListEditor({ listKey, singular, items, onChange }: { listKey: ListKey; singular: string; items: string[]; onChange: (items: string[]) => void }) {
  return (
    <fieldset className="space-y-2">
      <legend className="text-xs font-semibold text-text-secondary">{capital(listKey)}</legend>
      {items.map((item, index) => (
        <div key={index} className="flex items-center gap-2">
          <input aria-label={`${capital(singular)} ${index + 1}`} value={item} className={INPUT}
            onChange={(event) => onChange(items.map((current, at) => (at === index ? event.target.value : current)))} />
          <Button type="button" size="sm" variant="ghost" aria-label={`Remove ${singular} ${index + 1}`}
            onClick={() => onChange(items.filter((_, at) => at !== index))}>Remove</Button>
        </div>
      ))}
      <Button type="button" size="sm" variant="outline" onClick={() => onChange([...items, ''])}>{`Add ${singular}`}</Button>
    </fieldset>
  );
}

function EditForm({ topics, busy, onSave, onCancel }: { topics: PlannedLesson; busy: boolean; onSave: (topics: PlannedLesson) => Promise<boolean>; onCancel: () => void }) {
  const [draft, setDraft] = useState(topics);
  const cleaned = (items: string[]) => items.map((item) => item.trim()).filter(Boolean);
  const result: PlannedLesson = { lessonId: topics.lessonId, ideas: cleaned(draft.ideas), terms: cleaned(draft.terms), actions: cleaned(draft.actions), contrast: draft.contrast.trim() };
  return (
    <form className="mt-3 space-y-4" onSubmit={(event) => { event.preventDefault(); void onSave(result); }}>
      {LISTS.map(({ key, singular }) => (
        <ListEditor key={key} listKey={key} singular={singular} items={draft[key]} onChange={(items) => setDraft({ ...draft, [key]: items })} />
      ))}
      <label className="block text-xs font-semibold text-text-secondary">
        Contrast
        <textarea aria-label="Contrast" rows={2} value={draft.contrast} onChange={(event) => setDraft({ ...draft, contrast: event.target.value })} className={`${INPUT} mt-1 font-normal`} />
      </label>
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={busy || result.ideas.length === 0}>Save</Button>
        <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  );
}

function TopicCard({ title, written, error, entry, form, busy, selected, onToggle, onForm, onEdit, onChange, onUndo }: {
  title: string;
  written: boolean;
  error: unknown;
  entry: CoursePlanEntryDto;
  form: Form['kind'] | null;
  busy: boolean;
  selected: boolean;
  onToggle: (shiftKey: boolean) => void;
  onForm: (form: Form['kind'] | null) => void;
  onEdit: (topics: PlannedLesson) => Promise<boolean>;
  onChange: (instruction: string) => Promise<boolean>;
  onUndo: () => void;
}) {
  const { topics, previous, editable } = entry;
  return (
    <article aria-label={title} className="rounded-xl border border-border bg-surface p-4 text-sm">
      <div className="flex items-start gap-2">
        {editable && <ReviewCheckbox label={`Select ${title}`} checked={selected} onToggle={onToggle} />}
        <h3 className="font-serif text-lg leading-snug text-text-primary">{title}</h3>
      </div>
      {form === 'edit'
        ? <EditForm topics={topics} busy={busy} onSave={onEdit} onCancel={() => onForm(null)} />
        : <TopicLists topics={topics} className="mt-2 space-y-2 text-text-secondary" />}
      {previous && <p className="mt-2 text-xs text-text-muted">{`Was: ${previous.ideas.join(IDEAS_SEPARATOR)}`}</p>}
      {!editable && <p className="mt-3 text-xs text-text-secondary">{written ? 'Already written; its topics are fixed' : 'Being written; its topics are fixed'}</p>}
      {editable && form === null && (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => onForm('edit')}>Edit</Button>
          <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => onForm('ask')}>Ask for a change</Button>
          {previous && <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onUndo}>Undo</Button>}
        </div>
      )}
      {editable && form === 'ask' && <AskForChange busy={busy} error={error} onSend={onChange} onCancel={() => onForm(null)} />}
      {error && form !== 'ask' ? <PlainError error={error} className="mt-3 rounded-xl bg-danger-muted p-3 text-sm text-text-primary" /> : null}
    </article>
  );
}

export function TopicsReviewScreen({ program, onBack }: { program: LearningProgramDto; onBack: () => void }) {
  const programId = program.summary.id;
  const planQuery = useCoursePlan(programId);
  const edit = useEditCoursePlanEntry();
  const change = useChangeCoursePlanEntries();
  const undo = useUndoCoursePlanEntry();
  const [form, setForm] = useState<Form | null>(null);
  const [bulkOpen, setBulkOpen] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const view: CoursePlanViewDto | undefined = planQuery.data;
  const entries = useMemo(() => new Map((view?.entries ?? []).map((entry) => [entry.topics.lessonId, entry])), [view]);
  const lessonKeys = program.modules.flatMap((module) => module.lessons).filter((lesson) => entries.get(lesson.id)?.editable).map((lesson) => lesson.id);
  const selection = useReviewSelection(lessonKeys);
  const busy = edit.isPending || change.isPending || undo.isPending;

  const run = async (at: string, action: () => Promise<unknown>) => {
    setFailure(null);
    try {
      await action();
      return true;
    } catch (caught) {
      setFailure({ at, error: caught });
      return false;
    }
  };
  const editEntry = (opened: Form, topics: PlannedLesson) => run(opened.lessonId, async () => {
    await edit.mutateAsync({ programId, outlineRevision: opened.outlineRevision, expectedEntryRevision: opened.entryRevision, topics });
    setForm(null);
  });
  const askChange = (at: string, lessonIds: string[], instruction: string) => run(at, async () => {
    await change.mutateAsync({
      programId, outlineRevision: view!.outlineRevision, instruction,
      lessons: lessonIds.map((lessonId) => ({ lessonId, expectedEntryRevision: entries.get(lessonId)!.entryRevision })),
    });
    if (at === SELECTION) {
      setBulkOpen(false);
      selection.clear();
    } else {
      setForm(null);
    }
  });
  const undoEntry = (entry: CoursePlanEntryDto) => run(entry.topics.lessonId, () => undo.mutateAsync({
    programId, outlineRevision: view!.outlineRevision, lessonId: entry.topics.lessonId, expectedEntryRevision: entry.entryRevision,
  }));

  const shownError = planQuery.error;
  return (
    <div className="mx-auto max-w-3xl space-y-6 px-5 py-6 sm:px-8">
      <div className="flex items-center gap-3">
        <Button type="button" variant="ghost" size="sm" onClick={onBack}><ArrowLeft size={15} /> Back</Button>
        <h1 className="font-serif text-2xl text-text-primary">Review topics</h1>
      </div>
      {shownError ? <PlainError error={shownError} className="rounded-xl bg-danger-muted p-4 text-sm text-text-primary" /> : null}
      {view?.state !== 'planned' ? <CoursePlanStatus programId={programId} /> : program.modules.map((module) => {
        const lessons = module.lessons.filter((lesson) => entries.has(lesson.id));
        if (lessons.length === 0) return null;
        return (
          <section key={module.id} className="space-y-3">
            <h2 className="text-sm font-semibold text-text-secondary">{module.title}</h2>
            {lessons.map((lesson) => (
              <TopicCard key={lesson.id} title={lesson.title} written={lesson.preparation !== 'outline'} error={failure?.at === lesson.id ? failure.error : null}
                entry={entries.get(lesson.id)!} busy={busy}
                form={form?.lessonId === lesson.id ? form.kind : null} selected={selection.isSelected(lesson.id)}
                onToggle={(shiftKey) => selection.toggle(lesson.id, { shiftKey })}
                onForm={(kind) => setForm(kind ? { lessonId: lesson.id, kind, outlineRevision: view!.outlineRevision, entryRevision: entries.get(lesson.id)!.entryRevision } : null)}
                onEdit={(topics) => editEntry(form!, topics)}
                onChange={(instruction) => askChange(lesson.id, [lesson.id], instruction)}
                onUndo={() => void undoEntry(entries.get(lesson.id)!)} />
            ))}
          </section>
        );
      })}
      {bulkOpen && selection.selected.size > 0 && (
        <AskForChange busy={busy} error={failure?.at === SELECTION ? failure.error : null} onSend={(instruction) => askChange(SELECTION, [...selection.selected], instruction)} onCancel={() => setBulkOpen(false)} />
      )}
      <SelectionBar count={selection.selected.size} noun={['lesson', 'lessons']} busy={busy} onClear={() => { selection.clear(); setBulkOpen(false); }}
        actions={[{ label: 'Ask for a change', onClick: () => setBulkOpen(true) }]} />
    </div>
  );
}

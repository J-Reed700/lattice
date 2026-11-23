import { type FormEvent, useState } from 'react';

import { ArrowRight, BookOpen, Check, Clock3, Layers3, Link2, Sparkles } from 'lucide-react';

import { PlainError } from '@/components/PlainError';
import { Button } from '@/components/ui/button';
import { Slider } from '@/components/ui/slider';
import { Textarea } from '@/components/ui/textarea';
import { OutlineGenerationProgress } from '@/features/learning/curriculum/OutlineGenerationProgress';
import { programBuilderDraftKeys } from '@/features/learning/curriculum/programBuilderDraft';
import { LearningDocumentPicker, type LearningDocumentSelection } from '@/features/learning/sources/LearningDocumentPicker';
import type { GenerateLearningProgramRequestDto, LearningCourseDepth, LearningOutlineProgressDto } from '@/lib/bindings';
import { useDraft } from '@/shared/drafts/drafts';


const depths: { id: LearningCourseDepth; title: string; modules: string; lessons: string; description: string }[] = [
  { id: 'focused', title: 'Focused course', modules: '2–3 modules', lessons: '4–9 lessons', description: 'Build one specific skill and put it into practice.' },
  { id: 'course', title: 'Complete course', modules: '4–6 modules', lessons: '12–30 lessons', description: 'A full progression from foundations to a final project.' },
  { id: 'deep_dive', title: 'Deep dive', modules: '6–10 modules', lessons: '24–60 lessons', description: 'Explore the subject in depth, with advanced applications and synthesis.' },
];

function parseCourseDepth(value: string): LearningCourseDepth {
  if (value === 'focused' || value === 'course' || value === 'deep_dive') return value;
  return 'course';
}

function parseSessionMinutes(value: string): number {
  const parsed = Number(value);
  if (Number.isFinite(parsed) && parsed >= 15 && parsed <= 90) return Math.round(parsed);
  return 30;
}

export function ProgramBuilder({ pending, error, onDismissError, activeDepth, onGenerate, onCancel, progress, onStop, onOpenDraft, cancelling, cancelError }: {
  progress?: LearningOutlineProgressDto | null; onStop?: () => void; onOpenDraft?: () => void; cancelling?: boolean; cancelError?: string;
  pending: boolean; error?: Error; onDismissError?: () => void; activeDepth?: LearningCourseDepth | null; onGenerate: (request: GenerateLearningProgramRequestDto) => void; onCancel: () => void;
}) {
  // The text survives leaving the builder; it clears once a course is made from it.
  const [goal, setGoal] = useDraft(programBuilderDraftKeys.goal);
  const [prior, setPrior] = useDraft(programBuilderDraftKeys.prior);
  const [minutesDraft, setMinutesDraft] = useDraft(programBuilderDraftKeys.minutes);
  const [depthDraft, setDepthDraft] = useDraft(programBuilderDraftKeys.depth);
  const [urls, setUrls] = useDraft(programBuilderDraftKeys.urls);
  const [documents, setDocuments] = useState<LearningDocumentSelection[]>([]);
  const [materialsOpen, setMaterialsOpen] = useState(false);
  const [formError, setFormError] = useState('');
  const minutes = parseSessionMinutes(minutesDraft);
  const depth = parseCourseDepth(depthDraft);
  const resolvedDepth = pending && activeDepth ? activeDepth : depth;
  const course = depths.find((item) => item.id === resolvedDepth)!;
  const hasSources = documents.length > 0 || urls.trim().length > 0;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const sourceUrls = [...new Set(urls.split(/\n/).map((url) => url.trim()).filter(Boolean))];
    if (goal.trim().length < 3) return setFormError('Describe what you want to be able to do in at least 3 characters.');
    for (const url of sourceUrls) {
      try { if (!['http:', 'https:'].includes(new URL(url).protocol)) throw new Error(); }
      catch { return setFormError(`Use a complete http or https URL: ${url}`); }
    }
    setFormError('');
    onGenerate({ goal: goal.trim(), priorKnowledge: prior.trim(), minutesPerSession: minutes, documentIds: documents.map((item) => item.id), sourceUrls, courseDepth: depth });
  };

  return <div className="mx-auto max-w-6xl">
    <Button type="button" variant="ghost" size="sm" onClick={onCancel} className="mb-5 -ml-2 text-text-muted hover:text-text-primary">← Back to programs</Button>
    <div className="mb-7 max-w-3xl"><div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.16em] text-accent"><Sparkles size={15} /> Your personal classroom</div><h2 className="mt-3 font-serif text-3xl leading-tight text-text-primary sm:text-4xl">What would you like to learn?</h2><p className="mt-3 text-sm leading-6 text-text-secondary">Start with an ambition. Build a course that takes you from understanding the ideas to using them independently.</p></div>
    <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
      <form onSubmit={submit} className="min-w-0 rounded-xl border border-border-subtle bg-surface p-6 shadow-sm sm:p-7">
        <fieldset disabled={pending} className="min-w-0 disabled:opacity-70">
          <legend className="text-xs font-semibold uppercase tracking-[.14em] text-accent">01 · Your destination</legend>
          <div className="mt-4 space-y-4">
            <Textarea
              id="learning-goal"
              label="Your goal"
              value={goal}
              onChange={(event) => setGoal(event.target.value)}
              rows={3}
              placeholder="For example: Learn statistics well enough to design an experiment, interpret the results, and explain my conclusions."
            />
            <Textarea
              id="prior-knowledge"
              label="What do you already know?"
              value={prior}
              onChange={(event) => setPrior(event.target.value)}
              rows={2}
              placeholder="Your experience, skills you want to strengthen, and how you hope to use them. Leave blank to start from the foundations."
            />
          </div>
          <div className="mt-7 border-t border-border-subtle pt-6">
            <h3 className="text-xs font-semibold uppercase tracking-[.14em] text-accent">02 · Depth and pace</h3>
            <fieldset className="mt-4">
              <legend className="mb-3 text-xs font-medium text-text-secondary">How far do you want to go?</legend>
              <div className="grid gap-3 sm:grid-cols-3">
                {depths.map((item) => (
                  <label key={item.id} className={`relative cursor-pointer rounded-xl border p-4 transition ${resolvedDepth === item.id ? 'border-accent bg-accent-muted/45 ring-1 ring-accent/30' : 'border-border-subtle bg-bg/40 hover:border-border-default'}`}>
                    <input type="radio" name="course-depth" value={item.id} checked={resolvedDepth === item.id} onChange={() => setDepthDraft(item.id)} className="mb-2.5 accent-accent" />
                    <span className="block text-sm font-semibold text-text-primary">{item.title}</span>
                    <span className="mt-0.5 block text-xs font-medium text-accent">{item.modules}</span>
                    <span className="mt-2 block text-xs leading-5 text-text-secondary">{item.description}</span>
                  </label>
                ))}
              </div>
            </fieldset>
            <div className="mt-5">
              <Slider
                id="session-time"
                label="Time for each session"
                min={15}
                max={90}
                step={5}
                value={minutes}
                onValueChange={(next) => setMinutesDraft(String(next))}
                showValue
                formatValue={(val) => `${val} minutes`}
                minLabel="15 min"
                maxLabel="90 min"
                description="Short sessions break the subject into smaller steps. They don’t reduce the depth of your course."
              />
            </div>
          </div>
          <div className="mt-7 border-t border-border-subtle pt-6">
            <h3 className="text-xs font-semibold uppercase tracking-[.14em] text-accent">03 · Materials, if you have them</h3>
            <p className="mt-2.5 text-sm leading-6 text-text-secondary">Use textbooks, papers, notes, or reference pages you trust. Start with a topic or add your own materials. Lattice automatically researches supporting references when sources are missing or review finds gaps.</p>
            <Button type="button" variant="outline" aria-expanded={materialsOpen} aria-controls="course-materials" onClick={() => setMaterialsOpen(!materialsOpen)} className="mt-3.5">
              <BookOpen size={15} />
              {materialsOpen ? 'Hide materials' : 'Add optional materials'}
              {documents.length > 0 && ` · ${documents.length} selected`}
            </Button>
            <div id="course-materials" hidden={!materialsOpen} className="mt-4 space-y-4">
              {materialsOpen && <LearningDocumentPicker selected={documents} onChange={setDocuments} />}
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-text-secondary" htmlFor="reference-urls">
                  <Link2 size={13} className="text-text-muted" aria-hidden="true" />
                  Reference URLs (one per line; each URL saves one page)
                </label>
                <Textarea
                  id="reference-urls"
                  value={urls}
                  onChange={(event) => setUrls(event.target.value)}
                  rows={Math.max(4, urls.split('\n').length + 1)}
                  wrap="off"
                  placeholder="https://…"
                  className="max-h-[50vh] overflow-auto font-mono text-xs"
                />
              </div>
            </div>
          </div>
          {!pending && formError && <p role="alert" className="mt-5 rounded-lg bg-danger-muted/60 px-4 py-3 text-sm text-danger-fg">{formError}</p>}
          {!pending && !formError && error && <div className="mt-5 space-y-2"><PlainError error={error} className="rounded-lg bg-danger-muted/60 px-4 py-3 text-sm text-danger-fg" />{onDismissError && <Button type="button" variant="ghost" size="sm" onClick={onDismissError} className="px-0 text-xs text-text-muted underline hover:bg-transparent hover:text-text-primary">Dismiss error</Button>}</div>}
          {!pending && <div className="mt-7 border-t border-border-subtle pt-6"><Button type="submit" size="lg" className="w-full">Create outline<ArrowRight size={16} /></Button><p className="mt-3 text-center text-xs leading-5 text-text-muted">Writing and reviewing can take several minutes, depending on your model and course depth. You can follow each step and cancel while it runs.</p></div>}
        </fieldset>
        {pending && <OutlineGenerationProgress onOpenDraft={onOpenDraft} progress={progress} cancelling={cancelling} onCancel={onStop} cancelError={cancelError} deepDive={resolvedDepth === 'deep_dive'} />}
      </form>
      <aside className="rounded-xl border border-border-subtle bg-surface-raised/60 p-6 xl:sticky xl:top-6"><div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">Your course at a glance</div><h3 className="mt-3 font-serif text-2xl text-text-primary">{course.title}</h3><div className="mt-3.5 space-y-2 text-sm text-text-secondary"><p className="flex items-center gap-2"><Layers3 size={15} />{course.modules} · {course.lessons}</p><p className="flex items-center gap-2"><Clock3 size={15} />{minutes}-minute sessions</p></div><div className="mt-5 border-t border-border-subtle pt-5"><h4 className="text-sm font-semibold text-text-primary">A complete learning cycle</h4><ol className="mt-3.5 space-y-3.5">{[['Understand', 'In-depth explanations and worked examples.'], ['Apply', 'Guided exercises, then an independent assignment.'], ['Check', 'Lesson quizzes and cumulative module checks.'], ['Build', 'Projects that bring your skills together.'], ['Remember', 'Notes, recall cards, and spaced review.']].map(([title, description], index) => <li key={title} className="flex gap-3"><span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-surface text-xs font-medium text-accent">{index + 1}</span><div><div className="text-sm font-medium text-text-primary">{title}</div><p className="mt-0.5 text-xs leading-5 text-text-secondary">{description}</p></div></li>)}</ol></div><p className="mt-5 flex items-start gap-2 rounded-lg border border-border-subtle bg-surface p-3 text-xs leading-5 text-text-secondary"><Check size={15} className="mt-0.5 shrink-0 text-accent" />{hasSources ? 'Your materials ground the course. Additional references are researched automatically when review finds gaps.' : 'Start from your topic. Lattice searches for supporting references automatically and saves them with the draft.'}</p></aside>
    </div>
  </div>;
}

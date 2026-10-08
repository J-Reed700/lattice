import { type FormEvent, useState } from 'react';

import { ArrowRight, BookOpen, Check, Clock3, Layers3, Link2, Sparkles } from 'lucide-react';

import { OutlineGenerationProgress } from '@/features/learning/curriculum/OutlineGenerationProgress';
import { LearningDocumentPicker, type LearningDocumentSelection } from '@/features/learning/sources/LearningDocumentPicker';
import type { GenerateLearningProgramRequestDto, LearningCourseDepth, LearningOutlineProgressDto } from '@/lib/bindings';


const depths: { id: LearningCourseDepth; title: string; modules: string; lessons: string; description: string }[] = [
  { id: 'focused', title: 'Focused course', modules: '2–3 modules', lessons: '4–9 lessons', description: 'Build one specific skill and put it into practice.' },
  { id: 'course', title: 'Complete course', modules: '4–6 modules', lessons: '12–30 lessons', description: 'A full progression from foundations to a final project.' },
  { id: 'deep_dive', title: 'Deep dive', modules: '6–10 modules', lessons: '24–60 lessons', description: 'Explore the subject in depth, with advanced applications and synthesis.' },
];

export function ProgramBuilder({ pending, error, onGenerate, onCancel, progress, onStop, onOpenDraft, cancelling, cancelError }: {
  progress?: LearningOutlineProgressDto | null; onStop?: () => void; onOpenDraft?: () => void; cancelling?: boolean; cancelError?: string;
  pending: boolean; error?: string; onGenerate: (request: GenerateLearningProgramRequestDto) => void; onCancel: () => void;
}) {
  const [goal, setGoal] = useState('');
  const [prior, setPrior] = useState('');
  const [minutes, setMinutes] = useState(30);
  const [depth, setDepth] = useState<LearningCourseDepth>('course');
  const [urls, setUrls] = useState('');
  const [documents, setDocuments] = useState<LearningDocumentSelection[]>([]);
  const [materialsOpen, setMaterialsOpen] = useState(false);
  const [formError, setFormError] = useState('');
  const course = depths.find((item) => item.id === depth)!;
  const hasSources = documents.length > 0 || urls.trim().length > 0;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const sourceUrls = [...new Set(urls.split(/\n/).map((url) => url.trim()).filter(Boolean))];
    if (goal.trim().length < 3) return setFormError('Describe what you want to be able to do in at least 3 characters.');
    if (goal.trim().length > 500) return setFormError('Keep your learning goal to 500 characters or fewer.');
    if (documents.length > 8) return setFormError('Choose no more than 8 library documents.');
    if (sourceUrls.length > 8) return setFormError('Add no more than 8 reference URLs.');
    if (documents.length + sourceUrls.length > 12) return setFormError('Choose no more than 12 sources in total.');
    for (const url of sourceUrls) {
      try { if (!['http:', 'https:'].includes(new URL(url).protocol)) throw new Error(); }
      catch { return setFormError(`Use a complete http or https URL: ${url}`); }
    }
    setFormError('');
    onGenerate({ goal: goal.trim(), priorKnowledge: prior.trim(), minutesPerSession: minutes, documentIds: documents.map((item) => item.id), sourceUrls, courseDepth: depth });
  };

  return <div className="mx-auto max-w-6xl">
    <button type="button" disabled={pending} onClick={onCancel} className="mb-6 min-h-10 text-sm text-text-muted hover:text-text-primary disabled:opacity-50">← Back to programs</button>
    <div className="mb-8 max-w-3xl"><div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.18em] text-accent"><Sparkles size={16} /> Your personal classroom</div><h2 className="mt-4 font-serif text-4xl leading-tight text-text-primary sm:text-5xl">What would you like to learn?</h2><p className="mt-4 text-base leading-7 text-text-secondary">Start with an ambition. Build a course that takes you from understanding the ideas to using them independently.</p></div>
    <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
      <form onSubmit={submit} className="min-w-0 rounded-2xl border border-border bg-surface p-6 shadow-sm sm:p-8">
        <fieldset disabled={pending} className="min-w-0 disabled:opacity-70">
          <legend className="text-xs font-semibold uppercase tracking-[.15em] text-accent">01 · Your destination</legend>
          <label className="mt-5 block text-sm font-medium text-text-primary" htmlFor="learning-goal">Your goal</label>
          <textarea id="learning-goal" value={goal} onChange={(event) => setGoal(event.target.value)} rows={3} maxLength={500} placeholder="For example: Learn statistics well enough to design an experiment, interpret the results, and explain my conclusions." className="mt-2 w-full resize-y rounded-xl border border-border bg-background px-4 py-3 text-sm leading-6 outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15" /><div className="mt-1 text-right text-xs tabular-nums text-text-muted">{goal.length}/500</div>
          <label className="mt-4 block text-sm font-medium text-text-primary" htmlFor="prior-knowledge">What do you already know?</label>
          <textarea id="prior-knowledge" value={prior} onChange={(event) => setPrior(event.target.value)} rows={2} maxLength={2000} placeholder="Your experience, skills you want to strengthen, and how you hope to use them. Leave blank to start from the foundations." className="mt-2 w-full resize-y rounded-xl border border-border bg-background px-4 py-3 text-sm leading-6 outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15" />
          <div className="mt-8 border-t border-border pt-7"><h3 className="text-xs font-semibold uppercase tracking-[.15em] text-accent">02 · Depth and pace</h3><fieldset className="mt-4"><legend className="mb-3 text-sm font-medium text-text-primary">How far do you want to go?</legend><div className="grid gap-3 sm:grid-cols-3">{depths.map((item) => <label key={item.id} className={`relative cursor-pointer rounded-xl border p-4 transition ${depth === item.id ? 'border-accent bg-accent/5 ring-1 ring-accent/30' : 'border-border hover:border-accent/40'}`}><input type="radio" name="course-depth" value={item.id} checked={depth === item.id} onChange={() => setDepth(item.id)} className="mb-3 accent-accent" /><span className="block text-sm font-semibold text-text-primary">{item.title}</span><span className="mt-1 block text-xs font-medium text-accent">{item.modules}</span><span className="mt-3 block text-xs leading-5 text-text-secondary">{item.description}</span></label>)}</div></fieldset>
            <label className="mt-5 block text-sm font-medium text-text-primary" htmlFor="session-time">Time for each session</label><div className="mt-3 flex items-center gap-3"><input id="session-time" type="range" min="15" max="90" step="5" value={minutes} onChange={(event) => setMinutes(Number(event.target.value))} className="min-w-0 flex-1 accent-accent" /><span className="w-24 text-right text-sm tabular-nums text-text-secondary">{minutes} minutes</span></div><p className="mt-2 text-xs leading-5 text-text-muted">Short sessions break the subject into smaller steps. They don’t reduce the depth of your course.</p>
          </div>
          <div className="mt-8 border-t border-border pt-7"><h3 className="text-xs font-semibold uppercase tracking-[.15em] text-accent">03 · Materials, if you have them</h3><p className="mt-3 text-sm leading-6 text-text-secondary">Use textbooks, papers, notes, or reference pages you trust. Start with a topic or add your own materials. Lattice automatically researches supporting references when sources are missing or review finds gaps.</p><button type="button" aria-expanded={materialsOpen} aria-controls="course-materials" onClick={() => setMaterialsOpen(!materialsOpen)} className="mt-4 inline-flex min-h-11 items-center gap-2 rounded-xl border border-border px-4 text-sm font-medium text-text-primary hover:bg-background"><BookOpen size={16} />{materialsOpen ? 'Hide materials' : 'Add optional materials'}{documents.length > 0 && ` · ${documents.length} selected`}</button>
            <div id="course-materials" hidden={!materialsOpen} className="mt-5 space-y-4">{materialsOpen && <LearningDocumentPicker selected={documents} onChange={setDocuments} />}<label className="block text-xs font-medium text-text-secondary" htmlFor="reference-urls">Reference URLs (one per line; each URL saves one page)<div className="mt-2 flex items-start gap-2 rounded-xl border border-border bg-background px-3 py-2"><Link2 size={15} className="mt-1 shrink-0 text-text-muted" /><textarea id="reference-urls" value={urls} onChange={(event) => setUrls(event.target.value)} rows={2} placeholder="https://…" className="w-full resize-y bg-transparent text-sm outline-hidden" /></div></label></div>
          </div>
          {!pending && (formError || error) && <p role="alert" className="mt-5 rounded-lg bg-rose-500/10 px-4 py-3 text-sm text-rose-700">{formError || `The outline could not be generated: ${error}`}</p>}
          {!pending && <div className="mt-8 border-t border-border pt-6"><button type="submit" className="inline-flex min-h-12 w-full items-center justify-center gap-2 rounded-xl bg-accent px-5 py-3 text-sm font-semibold text-accent-fg transition hover:brightness-95">Create outline<ArrowRight size={17} /></button><p className="mt-3 text-center text-xs leading-5 text-text-muted">Writing and reviewing can take several minutes, depending on your model and course depth. You can follow each step and cancel while it runs.</p></div>}
        </fieldset>
        {pending && <OutlineGenerationProgress onOpenDraft={onOpenDraft} progress={progress} cancelling={cancelling} onCancel={onStop} cancelError={cancelError} deepDive={depth === 'deep_dive'} />}
      </form>
      <aside className="rounded-2xl border border-border bg-[#eee8df] p-6 xl:sticky xl:top-6 dark:bg-white/5"><div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Your course at a glance</div><h3 className="mt-4 font-serif text-2xl text-text-primary">{course.title}</h3><div className="mt-4 space-y-2 text-sm text-text-secondary"><p className="flex items-center gap-2"><Layers3 size={15} />{course.modules} · {course.lessons}</p><p className="flex items-center gap-2"><Clock3 size={15} />{minutes}-minute sessions</p></div><div className="mt-6 border-t border-border pt-5"><h4 className="text-sm font-semibold text-text-primary">A complete learning cycle</h4><ol className="mt-4 space-y-4">{[['Understand', 'In-depth explanations and worked examples.'], ['Apply', 'Guided exercises, then an independent assignment.'], ['Check', 'Lesson quizzes and cumulative module checks.'], ['Build', 'Projects that bring your skills together.'], ['Remember', 'Notes, recall cards, and spaced review.']].map(([title, description], index) => <li key={title} className="flex gap-3"><span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-surface/70 text-xs font-medium text-accent">{index + 1}</span><div><div className="text-sm font-medium text-text-primary">{title}</div><p className="mt-1 text-xs leading-5 text-text-secondary">{description}</p></div></li>)}</ol></div><p className="mt-6 flex items-start gap-2 rounded-xl bg-surface/60 p-3 text-xs leading-5 text-text-secondary"><Check size={15} className="mt-0.5 shrink-0 text-accent" />{hasSources ? 'Your materials ground the course. Additional references are researched automatically when review finds gaps.' : 'Start from your topic. Lattice searches for supporting references automatically and saves them with the draft.'}</p></aside>
    </div>
  </div>;
}

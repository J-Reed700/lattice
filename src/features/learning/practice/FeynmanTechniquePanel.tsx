import { useMemo, useState } from 'react';
import {
  AlertCircle,
  Check,
  CheckCircle2,
  Copy,
  HelpCircle,
  Lightbulb,
  Lock,
  PlusCircle,
  RotateCcw,
  Sparkles,
  Unlock,
  Volume2,
} from 'lucide-react';

import { CardText } from '@/features/learning/recall/CardText';
import type * as Wire from '@/lib/bindings';
import { useDraft } from '@/shared/drafts/drafts';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { analyzeFeynmanExplanation, lessonPoints } from './feynmanAnalysis';
import { SaveAsReviewCard } from './shared/SaveAsReviewCard';

export interface FeynmanTechniquePanelProps {
  program: Wire.LearningProgramDto;
  lesson?: Wire.LearningLessonDto;
  module?: Wire.LearningModuleDto;
}

/**
 * Each lesson keeps its own explanation. Choosing another lesson while the
 * panel is open starts that lesson's, rather than carrying this one over.
 */
export function FeynmanTechniquePanel(props: FeynmanTechniquePanelProps) {
  return <FeynmanWorkbench key={`${props.program.summary.id}:${props.lesson?.id ?? 'general'}`} {...props} />;
}

function FeynmanWorkbench({ program, lesson, module }: FeynmanTechniquePanelProps) {
  const defaultConcept = lesson?.title ?? module?.title ?? program.summary.title;
  const draftKey = `feynman:${program.summary.id}:${lesson?.id ?? 'general'}`;

  const [storedConcept, setSelectedConcept] = useDraft(`${draftKey}:concept`);
  const selectedConcept = storedConcept || defaultConcept;
  const [customConcept, setCustomConcept] = useState('');
  const [isCustom, setIsCustom] = useState(false);

  const activeConcept = isCustom ? customConcept : selectedConcept;

  const [explanation, setExplanation] = useDraft(`${draftKey}:explanation`);
  const [analogy, setAnalogy] = useDraft(`${draftKey}:analogy`);
  const [readAloud, setReadAloud] = useDraft(`${draftKey}:readAloud`);
  const readAloudConfirmed = readAloud === 'yes';

  const [copied, setCopied] = useState(false);
  const [comparison, setComparison] = useState<{ concept: string; covered: number[]; drafted: number[] } | null>(null);
  const [reviewCardPointIndex, setReviewCardPointIndex] = useState<number | null>(null);

  const analysis = useMemo(
    () => analyzeFeynmanExplanation(explanation, analogy, lesson, activeConcept),
    [explanation, analogy, lesson, activeConcept],
  );

  const points = useMemo(() => lessonPoints(activeConcept, lesson, module), [activeConcept, lesson, module]);
  const compared = comparison !== null && comparison.concept === activeConcept;
  const coveredCount = compared ? comparison.covered.length : 0;

  const toggleCovered = (index: number) => setComparison((current) => current && {
    ...current,
    covered: current.covered.includes(index) ? current.covered.filter((item) => item !== index) : [...current.covered, index],
  });

  const draftCard = (index: number) => {
    const point = points[index];
    if (!point) return;
    setReviewCardPointIndex(index);
    setComparison((current) => current && { ...current, drafted: [...current.drafted, index] });
  };

  const conceptOptions = useMemo(() => {
    const options: { id: string; label: string }[] = [];
    if (lesson?.title) {
      options.push({ id: lesson.title, label: `Lesson: ${lesson.title}` });
    }
    if (lesson?.objective) {
      options.push({ id: lesson.objective, label: `Objective: ${lesson.objective}` });
    }
    lesson?.blocks?.forEach((b, idx) => {
      if (b.title) {
        options.push({ id: b.title, label: `Block ${idx + 1}: ${b.title}` });
      }
    });
    module?.outcomes?.forEach((outcome, idx) => {
      options.push({ id: outcome, label: `Outcome ${idx + 1}: ${outcome}` });
    });
    return options;
  }, [lesson, module]);

  const handleCopyExplanation = async () => {
    const textToCopy = [
      `# Feynman Explanation: ${activeConcept}`,
      '',
      '## Explanation for a Beginner',
      explanation || '(No explanation provided yet)',
      '',
      '## Everyday Analogy',
      analogy || '(No analogy provided yet)',
      '',
      `Clarity Rating: ${analysis.rating}`,
    ].join('\n');

    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // fallback
    }
  };

  return (
    <div className="space-y-6">
      {/* Header and Feynman Technique Guidance */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="max-w-2xl">
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
              <Sparkles size={14} aria-hidden="true" />
              <span>The Feynman Technique · Teach a Beginner</span>
            </div>
            <h3 className="mt-1 font-serif text-2xl text-text-primary">
              Teach it from memory, then check it
            </h3>
            <p className="mt-2 text-xs leading-5 text-text-secondary">
              Explain the concept out loud or in writing, in everyday language, as if teaching a complete beginner, without
              looking at the lesson. Then compare with the lesson: what you left out or got wrong is what to study next, and
              explaining it again from memory after a day is what makes it last.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={handleAsyncEvent(handleCopyExplanation)}
              className="inline-flex items-center gap-1.5 rounded-full border border-border-default bg-surface-raised px-3.5 py-1.5 text-xs font-medium text-text-primary hover:bg-surface-sunken"
            >
              {copied ? <Check size={13} className="text-accent" /> : <Copy size={13} />}
              <span>{copied ? 'Copied' : 'Copy explanation'}</span>
            </button>
            <span className="inline-flex items-center gap-1.5 text-xs text-text-muted">
              <Check size={13} className="text-success-fg" aria-hidden="true" /> Draft kept on this device
            </span>
          </div>
        </div>

        {/* Step 1: Choose Concept to Teach */}
        <div className="mt-6 rounded-xl border border-border-subtle bg-surface-sunken/40 p-4">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-semibold uppercase tracking-wider text-text-tertiary">
              Step 1: Choose what to explain
            </span>
            <span className="rounded-full bg-accent-muted px-2.5 py-0.5 text-xs font-medium text-accent">
              Target Concept
            </span>
          </div>

          <div className="mt-3 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
            <div>
              <label htmlFor="feynman-concept-select" className="sr-only">
                Choose concept to teach
              </label>
              <select
                id="feynman-concept-select"
                value={isCustom ? '__custom__' : selectedConcept}
                onChange={(e) => {
                  if (e.target.value === '__custom__') {
                    setIsCustom(true);
                  } else {
                    setIsCustom(false);
                    setSelectedConcept(e.target.value);
                  }
                  setComparison(null);
                }}
                className="w-full rounded-lg border border-border-default bg-surface px-3 py-2 text-xs text-text-primary outline-hidden focus:border-accent focus:ring-1 focus:ring-ring"
              >
                {conceptOptions.map((opt) => (
                  <option key={opt.id} value={opt.id}>
                    {opt.label}
                  </option>
                ))}
                <option value="__custom__">+ Enter custom concept…</option>
              </select>
            </div>

            {isCustom && (
              <input
                type="text"
                value={customConcept}
                onChange={(e) => {
                  setCustomConcept(e.target.value);
                  setComparison(null);
                }}
                placeholder="Enter specific concept or idea to explain"
                className="w-full rounded-lg border border-border-default bg-surface px-3 py-2 text-xs text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent focus:ring-1 focus:ring-ring sm:w-64"
              />
            )}
          </div>

          <p className="mt-2 text-xs text-text-muted">
            Teaching target: <strong className="text-text-primary">{activeConcept || 'None selected'}</strong>
          </p>
        </div>
      </section>

      {/* Step 2: The Plain-Language Explanation */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
              <Lightbulb size={14} aria-hidden="true" />
              <span>Step 2: Explain to a complete beginner</span>
            </div>
            <h4 className="mt-1 font-serif text-lg text-text-primary">
              Use your own words, everyday terms, and zero unexplained jargon
            </h4>
          </div>
          <div className="text-right">
            <span className="text-xs text-text-muted">{analysis.wordCount} words</span>
          </div>
        </div>

        <div className="mt-4 space-y-4">
          <div>
            <label htmlFor="feynman-explanation-input" className="block text-xs font-medium text-text-secondary">
              Your plain-language explanation
            </label>
            <p className="mt-0.5 text-xs text-text-muted">
              Pretend you are explaining this to a smart 12-year-old or someone completely new to this topic.
            </p>
            <textarea
              id="feynman-explanation-input"
              rows={6}
              value={explanation}
              readOnly={compared}
              onChange={(e) => setExplanation(e.target.value)}
              placeholder="Start with: 'Think of it as...' or 'In simple terms, this is how it works...'"
              className="mt-2 w-full rounded-xl border border-border-default bg-surface px-4 py-3 text-sm leading-6 text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent focus:ring-1 focus:ring-ring read-only:bg-surface-sunken"
            />
          </div>

          <div>
            <label htmlFor="feynman-analogy-input" className="block text-xs font-medium text-text-secondary">
              Everyday Analogy (Required for genuine simplicity)
            </label>
            <p className="mt-0.5 text-xs text-text-muted">
              What daily life object, scenario, or mechanism works just like this?
            </p>
            <input
              id="feynman-analogy-input"
              type="text"
              value={analogy}
              readOnly={compared}
              onChange={(e) => setAnalogy(e.target.value)}
              placeholder="e.g. 'It works like a restaurant kitchen ticket wheel because...'"
              className="mt-1.5 w-full rounded-lg border border-border-default bg-surface px-3.5 py-2 text-xs text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent focus:ring-1 focus:ring-ring read-only:bg-surface-sunken"
            />
          </div>

          <div className="rounded-xl border border-border-subtle bg-surface-sunken/40 p-3">
            <label className="flex cursor-pointer items-start gap-3 text-xs">
              <input
                type="checkbox"
                aria-label="Speak out loud test"
                checked={readAloudConfirmed}
                onChange={(e) => setReadAloud(e.target.checked ? 'yes' : '')}
                className="mt-0.5 accent-accent"
              />
              <div>
                <span className="flex items-center gap-1.5 font-medium text-text-primary">
                  <Volume2 size={13} className="text-accent" aria-hidden="true" />
                  &ldquo;Speak out loud&rdquo; test
                </span>
                <span className="block text-xs text-text-muted">
                  I read my explanation out loud to hear where I stumbled or hesitated. Stumbling out loud reveals where your mental model is still fuzzy.
                </span>
              </div>
            </label>
          </div>
        </div>
      </section>

      {/* Step 3: Check it against the lesson, only after explaining from memory */}
      <section aria-labelledby="feynman-compare-heading" className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
          {compared ? <Unlock size={14} aria-hidden="true" /> : <Lock size={14} aria-hidden="true" />}
          <span id="feynman-compare-heading">Step 3: Check it against the lesson</span>
        </div>
        {!compared ? (
          <>
            <p className="mt-1 text-xs text-text-muted">
              Once you have explained it, open what the lesson says and tick each point your explanation got right. The rest
              are your gaps.
            </p>
            {points.length > 0 ? (
              <button
                type="button"
                disabled={!explanation.trim()}
                onClick={() => setComparison({ concept: activeConcept, covered: [], drafted: [] })}
                className="mt-3 inline-flex items-center gap-1.5 rounded-full bg-accent px-3.5 py-1.5 text-xs font-semibold text-accent-fg hover:opacity-95 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Unlock size={13} aria-hidden="true" /> Compare with the lesson
              </button>
            ) : (
              <p className="mt-3 text-xs italic text-text-muted">This concept has no lesson material to compare with yet.</p>
            )}
          </>
        ) : (
          <>
            <p className="mt-1 text-xs text-text-muted" role="status">
              Tick each point your explanation got right, in plain words: {coveredCount} of {points.length} covered.
            </p>
            <ul className="mt-3 space-y-2">
              {points.map((point, index) => {
                const covered = comparison.covered.includes(index);
                const drafted = comparison.drafted.includes(index);
                return (
                  <li key={`${index}:${point.title}`} className="rounded-xl border border-border-subtle bg-surface-sunken/40 p-3">
                    <label className="flex cursor-pointer items-start gap-3 text-sm text-text-primary">
                      <input type="checkbox" aria-label={point.title} checked={covered} onChange={() => toggleCovered(index)} className="mt-1 accent-accent" />
                      <CardText text={point.title} />
                    </label>
                    {point.body && (
                      <details className="mt-2 pl-7" open={points.length === 1}>
                        <summary className="cursor-pointer text-xs text-text-secondary">What the lesson says</summary>
                        <CardText className="mt-2 text-xs leading-5 text-text-secondary" text={point.body} />
                      </details>
                    )}
                    {!covered && (
                      <div className="mt-2">
                        {reviewCardPointIndex === index ? (
                          <SaveAsReviewCard
                            programId={program.summary.id}
                            front={point.title}
                            back={point.body.trim() || point.title}
                            sourceLabel="Drafted from a gap found using the Feynman technique"
                          />
                        ) : (
                          <div className="flex justify-end">
                            <button
                              type="button"
                              disabled={drafted}
                              onClick={() => draftCard(index)}
                              className="inline-flex items-center gap-1 rounded-full border border-border-default bg-surface-raised px-2.5 py-1 text-xs font-medium text-accent hover:bg-accent hover:text-accent-fg disabled:cursor-default disabled:opacity-70"
                            >
                              {drafted ? (
                                <>
                                  <CheckCircle2 size={12} aria-hidden="true" /> Added to recall cards
                                </>
                              ) : (
                                <>
                                  <PlusCircle size={12} aria-hidden="true" /> Make a recall card
                                </>
                              )}
                            </button>
                          </div>
                        )}
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={() => {
                  setComparison(null);
                  setReviewCardPointIndex(null);
                }}
                className="inline-flex items-center gap-1.5 rounded-full border border-border-default bg-surface-sunken px-3.5 py-1.5 text-xs font-medium text-text-primary hover:border-accent/40"
              >
                <RotateCcw size={13} aria-hidden="true" /> Explain it again from memory
              </button>
              <span className="text-xs leading-4 text-text-muted">
                Close the lesson and fill your gaps from memory. Explaining it once more tomorrow, before you look, is what makes it last.
              </span>
            </div>
          </>
        )}
      </section>

      {/* Real-time Clarity & Jargon Diagnostics */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle pb-4">
          <div>
            <h4 className="font-serif text-lg text-text-primary">Diagnostic Checks</h4>
            <p className="text-xs text-text-muted">Real-time heuristics testing clarity, density, and jargon.</p>
          </div>
          <div>
            <span
              className={
                analysis.rating === 'Simple & Clear'
                  ? 'rounded-full bg-success-muted px-3 py-1 text-xs font-semibold text-success-fg'
                  : analysis.rating === 'Has Unexplained Jargon'
                    ? 'rounded-full bg-warning-muted px-3 py-1 text-xs font-semibold text-warning-fg'
                    : 'rounded-full bg-danger-muted px-3 py-1 text-xs font-semibold text-danger-fg'
              }
            >
              {analysis.rating}
            </span>
          </div>
        </div>

        <div className="mt-4 space-y-4">
          {/* Flagged Jargon Alert */}
          {analysis.flaggedJargon.length > 0 && (
            <div className="rounded-xl border border-warning-muted bg-warning-muted/20 p-4 text-xs">
              <div className="flex items-center gap-2 font-semibold text-warning-fg">
                <AlertCircle size={14} aria-hidden="true" />
                <span>Unexplained domain jargon detected:</span>
              </div>
              <p className="mt-1 text-text-secondary">
                You used the following terms without defining them using plain-language cues (such as &ldquo;which means&rdquo;, &ldquo;for example&rdquo;, or &ldquo;like a&rdquo;):
              </p>
              <div className="mt-2.5 flex flex-wrap gap-1.5">
                {analysis.flaggedJargon.map((term) => (
                  <span
                    key={term}
                    className="rounded-md border border-warning-muted bg-surface px-2 py-0.5 font-mono text-xs text-text-primary"
                  >
                    {term}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Dense Sentences Alert */}
          {analysis.denseSentences.length > 0 && (
            <div className="rounded-xl border border-warning-muted bg-warning-muted/20 p-4 text-xs">
              <div className="flex items-center gap-2 font-semibold text-warning-fg">
                <AlertCircle size={14} aria-hidden="true" />
                <span>Overly dense sentences (&gt; 28 words):</span>
              </div>
              <p className="mt-1 text-text-secondary">
                Long sentences make it difficult for beginners to follow. Try breaking these into two simpler sentences:
              </p>
              <ul className="mt-2 space-y-1.5 border-l-2 border-warning-muted pl-3 italic text-text-secondary">
                {analysis.denseSentences.map((sentence) => (
                  <li key={sentence}>&ldquo;{sentence}&rdquo;</li>
                ))}
              </ul>
            </div>
          )}

          {/* Verbatim Copied Phrasing Alert */}
          {analysis.verbatimPhrases.length > 0 && analysis.verbatimOverlapPercent > 20 && (
            <div className="rounded-xl border border-danger-muted bg-danger-muted/20 p-4 text-xs">
              <div className="flex items-center gap-2 font-semibold text-danger-fg">
                <AlertCircle size={14} aria-hidden="true" />
                <span>Textbook phrasing detected ({analysis.verbatimOverlapPercent}% verbatim overlap):</span>
              </div>
              <p className="mt-1 text-text-secondary">
                You are copying textbook sentences directly. Rephrasing into your own words forces genuine understanding:
              </p>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {analysis.verbatimPhrases.slice(0, 3).map((phrase) => (
                  <span
                    key={phrase}
                    className="rounded-md border border-danger-muted bg-surface px-2 py-0.5 text-xs text-text-secondary"
                  >
                    &ldquo;{phrase}&rdquo;
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Beginner Follow-Up Questions */}
          {analysis.beginnerQuestions.length > 0 && (
            <div className="rounded-xl border border-border-default bg-surface-sunken/40 p-4">
              <div className="flex items-center gap-2 text-xs font-semibold text-text-primary">
                <HelpCircle size={14} className="text-accent" aria-hidden="true" />
                <span>Beginner Follow-up Questions (Gaps in explanation)</span>
              </div>
              <p className="mt-1 text-xs text-text-muted">
                A curious beginner reading your explanation would ask:
              </p>
              <ul className="mt-3 space-y-2">
                {analysis.beginnerQuestions.map((q) => (
                  <li
                    key={q}
                    className="flex items-start gap-2.5 rounded-lg border border-border-subtle bg-surface p-2.5 text-xs text-text-primary"
                  >
                    <span className="mt-0.5 grid h-4 w-4 shrink-0 place-items-center rounded-full bg-accent/15 text-xs font-bold text-accent">
                      ?
                    </span>
                    <span className="leading-5">{q}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

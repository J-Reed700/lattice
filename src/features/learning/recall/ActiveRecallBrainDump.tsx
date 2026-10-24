import { useCallback, useEffect, useId, useMemo, useState } from 'react';

import {
  Brain,
  CheckCircle2,
  ExternalLink,
  Lock,
  Pause,
  Play,
  PlusCircle,
  RotateCcw,
  Sparkles,
  Unlock,
} from 'lucide-react';

import type {
  LearningLessonDto,
  LearningProgramDto,
} from '@/lib/bindings';
import { cn } from '@/lib/utils';

export interface ActiveRecallBrainDumpProps {
  program: LearningProgramDto;
  lesson?: LearningLessonDto;
  onDraftMissedConcept?: (question: string, answer: string) => void;
}

interface ConceptCheckpoint {
  id: string;
  title: string;
  explanation: string;
  sourceLabel: string;
  keywords: string[];
}

const STOP_WORDS = new Set([
  'about', 'above', 'after', 'again', 'against', 'also', 'because', 'before', 'being', 'between',
  'both', 'could', 'does', 'doing', 'down', 'during', 'each', 'from', 'further', 'have', 'having',
  'here', 'into', 'just', 'more', 'most', 'only', 'other', 'over', 'same', 'should', 'some',
  'such', 'than', 'that', 'their', 'them', 'then', 'there', 'these', 'they', 'this', 'those',
  'through', 'under', 'until', 'very', 'what', 'when', 'where', 'which', 'while', 'with', 'would',
  'your', 'will', 'been', 'were',
]);

function extractKeywords(text: string): string[] {
  const matches = text.toLowerCase().match(/\b[a-z]{4,}\b/g) || [];
  return Array.from(new Set(matches.filter((word) => !STOP_WORDS.has(word))));
}

export function ActiveRecallBrainDump({
  program,
  lesson,
  onDraftMissedConcept,
}: ActiveRecallBrainDumpProps) {
  const textareaId = useId();
  const storageKey = `active-recall-dump:${program.summary.id}:${lesson?.id ?? 'course'}`;

  const [dumpText, setDumpText] = useState(() => {
    try {
      return localStorage.getItem(storageKey) ?? '';
    } catch {
      return '';
    }
  });
  const [revealed, setRevealed] = useState(false);
  const [timerSeconds, setTimerSeconds] = useState(0);
  const [timerRunning, setTimerRunning] = useState(false);
  const [copiedConceptId, setCopiedConceptId] = useState<string | null>(null);

  // Sync to localStorage
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, dumpText);
    } catch {
      // LocalStorage unavailable
    }
  }, [storageKey, dumpText]);

  // Self-test timer
  useEffect(() => {
    if (!timerRunning) return;
    const interval = setInterval(() => {
      setTimerSeconds((prev) => prev + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [timerRunning]);

  const wordCount = useMemo(() => {
    const trimmed = dumpText.trim();
    if (!trimmed) return 0;
    return trimmed.split(/\s+/).filter(Boolean).length;
  }, [dumpText]);

  // Extract checkpoints from lesson or program
  const checkpoints = useMemo<ConceptCheckpoint[]>(() => {
    const list: ConceptCheckpoint[] = [];

    if (lesson) {
      if (lesson.objective) {
        list.push({
          id: `${lesson.id}-objective`,
          title: lesson.title ? `${lesson.title} Objective` : 'Lesson Objective',
          explanation: lesson.objective,
          sourceLabel: 'Objective',
          keywords: extractKeywords(lesson.objective),
        });
      }

      lesson.blocks.forEach((block, index) => {
        const title = block.title || `Block ${index + 1}`;
        list.push({
          id: `${lesson.id}-block-${index}`,
          title,
          explanation: block.body,
          sourceLabel: block.kind.replace('_', ' '),
          keywords: extractKeywords(`${title} ${block.body}`),
        });
      });

      lesson.questions.forEach((question, index) => {
        const explanation = question.options.length > 0 ? question.options.join('; ') : question.prompt;
        list.push({
          id: `${lesson.id}-q-${question.id || index}`,
          title: question.prompt,
          explanation,
          sourceLabel: 'Review Question',
          keywords: extractKeywords(question.prompt),
        });
      });
    } else {
      if (program.summary.goal) {
        list.push({
          id: `${program.summary.id}-goal`,
          title: 'Program Goal',
          explanation: program.summary.goal,
          sourceLabel: 'Program Goal',
          keywords: extractKeywords(program.summary.goal),
        });
      }

      program.modules.forEach((mod, modIdx) => {
        list.push({
          id: `${mod.id || modIdx}-summary`,
          title: mod.title,
          explanation: mod.summary,
          sourceLabel: 'Module',
          keywords: extractKeywords(`${mod.title} ${mod.summary}`),
        });

        mod.outcomes.forEach((outcome, outIdx) => {
          list.push({
            id: `${mod.id || modIdx}-outcome-${outIdx}`,
            title: `Outcome: ${outcome}`,
            explanation: outcome,
            sourceLabel: 'Outcome',
            keywords: extractKeywords(outcome),
          });
        });
      });
    }

    return list;
  }, [lesson, program]);

  // Evaluate coverage
  const evaluation = useMemo(() => {
    const dumpLower = dumpText.toLowerCase();

    const covered: ConceptCheckpoint[] = [];
    const missed: ConceptCheckpoint[] = [];

    checkpoints.forEach((cp) => {
      if (cp.keywords.length === 0) {
        if (dumpLower.length > 20) {
          covered.push(cp);
        } else {
          missed.push(cp);
        }
        return;
      }

      const matches = cp.keywords.filter((kw) => dumpLower.includes(kw));
      // Covered if user matched at least 1 keyword when keywords <= 2, or >= 2 when keywords > 2
      const requiredMatches = cp.keywords.length <= 2 ? 1 : Math.min(2, cp.keywords.length);
      const isCovered = matches.length >= requiredMatches || dumpLower.includes(cp.title.toLowerCase());

      if (isCovered) {
        covered.push(cp);
      } else {
        missed.push(cp);
      }
    });

    const percent = checkpoints.length > 0 ? Math.round((covered.length / checkpoints.length) * 100) : 0;

    return { covered, missed, percent };
  }, [checkpoints, dumpText]);

  const handleTurnIntoCard = useCallback((concept: ConceptCheckpoint) => {
    if (onDraftMissedConcept) {
      onDraftMissedConcept(concept.title, concept.explanation);
    }
    setCopiedConceptId(concept.id);
    setTimeout(() => {
      setCopiedConceptId(null);
    }, 2500);
  }, [onDraftMissedConcept]);

  const formatTimer = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  const handleReset = () => {
    if (window.confirm('Clear your brain dump for this lesson?')) {
      setDumpText('');
      setRevealed(false);
      setTimerSeconds(0);
      setTimerRunning(false);
      try {
        localStorage.removeItem(storageKey);
      } catch {
        // LocalStorage unavailable
      }
    }
  };

  return (
    <section
      aria-label="Active Recall Brain Dump"
      className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6"
    >
      {/* Evidence-based Header */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[.14em] text-accent">
            <Brain size={13} />
            <span>Active Recall · Testing Yourself</span>
          </div>
          <h3 className="mt-1 font-serif text-xl font-medium text-text-primary">
            Close your books &amp; write everything from memory
          </h3>
        </div>

        <div className="flex items-center gap-2">
          {/* Notes-closed Lock Indicator */}
          <span
            className={cn(
              'inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium transition-colors',
              revealed
                ? 'border border-warning-muted bg-warning-muted/40 text-warning-fg'
                : 'border border-accent/40 bg-accent/10 text-accent',
            )}
          >
            {revealed ? <Unlock size={12} /> : <Lock size={12} />}
            <span>{revealed ? 'Lesson revealed & compared' : 'Notes-closed lock active'}</span>
          </span>

          {/* Self-test Timer */}
          <div className="flex items-center gap-1.5 rounded-full border border-border-default bg-surface-sunken px-3 py-1 text-xs font-mono text-text-secondary">
            <span>{formatTimer(timerSeconds)}</span>
            <button
              type="button"
              aria-label={timerRunning ? 'Pause timer' : 'Start timer'}
              onClick={() => setTimerRunning((r) => !r)}
              className="text-text-muted transition hover:text-text-primary"
            >
              {timerRunning ? <Pause size={12} /> : <Play size={12} />}
            </button>
          </div>
        </div>
      </div>

      <p className="mt-3 text-xs leading-relaxed text-text-secondary">
        Close your notes and write down or speak everything you remember from scratch. Studies show students who test themselves retain roughly 80% of material compared to just 34% for those who merely reread notes.
      </p>

      {/* Lock Notice */}
      {!revealed && (
        <div className="mt-4 flex items-center gap-2 rounded-xl border border-accent/20 bg-accent/5 p-3 text-xs text-text-secondary">
          <Lock size={14} className="shrink-0 text-accent" />
          <span>
            <strong>Notes-Closed Lock:</strong> Lesson text and source notes are concealed so you must retrieve concepts purely from memory. Write as much as you can recall before comparing.
          </span>
        </div>
      )}

      {/* Brain Dump Input Area */}
      <div className="mt-4">
        <div className="flex items-center justify-between text-xs text-text-muted">
          <label htmlFor={textareaId} className="font-medium text-text-secondary">
            Your free-recall memory dump
          </label>
          <span>
            {wordCount} {wordCount === 1 ? 'word' : 'words'}
          </span>
        </div>

        <textarea
          id={textareaId}
          value={dumpText}
          onChange={(e) => setDumpText(e.target.value)}
          rows={6}
          disabled={revealed}
          placeholder="Without checking notes, write down key concepts, definitions, formulas, or steps you learned..."
          className={cn(
            'mt-2 block w-full rounded-xl border border-border-default bg-surface p-3.5 text-sm leading-relaxed text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-hidden',
            revealed && 'bg-surface-sunken opacity-80',
          )}
        />
      </div>

      {/* Action Buttons */}
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {!revealed ? (
            <button
              type="button"
              disabled={wordCount === 0}
              onClick={() => {
                setRevealed(true);
                setTimerRunning(false);
              }}
              className="inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg transition hover:opacity-90 disabled:opacity-50"
            >
              <Sparkles size={13} /> Compare with Lesson &amp; Check Recall
            </button>
          ) : (
            <button
              type="button"
              onClick={() => setRevealed(false)}
              className="inline-flex items-center gap-1.5 rounded-full border border-border-default bg-surface px-4 py-2 text-xs font-medium text-text-secondary transition hover:bg-surface-raised"
            >
              <Lock size={13} /> Continue writing dump
            </button>
          )}

          {dumpText && (
            <button
              type="button"
              onClick={handleReset}
              className="inline-flex items-center gap-1 text-xs text-text-muted transition hover:text-text-primary"
            >
              <RotateCcw size={12} /> Clear
            </button>
          )}
        </div>

        <span className="text-[11px] text-text-muted">
          Persisted locally for {lesson?.title ? `“${lesson.title}”` : 'this program'}
        </span>
      </div>

      {/* Revealed Comparison & Coverage */}
      {revealed && (
        <div className="mt-6 border-t border-border-subtle pt-6">
          {/* Coverage Summary */}
          <div className="rounded-xl border border-border-default bg-surface-sunken/60 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <span className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">
                  Recall Coverage
                </span>
                <h4 className="font-serif text-lg text-text-primary">
                  {evaluation.covered.length} of {checkpoints.length} key concepts recalled ({evaluation.percent}%)
                </h4>
              </div>
              <span
                className={cn(
                  'rounded-full px-3 py-1 text-xs font-semibold',
                  evaluation.percent >= 70
                    ? 'bg-success-muted text-success-fg'
                    : evaluation.percent >= 40
                    ? 'bg-warning-muted text-warning-fg'
                    : 'bg-danger-muted text-danger-fg',
                )}
              >
                {evaluation.percent >= 80 ? 'Strong Recall (Target: ~80%)' : evaluation.percent >= 50 ? 'Developing Retention' : 'Needs Review'}
              </span>
            </div>

            {/* Coverage Progress Bar */}
            <div className="mt-3 h-2 overflow-hidden rounded-full bg-surface-raised">
              <div
                className="h-full bg-accent transition-all duration-500"
                style={{ width: `${evaluation.percent}%` }}
              />
            </div>
          </div>

          {/* Recalled vs Gaps Grid */}
          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            {/* Recalled Concepts */}
            <div className="rounded-xl border border-success-muted/50 bg-success-muted/10 p-4">
              <div className="flex items-center gap-2">
                <CheckCircle2 size={16} className="text-success-fg" />
                <h5 className="font-serif text-sm font-semibold text-text-primary">
                  Recalled Concepts ({evaluation.covered.length})
                </h5>
              </div>
              <p className="mt-1 text-[11px] text-text-muted">
                These concepts were detected in your free-recall memory dump.
              </p>

              {evaluation.covered.length > 0 ? (
                <ul className="mt-3 space-y-2">
                  {evaluation.covered.map((concept) => (
                    <li
                      key={concept.id}
                      className="rounded-lg border border-border-subtle bg-surface p-2.5 text-xs"
                    >
                      <div className="flex items-center justify-between gap-1 text-[10px] text-text-muted">
                        <span className="font-medium text-text-primary">{concept.title}</span>
                        <span className="rounded bg-surface-sunken px-1.5 py-0.5">{concept.sourceLabel}</span>
                      </div>
                      <p className="mt-1 line-clamp-2 text-text-secondary">{concept.explanation}</p>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="mt-3 text-xs italic text-text-muted">
                  No concepts detected yet. Try elaborating or writing more key terms.
                </p>
              )}
            </div>

            {/* Gaps to Revisit */}
            <div className="rounded-xl border border-warning-muted/50 bg-warning-muted/10 p-4">
              <div className="flex items-center gap-2">
                <ExternalLink size={16} className="text-warning-fg" />
                <h5 className="font-serif text-sm font-semibold text-text-primary">
                  Gaps to Revisit ({evaluation.missed.length})
                </h5>
              </div>
              <p className="mt-1 text-[11px] text-text-muted">
                Concepts missing from your dump. Turn them into recall cards for high-retention practice.
              </p>

              {evaluation.missed.length > 0 ? (
                <ul className="mt-3 space-y-2">
                  {evaluation.missed.map((concept) => (
                    <li
                      key={concept.id}
                      className="rounded-lg border border-border-subtle bg-surface p-2.5 text-xs"
                    >
                      <div className="flex items-center justify-between gap-1 text-[10px] text-text-muted">
                        <span className="font-medium text-text-primary">{concept.title}</span>
                        <span className="rounded bg-surface-sunken px-1.5 py-0.5">{concept.sourceLabel}</span>
                      </div>
                      <p className="mt-1 text-text-secondary">{concept.explanation}</p>

                      <div className="mt-2 flex items-center justify-end">
                        <button
                          type="button"
                          onClick={() => handleTurnIntoCard(concept)}
                          className="inline-flex items-center gap-1 rounded-full border border-border-default bg-surface-raised px-2.5 py-1 text-[11px] font-medium text-accent transition hover:bg-accent hover:text-accent-fg"
                        >
                          {copiedConceptId === concept.id ? (
                            <>
                              <CheckCircle2 size={12} /> Added to cards
                            </>
                          ) : (
                            <>
                              <PlusCircle size={12} /> Turn missed concept into recall card
                            </>
                          )}
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              ) : (
                <div className="mt-3 rounded-lg border border-dashed border-success-muted bg-surface p-4 text-center">
                  <Sparkles size={16} className="mx-auto text-success-fg" />
                  <p className="mt-1 text-xs font-medium text-text-primary">Complete Recall!</p>
                  <p className="text-[11px] text-text-muted">
                    You recalled every key concept from memory alone.
                  </p>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

import { useEffect, useMemo, useState } from 'react';

import {
  AlertCircle,
  BookOpen,
  Check,
  CheckCircle2,
  Copy,
  HelpCircle,
  Lightbulb,
  Sparkles,
  Volume2,
} from 'lucide-react';

import type { LearningLessonDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

export type FeynmanRating = 'Simple & Clear' | 'Has Unexplained Jargon' | 'Needs More Explanation';

export interface FeynmanAnalysis {
  rating: FeynmanRating;
  wordCount: number;
  jargonTerms: string[];
  flaggedJargon: string[];
  denseSentences: string[];
  verbatimPhrases: string[];
  verbatimOverlapPercent: number;
  beginnerQuestions: string[];
  hasAnalogy: boolean;
}

export interface FeynmanTechniquePanelProps {
  program: LearningProgramDto;
  lesson?: LearningLessonDto;
  module?: LearningModuleDto;
}

const COMMON_STOP_WORDS = new Set([
  'about', 'above', 'across', 'after', 'against', 'almost', 'along', 'already', 'also',
  'although', 'always', 'among', 'another', 'anyone', 'anything', 'around', 'because',
  'become', 'before', 'behind', 'between', 'beyond', 'cannot', 'certain', 'clearly',
  'could', 'during', 'either', 'enough', 'especially', 'every', 'everyone', 'everything',
  'everywhere', 'example', 'explain', 'explanation', 'further', 'general', 'however',
  'important', 'include', 'includes', 'including', 'inside', 'instead', 'itself',
  'learning', 'little', 'means', 'meaning', 'might', 'mostly', 'myself', 'neither',
  'never', 'nothing', 'outside', 'people', 'perhaps', 'possible', 'probably', 'rather',
  'really', 'remember', 'several', 'should', 'simple', 'simply', 'someone', 'something',
  'sometimes', 'somewhere', 'still', 'student', 'teach', 'teacher', 'teaching',
  'their', 'theirs', 'themselves', 'there', 'therefore', 'these', 'think', 'thinking',
  'those', 'though', 'thought', 'through', 'together', 'toward', 'towards',
  'understand', 'understanding', 'understands', 'usually', 'without', 'worked', 'working',
  'would', 'concept', 'concepts', 'lesson', 'lessons', 'module', 'modules', 'program',
  'section', 'sections', 'practice', 'detail', 'details', 'different', 'difference',
]);

const EXPLANATION_CUE_PHRASES = [
  'which means',
  'in other words',
  'like a',
  'like an',
  'for example',
  'because',
  'meaning that',
  'think of it as',
  'is basically',
  'simply put',
  'that is',
  'meaning',
  'analogous to',
  'similar to',
  'just like',
  'as if',
  'it works like',
];

function extractTechnicalTerms(lesson?: LearningLessonDto | null, targetConcept?: string): string[] {
  const textCorpus = [
    targetConcept ?? '',
    lesson?.title ?? '',
    lesson?.objective ?? '',
    ...(lesson?.blocks?.map((b) => `${b.title} ${b.body}`) ?? []),
  ].join(' ');

  const words = textCorpus.match(/\b[A-Za-z][A-Za-z0-9_-]{2,}\b/g) ?? [];
  const found = new Set<string>();

  for (const raw of words) {
    const lower = raw.toLowerCase();
    if (COMMON_STOP_WORDS.has(lower)) continue;

    const isAcronym = /^[A-Z]{2,6}$/.test(raw);
    const isLongWord = raw.length >= 7;
    const isCapitalized = /^[A-Z][a-z]+$/.test(raw) && raw.length >= 5;

    if (isAcronym || isLongWord || isCapitalized) {
      found.add(lower);
    }
  }

  return Array.from(found);
}

function hasPlainLanguageCueNear(explanation: string, term: string): boolean {
  const lowerExp = explanation.toLowerCase();
  const termIndex = lowerExp.indexOf(term.toLowerCase());
  if (termIndex === -1) return true;

  const windowStart = Math.max(0, termIndex - 90);
  const windowEnd = Math.min(lowerExp.length, termIndex + term.length + 110);
  const localSnippet = lowerExp.slice(windowStart, windowEnd);

  return EXPLANATION_CUE_PHRASES.some((cue) => localSnippet.includes(cue));
}

function findDenseSentences(text: string): string[] {
  const sentences = text
    .split(/(?<=[.!?])\s+|\n+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);

  const dense: string[] = [];
  for (const sentence of sentences) {
    const wordCount = sentence.split(/\s+/).filter(Boolean).length;
    if (wordCount > 28) {
      dense.push(sentence);
    }
  }
  return dense;
}

function computeLessonVerbatim(explanation: string, lesson?: LearningLessonDto | null): { percent: number; phrases: string[] } {
  if (!lesson) return { percent: 0, phrases: [] };
  const lessonCorpus = [
    lesson.title,
    lesson.objective,
    ...(lesson.blocks?.map((b) => b.body) ?? []),
  ].join(' ').toLowerCase();

  const expWords = explanation.toLowerCase().replace(/[^a-z0-9\s]/g, ' ').split(/\s+/).filter(Boolean);
  if (expWords.length < 5) return { percent: 0, phrases: [] };

  const matchedPhrases = new Set<string>();
  const matchedWordIndices = new Set<number>();

  for (let i = 0; i <= expWords.length - 5; i++) {
    const ngram = expWords.slice(i, i + 5).join(' ');
    if (lessonCorpus.includes(ngram)) {
      matchedPhrases.add(ngram);
      for (let j = 0; j < 5; j++) {
        matchedWordIndices.add(i + j);
      }
    }
  }

  const percent = Math.min(100, Math.round((matchedWordIndices.size / expWords.length) * 100));
  return { percent, phrases: Array.from(matchedPhrases) };
}

function generateBeginnerQuestions(
  flaggedJargon: string[],
  analogy: string,
  targetConcept: string,
  wordCount: number
): string[] {
  const questions: string[] = [];

  for (const term of flaggedJargon.slice(0, 2)) {
    questions.push(
      `Beginner asks: You mentioned '${term}' — how would you explain what that does using everyday words?`
    );
  }

  if (!analogy.trim()) {
    questions.push(
      "Beginner asks: Can you give an everyday analogy (e.g. 'It works like...') so I can picture it in daily life?"
    );
  }

  if (wordCount > 0 && wordCount < 20) {
    questions.push(
      `Beginner asks: Could you explain step-by-step what actually happens with ${targetConcept || 'this concept'}?`
    );
  } else if (questions.length < 3 && targetConcept) {
    questions.push(
      `Beginner asks: Why does ${targetConcept} matter to someone who is not in this field?`
    );
  }

  return questions;
}

export function analyzeFeynmanExplanation(
  explanation: string,
  analogy = '',
  lesson?: LearningLessonDto | null,
  targetConcept = ''
): FeynmanAnalysis {
  const trimmed = explanation.trim();
  const words = trimmed.length > 0 ? trimmed.split(/\s+/).filter(Boolean) : [];
  const wordCount = words.length;

  const technicalTerms = extractTechnicalTerms(lesson, targetConcept);
  const lowerExp = trimmed.toLowerCase();

  const usedTerms: string[] = [];
  const flaggedJargon: string[] = [];

  for (const term of technicalTerms) {
    const regex = new RegExp(`\\b${term}\\b`, 'i');
    if (regex.test(lowerExp)) {
      usedTerms.push(term);
      if (!hasPlainLanguageCueNear(trimmed, term)) {
        flaggedJargon.push(term);
      }
    }
  }

  const denseSentences = findDenseSentences(trimmed);
  const { percent: verbatimOverlapPercent, phrases: verbatimPhrases } = computeLessonVerbatim(trimmed, lesson);
  const hasAnalogy = Boolean(analogy.trim());

  const conceptToUse = targetConcept || lesson?.title || 'the concept';
  const beginnerQuestions = generateBeginnerQuestions(flaggedJargon, analogy, conceptToUse, wordCount);

  let rating: FeynmanRating;
  if (wordCount < 15) {
    rating = 'Needs More Explanation';
  } else if (flaggedJargon.length > 0 || verbatimOverlapPercent > 20 || denseSentences.length > 0) {
    rating = 'Has Unexplained Jargon';
  } else {
    rating = 'Simple & Clear';
  }

  return {
    rating,
    wordCount,
    jargonTerms: usedTerms,
    flaggedJargon,
    denseSentences,
    verbatimPhrases,
    verbatimOverlapPercent,
    beginnerQuestions,
    hasAnalogy,
  };
}

export function FeynmanTechniquePanel({ program, lesson, module }: FeynmanTechniquePanelProps) {
  const defaultConcept = lesson?.title ?? module?.title ?? program.summary.title;
  const [selectedConcept, setSelectedConcept] = useState<string>(defaultConcept);
  const [customConcept, setCustomConcept] = useState('');
  const [isCustom, setIsCustom] = useState(false);

  const activeConcept = isCustom ? customConcept : selectedConcept;

  const storageKey = `feynman_workbench_${program.summary.id}_${lesson?.id ?? 'general'}`;

  const [explanation, setExplanation] = useState('');
  const [analogy, setAnalogy] = useState('');
  const [readAloudConfirmed, setReadAloudConfirmed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [savedMessage, setSavedMessage] = useState(false);

  useEffect(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = JSON.parse(saved) as { explanation?: string; analogy?: string; readAloud?: boolean; concept?: string };
        if (parsed.explanation) setExplanation(parsed.explanation);
        if (parsed.analogy) setAnalogy(parsed.analogy);
        if (parsed.readAloud) setReadAloudConfirmed(parsed.readAloud);
        if (parsed.concept) setSelectedConcept(parsed.concept);
      }
    } catch {
      // ignore storage error
    }
  }, [storageKey]);

  useEffect(() => {
    try {
      localStorage.setItem(
        storageKey,
        JSON.stringify({
          explanation,
          analogy,
          readAloud: readAloudConfirmed,
          concept: activeConcept,
        })
      );
    } catch {
      // ignore storage error
    }
  }, [activeConcept, analogy, explanation, readAloudConfirmed, storageKey]);

  const analysis = useMemo(
    () => analyzeFeynmanExplanation(explanation, analogy, lesson, activeConcept),
    [explanation, analogy, lesson, activeConcept]
  );

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

  const handleManualSave = () => {
    try {
      localStorage.setItem(
        storageKey,
        JSON.stringify({
          explanation,
          analogy,
          readAloud: readAloudConfirmed,
          concept: activeConcept,
        })
      );
      setSavedMessage(true);
      setTimeout(() => setSavedMessage(false), 2000);
    } catch {
      // ignore
    }
  };

  return (
    <div className="space-y-6">
      {/* Header and Feynman Technique Guidance */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="max-w-2xl">
            <div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.16em] text-accent">
              <Sparkles size={14} />
              <span>The Feynman Technique · Teach a Beginner</span>
            </div>
            <h3 className="mt-1 font-serif text-2xl text-text-primary">
              Learn by Teaching in Simple Language
            </h3>
            <p className="mt-2 text-xs leading-5 text-text-secondary">
              Explain the concept out loud or in writing in simple, everyday language as if teaching a complete beginner.
              If you stumble or rely on jargon, it highlights the exact gaps in your understanding.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={handleAsyncEvent(handleCopyExplanation)}
              disabled={!explanation.trim()}
              className="inline-flex items-center gap-1.5 rounded-full border border-border-default bg-surface-sunken px-3.5 py-1.5 text-xs font-medium text-text-primary hover:border-accent/40 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {copied ? <Check size={13} className="text-success-fg" /> : <Copy size={13} />}
              {copied ? 'Copied' : 'Copy explanation'}
            </button>
            <button
              type="button"
              onClick={handleManualSave}
              className="inline-flex items-center gap-1.5 rounded-full bg-accent px-3.5 py-1.5 text-xs font-semibold text-accent-fg hover:opacity-95"
            >
              {savedMessage ? <Check size={13} /> : <CheckCircle2 size={13} />}
              {savedMessage ? 'Saved' : 'Save draft'}
            </button>
          </div>
        </div>
      </section>

      {/* Step 1: Choose the Concept */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
          <BookOpen size={14} />
          <span>Step 1: Choose the concept to teach</span>
        </div>
        <p className="mt-1 text-xs text-text-muted">
          Pick a focused lesson objective, core block, or custom idea to distill down.
        </p>

        <div className="mt-4 flex flex-wrap items-center gap-3">
          <select
            value={isCustom ? '__custom__' : selectedConcept}
            onChange={(e) => {
              if (e.target.value === '__custom__') {
                setIsCustom(true);
              } else {
                setIsCustom(false);
                setSelectedConcept(e.target.value);
              }
            }}
            aria-label="Choose concept to teach"
            className="rounded-xl border border-border-default bg-surface-sunken px-3.5 py-2 text-xs text-text-primary focus:border-accent focus:outline-hidden"
          >
            {conceptOptions.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {opt.label}
              </option>
            ))}
            <option value="__custom__">Custom concept…</option>
          </select>

          {isCustom && (
            <input
              type="text"
              value={customConcept}
              onChange={(e) => setCustomConcept(e.target.value)}
              placeholder="Enter specific concept or idea to explain…"
              className="min-w-64 flex-1 rounded-xl border border-border-default bg-surface-sunken px-3.5 py-2 text-xs text-text-primary focus:border-accent focus:outline-hidden"
            />
          )}
        </div>

        <div className="mt-3 inline-flex items-center gap-2 rounded-lg bg-surface-sunken px-3 py-1.5 text-xs text-text-secondary">
          <span className="text-[11px] font-medium text-text-muted">Target concept:</span>
          <strong className="text-text-primary">{activeConcept}</strong>
        </div>
      </section>

      {/* Step 2: Explain to a Beginner + Analogy */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
          <Lightbulb size={14} />
          <span>Step 2: Explain to a beginner (Simple, everyday language)</span>
        </div>
        <p className="mt-1 text-xs text-text-muted">
          Imagine explaining this to a curious 12-year-old or someone completely outside the field. Avoid jargon and textbook definitions.
        </p>

        <div className="mt-4 space-y-4">
          <div>
            <div className="flex items-center justify-between pb-1.5">
              <label htmlFor="feynman-explanation" className="text-xs font-medium text-text-primary">
                Your plain-language explanation
              </label>
              <span className="text-[11px] text-text-muted">
                {analysis.wordCount} {analysis.wordCount === 1 ? 'word' : 'words'}
              </span>
            </div>
            <textarea
              id="feynman-explanation"
              rows={6}
              value={explanation}
              onChange={(e) => setExplanation(e.target.value)}
              placeholder="Start from first principles. How would you explain what this is and how it works without assuming any prior jargon? If you must use a specialized word, immediately explain what it means in plain English..."
              className="w-full rounded-xl border border-border-default bg-surface-sunken px-4 py-3 text-sm leading-6 text-text-primary focus:border-accent focus:outline-hidden"
            />
          </div>

          <div>
            <label htmlFor="feynman-analogy" className="block pb-1.5 text-xs font-medium text-text-primary">
              Everyday Analogy (&ldquo;It works like...&rdquo;)
            </label>
            <textarea
              id="feynman-analogy"
              rows={2}
              value={analogy}
              onChange={(e) => setAnalogy(e.target.value)}
              placeholder="It works like a postal worker sorting letters into numbered mailboxes, because..."
              className="w-full rounded-xl border border-border-default bg-surface-sunken px-4 py-2.5 text-xs leading-5 text-text-primary focus:border-accent focus:outline-hidden"
            />
          </div>

          <div className="rounded-xl border border-border-subtle bg-surface-sunken/60 p-3.5">
            <label className="flex cursor-pointer items-start gap-3 text-xs text-text-secondary">
              <input
                type="checkbox"
                aria-label="Speak out loud test"
                checked={readAloudConfirmed}
                onChange={(e) => setReadAloudConfirmed(e.target.checked)}
                className="mt-0.5 accent-accent"
              />
              <div>
                <span className="flex items-center gap-1.5 font-medium text-text-primary">
                  <Volume2 size={13} className="text-accent" />
                  &ldquo;Speak out loud&rdquo; test
                </span>
                <span className="block text-[11px] text-text-muted">
                  I read my explanation out loud to hear where I stumbled or hesitated. Stumbling out loud reveals where your mental model is still fuzzy.
                </span>
              </div>
            </label>
          </div>
        </div>
      </section>

      {/* Step 3: Real-Time Jargon, Verbatim & Gap Detector */}
      <section className="rounded-2xl border border-border-default bg-surface p-5 shadow-xs sm:p-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-accent">
            <HelpCircle size={14} />
            <span>Step 3: Real-time Jargon, Verbatim &amp; Gap Detector</span>
          </div>

          {/* Rating Badge */}
          <div>
            {analysis.rating === 'Simple & Clear' ? (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-success-muted px-3 py-1 text-xs font-semibold text-success-fg">
                <CheckCircle2 size={13} />
                Simple &amp; Clear
              </span>
            ) : analysis.rating === 'Has Unexplained Jargon' ? (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-warning-muted px-3 py-1 text-xs font-semibold text-warning-fg">
                <AlertCircle size={13} />
                Has Unexplained Jargon
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-sunken border border-border-subtle px-3 py-1 text-xs font-semibold text-text-secondary">
                <HelpCircle size={13} />
                Needs More Explanation
              </span>
            )}
          </div>
        </div>

        <div className="mt-4 space-y-4">
          {/* Flagged Jargon Alert */}
          {analysis.flaggedJargon.length > 0 && (
            <div className="rounded-xl border border-warning-muted bg-warning-muted/20 p-4 text-xs">
              <div className="flex items-center gap-2 font-semibold text-warning-fg">
                <AlertCircle size={14} />
                <span>Unexplained domain jargon detected:</span>
              </div>
              <p className="mt-1 text-text-secondary">
                You used the following terms without defining them using plain-language cues (such as &ldquo;which means&rdquo;, &ldquo;for example&rdquo;, or &ldquo;like a&rdquo;):
              </p>
              <div className="mt-2.5 flex flex-wrap gap-1.5">
                {analysis.flaggedJargon.map((term) => (
                  <span
                    key={term}
                    className="rounded-md border border-warning-muted bg-surface px-2 py-0.5 font-mono text-[11px] text-text-primary"
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
                <AlertCircle size={14} />
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
                <AlertCircle size={14} />
                <span>Textbook phrasing detected ({analysis.verbatimOverlapPercent}% verbatim overlap):</span>
              </div>
              <p className="mt-1 text-text-secondary">
                You are copying textbook sentences directly. Rephrasing into your own words forces genuine understanding:
              </p>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {analysis.verbatimPhrases.slice(0, 3).map((phrase) => (
                  <span
                    key={phrase}
                    className="rounded-md border border-danger-muted bg-surface px-2 py-0.5 text-[11px] text-text-secondary"
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
                <HelpCircle size={14} className="text-accent" />
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
                    <span className="mt-0.5 grid h-4 w-4 shrink-0 place-items-center rounded-full bg-accent/15 text-[10px] font-bold text-accent">
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

import { keyPoints } from '@/features/learning/recall/flashcards/retrieval';
import type { LearningBlockKind, LearningLessonDto, LearningModuleDto } from '@/lib/bindings';

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

/** One thing the course says about the concept, to check an explanation against. */
export interface LessonPoint {
  title: string;
  body: string;
}

/** The sections that teach an idea, as opposed to practising or reviewing it. */
const TEACHING_KINDS = new Set<LearningBlockKind>(['explanation', 'worked_example', 'contrast']);

/**
 * What the course says about `concept`: the key points of the section it
 * names (or that section whole), otherwise the lesson's teaching sections,
 * otherwise the module's outcomes. The Feynman technique's third step is
 * going back to the source to find what the explanation missed or got wrong.
 */
export function lessonPoints(concept: string, lesson?: LearningLessonDto | null, module?: LearningModuleDto | null): LessonPoint[] {
  const named = lesson?.blocks.find((block) => block.title.trim() !== '' && block.title.trim() === concept.trim());
  if (named) {
    const items = keyPoints(named.body);
    return items.length ? items.map((item) => ({ title: item, body: '' })) : [{ title: named.title, body: named.body }];
  }
  const teaching = lesson?.blocks.filter((block) => TEACHING_KINDS.has(block.kind)) ?? [];
  if (teaching.length) return teaching.map((block) => ({ title: block.title, body: block.body }));
  return (module?.outcomes ?? []).map((outcome) => ({ title: outcome, body: '' }));
}

export const COMMON_STOP_WORDS = new Set([
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

export const EXPLANATION_CUE_PHRASES = [
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

export function extractTechnicalTerms(lesson?: LearningLessonDto | null, targetConcept?: string): string[] {
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

export function hasPlainLanguageCueNear(explanation: string, term: string): boolean {
  const lowerExp = explanation.toLowerCase();
  const termIndex = lowerExp.indexOf(term.toLowerCase());
  if (termIndex === -1) return true;

  const windowStart = Math.max(0, termIndex - 90);
  const windowEnd = Math.min(lowerExp.length, termIndex + term.length + 110);
  const localSnippet = lowerExp.slice(windowStart, windowEnd);

  return EXPLANATION_CUE_PHRASES.some((cue) => localSnippet.includes(cue));
}

export function findDenseSentences(text: string): string[] {
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

export function computeLessonVerbatim(explanation: string, lesson?: LearningLessonDto | null): { percent: number; phrases: string[] } {
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

export function generateBeginnerQuestions(
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

import { describe, expect, it } from 'vitest';

import { analyzeFeynmanExplanation, lessonPoints } from '@/features/learning/practice/feynmanAnalysis';
import type * as Wire from '@/lib/bindings';

describe('analyzeFeynmanExplanation', () => {
  const sampleLesson: Wire.LearningLessonDto = {
    id: 'lesson-1',
    title: 'Photosynthesis Fundamentals',
    objective: 'Understand how chloroplasts transform solar radiation into glucose',
    estimatedMinutes: 20,
    preparation: 'ready',
    completed: false,
    blocks: [
      {
        kind: 'explanation',
        title: 'The Chloroplast Engine',
        body: 'Inside plant cells, chloroplasts contain chlorophyll molecules that absorb photons and synthesize adenosine triphosphate to produce carbohydrates.',
        sourceIds: ['s1'],
      },
    ],
  };

  it('rates short explanation (< 15 words) as Needs More Explanation', () => {
    const analysis = analyzeFeynmanExplanation('Plants make food from the sun.', '', sampleLesson);
    expect(analysis.rating).toBe('Needs More Explanation');
    expect(analysis.wordCount).toBeLessThan(15);
  });

  it('flags unexplained domain jargon when no plain-language cues are used', () => {
    const explanation =
      'The chloroplasts take photons and synthesize adenosine and carbohydrates without any special tools in the leafy structure of the plant.';
    const analysis = analyzeFeynmanExplanation(explanation, '', sampleLesson);
    expect(analysis.flaggedJargon).toContain('chloroplasts');
    expect(analysis.rating).toBe('Has Unexplained Jargon');
    expect(analysis.beginnerQuestions.some((q) => q.includes('chloroplasts'))).toBe(true);
  });

  it('does not flag jargon when explained using plain-language cues', () => {
    const explanation =
      'Plants have tiny solar factories called chloroplasts, which means tiny chambers inside leaves. Sunlight turns into sweet sugar because the cell needs food.';
    const analysis = analyzeFeynmanExplanation(explanation, 'It works like a solar bakery.', sampleLesson);
    expect(analysis.flaggedJargon).not.toContain('chloroplasts');
    expect(analysis.hasAnalogy).toBe(true);
    expect(analysis.denseSentences).toHaveLength(0);
    expect(analysis.rating).toBe('Simple & Clear');
  });

  it('flags overly dense sentences (> 28 words)', () => {
    const longSentence =
      'When the sun rises in the morning and shines its bright rays onto the green leaves of tall trees standing in the open forest, the cellular machinery inside every single plant begins absorbing each individual packet of light energy and converts it through a sequence of chemical reactions into sweet nourishing fuel.';
    const analysis = analyzeFeynmanExplanation(longSentence, '', sampleLesson);
    expect(analysis.denseSentences[0]).toBe(longSentence);
    expect(analysis.rating).toBe('Has Unexplained Jargon');
  });

  it('detects textbook verbatim overlap against lesson blocks', () => {
    const copiedText =
      'Inside plant cells chloroplasts contain chlorophyll molecules that absorb photons and synthesize adenosine triphosphate to produce carbohydrates in every single leaf on the branch.';
    const analysis = analyzeFeynmanExplanation(copiedText, '', sampleLesson);
    expect(analysis.verbatimOverlapPercent).toBeGreaterThan(20);
    expect(analysis.verbatimPhrases.length).toBeGreaterThan(0);
  });

  it('asks for an analogy when there is none', () => {
    const analysis = analyzeFeynmanExplanation(
      'The leaf uses chloroplasts to trap sunlight and turn it into basic sugars for cell nourishment.',
      '',
      sampleLesson,
      'Photosynthesis Fundamentals',
    );
    expect(analysis.beginnerQuestions.some((q) => q.includes('analogy'))).toBe(true);
  });

  it('does not treat a term with regex characters as a pattern', () => {
    const lesson = { ...sampleLesson, title: 'C++ templates', blocks: [] } as Wire.LearningLessonDto;
    expect(() => analyzeFeynmanExplanation('C++ templates are checked when they are used, not when they are written down.', '', lesson)).not.toThrow();
  });
});

describe('lessonPoints', () => {
  const block = (kind: Wire.LearningLessonDto['blocks'][number]['kind'], title: string, body: string) => ({ kind, title, body, sourceIds: [] });
  const lesson = {
    id: 'l', title: 'Caching', objective: 'Choose an eviction policy', estimatedMinutes: 20, preparation: 'ready', completed: false,
    blocks: [
      block('explanation', 'What eviction is', '- Space is bounded\n- Something must leave when it fills'),
      block('worked_example', 'An LRU cache, step by step', 'Insert A, B, C; touch A; insert D: B leaves.'),
      block('check', 'Your turn', 'Which entry leaves?'),
      block('guided_practice', 'Try it', 'Build one.'),
    ],
  } as Wire.LearningLessonDto;

  it('checks a named section against its key points, or the whole section when it is prose', () => {
    expect(lessonPoints('What eviction is', lesson)).toEqual([
      { title: 'Space is bounded', body: '' },
      { title: 'Something must leave when it fills', body: '' },
    ]);
    expect(lessonPoints('An LRU cache, step by step', lesson)).toEqual([
      { title: 'An LRU cache, step by step', body: 'Insert A, B, C; touch A; insert D: B leaves.' },
    ]);
  });

  it('checks the lesson as a whole against its teaching sections only, and a module against its outcomes', () => {
    expect(lessonPoints('Caching', lesson).map((point) => point.title)).toEqual(['What eviction is', 'An LRU cache, step by step']);
    expect(lessonPoints('Anything', null, { outcomes: ['Pick a policy for a workload'] } as Wire.LearningModuleDto)).toEqual([
      { title: 'Pick a policy for a workload', body: '' },
    ]);
  });
});

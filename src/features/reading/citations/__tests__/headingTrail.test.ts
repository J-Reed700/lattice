import { describe, expect, it } from 'vitest';

import { articleBlocks } from '@/features/reading/model/articleBlocks';

import { headingTrail } from '../headingTrail';

const TEXT = [
  '# The Quillet Guide',
  'Intro words.',
  '## Setup',
  'Install it.',
  '### **Bold** `step`',
  'Run it.',
  '## Usage',
  'Use it.',
  '# Appendix',
  'Last words.',
].join('\n\n');

const trailAt = (words: string): string[] => headingTrail(TEXT, articleBlocks(TEXT), TEXT.indexOf(words));

describe('headingTrail', () => {
  it('is empty before any heading', () => {
    expect(headingTrail(TEXT, articleBlocks(TEXT), 0)).toEqual([]);
  });

  it('lists the headings above an offset, outermost first', () => {
    expect(trailAt('Intro words.')).toEqual(['The Quillet Guide']);
    expect(trailAt('Install it.')).toEqual(['The Quillet Guide', 'Setup']);
  });

  it('reads a heading as the reader draws it, without inline Markdown', () => {
    expect(trailAt('Run it.')).toEqual(['The Quillet Guide', 'Setup', 'Bold step']);
  });

  it('drops a sibling heading and what was nested under it', () => {
    expect(trailAt('Use it.')).toEqual(['The Quillet Guide', 'Usage']);
  });

  it('starts over at a new top-level heading', () => {
    expect(trailAt('Last words.')).toEqual(['Appendix']);
  });
});

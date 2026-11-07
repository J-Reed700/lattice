import { describe, expect, it } from 'vitest';

import { checkWords, commonParts, passageKey, passageNumbersFor } from '@/features/learning/model/evidencePresentation';
import type { LearningClaimEvidenceDto, LearningEvidencePassageDto } from '@/lib/bindings';

const passage = (version: string, startByte = 0): LearningEvidencePassageDto => ({
  sourceVersionId: version, startByte, endByte: startByte + 5,
  title: 'Source', text: 'Quote', url: null, retrievalKind: 'lexical', trust: null,
});
const claim = (sectionIndex: number, passages: LearningEvidencePassageDto[]): LearningClaimEvidenceDto => ({
  sectionIndex, passages, claim: 'Claim', contentQuote: 'Lesson text', verdict: 'supported', reason: 'Reason', supportingQuote: null,
  resolution: 'verified', note: null, sufficiency: null,
});

describe('lesson passage numbering', () => {
  it('deduplicates the same saved passage across sections without collapsing different versions or spans', () => {
    const first = passage('v1');
    const second = passage('v1', 10);
    const third = passage('v2');
    const numbers = passageNumbersFor({ teachingClaims: [claim(0, [first, second]), claim(1, [first, third])] });
    expect([...numbers.entries()]).toEqual([[passageKey(first), 1], [passageKey(second), 2], [passageKey(third), 3]]);
  });

  it('has no citation numbers before a report is available', () => {
    expect(passageNumbersFor(null).size).toBe(0);
    expect(passageNumbersFor(undefined).size).toBe(0);
  });
});

describe('how a statement was checked, in words', () => {
  const titled = (version: string, title: string, retrievalKind = 'lexical', text = 'Quote'): LearningEvidencePassageDto => ({ ...passage(version), title, retrievalKind, text });
  const classed = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({
    ...claim(0, [titled('v1', 'Tesseract documentation')]), class: 'sourced', ...overrides,
  });

  it.each([
    ['attributed', 'According to Tesseract documentation'],
    ['practice', 'A practice, per Tesseract documentation'],
    ['community_consensus', 'Community consensus, per Tesseract documentation'],
    ['reported_by', 'Reported by whoever measured it, per Tesseract documentation'],
    ['secondary', 'Per Tesseract documentation, not the owner’s documentation'],
    ['mirror', 'Per Tesseract documentation, a verified copy of the official text'],
    ['best_available', 'Per Tesseract documentation, the strongest source the course has on this'],
    [null, 'Stated by Tesseract documentation'],
  ])('names the source of a Sourced statement the gate labelled %s', (label, expected) => {
    expect(checkWords(classed({ label }))).toBe(expected);
  });

  it('names the passage that states the statement, not just the first one it was compared with', () => {
    const passages = [titled('v1', 'Release notes'), titled('v2', 'API reference')];
    const parts = [{ part: 'The API returns pixels', kind: 'stated' as const, sourceVersionId: 'v2' }];
    expect(checkWords(classed({ passages, parts, label: 'attributed' }))).toBe('According to API reference');
  });

  it('names the passage holding the supporting quote, and never an executed example', () => {
    const passages = [titled('run', 'Executed example', 'execution'), titled('v1', 'Release notes', 'lexical', 'Other words'), titled('v2', 'API reference', 'lexical', 'The Quote is here')];
    expect(checkWords(classed({ passages, supportingQuote: 'Quote' }))).toBe('Stated by API reference');
    expect(checkWords(classed({ passages, supportingQuote: null }))).toBe('Stated by Release notes');
    expect(checkWords(classed({ passages: [passages[0]], supportingQuote: 'Quote' }))).toBeNull();
  });

  it('says a Reasoning statement is consistent with the sources it was read against, each once', () => {
    const titles = (...names: string[]) => names.map((name, index) => titled(`v${index}`, name));
    expect(checkWords(classed({ class: 'reasoning', passages: titles('Rust book') }))).toBe('Consistent with Rust book');
    expect(checkWords(classed({ class: 'reasoning', passages: titles('Rust book', 'Cargo book', 'Rust book') }))).toBe('Consistent with Rust book and Cargo book');
    expect(checkWords(classed({ class: 'reasoning', passages: titles('A', 'B', 'C') }))).toBe('Consistent with A, B and C');
    expect(checkWords(classed({ class: 'reasoning', passages: titles('A', 'B', 'C', 'D') }))).toBe('Consistent with A, B and 2 more');
    expect(checkWords(classed({ class: 'reasoning', passages: [] }))).toBeNull();
  });

  it('names no source for a Common statement, which has none, or for one that was never sorted', () => {
    expect(checkWords(classed({ class: 'common' }))).toBeNull();
    expect(checkWords(classed({ class: null }))).toBeNull();
    expect(checkWords(claim(0, [titled('v1', 'Tesseract documentation')]))).toBeNull();
  });

  it('lists the parts of a Sourced statement that are common knowledge, not the ones a passage states', () => {
    const parts = [
      { part: 'Quillet is a lexer', kind: 'stated' as const, sourceVersionId: 'v1' },
      { part: 'a lexer turns characters into tokens', kind: 'common' as const, sourceVersionId: null },
    ];
    expect(commonParts(classed({ parts }))).toEqual(['a lexer turns characters into tokens']);
    expect(commonParts(classed({ parts: [parts[0]] }))).toEqual([]);
    expect(commonParts(classed({}))).toEqual([]);
  });
});

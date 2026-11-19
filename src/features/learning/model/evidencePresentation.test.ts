import { describe, expect, it } from 'vitest';

import { checkWords, citationNumbers, citationsByNumber, claimMarkVerdict, partsOfKind, passageKey, resolutionText, UNSTATED_PARTS } from '@/features/learning/model/evidencePresentation';
import type { LearningCitationDto, LearningClaimEvidenceDto, LearningEvidencePassageDto } from '@/lib/bindings';

const passage = (version: string, startByte = 0): LearningEvidencePassageDto => ({
  sourceVersionId: version, startByte, endByte: startByte + 5,
  title: 'Source', text: 'Quote', url: null, retrievalKind: 'lexical', trust: null,
});
const claim = (sectionIndex: number, passages: LearningEvidencePassageDto[]): LearningClaimEvidenceDto => ({
  sectionIndex, passages, claim: 'Claim', contentQuote: 'Lesson text', verdict: 'supported', reason: 'Reason', supportingQuote: null,
  resolution: 'verified', note: null, sufficiency: null, cites: [],
});
const citation = (number: number, version: string, title = 'Source', startByte = 0): LearningCitationDto => ({
  number, sourceId: `source-${version}`, passage: { ...passage(version, startByte), title },
});

describe('lesson passage numbering', () => {
  it('takes the numbers the report gives its passages, not its own count', () => {
    const first = citation(1, 'v1');
    const second = citation(2, 'v1', 'Source', 10);
    const third = citation(3, 'v2');
    const numbers = citationNumbers({ citations: [first, second, third] });
    expect([...numbers.entries()]).toEqual([[passageKey(first.passage), 1], [passageKey(second.passage), 2], [passageKey(third.passage), 3]]);
  });

  it('has no citation numbers before a report is available', () => {
    expect(citationNumbers(null).size).toBe(0);
    expect(citationNumbers(undefined).size).toBe(0);
    expect(citationsByNumber(null).size).toBe(0);
    expect(citationsByNumber(undefined).size).toBe(0);
  });

  it('finds each passage by its number', () => {
    const byNumber = citationsByNumber({ citations: [citation(1, 'v1', 'Rust book'), citation(2, 'v2', 'Cargo book')] });
    expect(byNumber.get(2)?.passage.title).toBe('Cargo book');
    expect(byNumber.get(3)).toBeUndefined();
  });
});

describe('how a statement was checked, in words', () => {
  const cited = (...titles: string[]) => citationsByNumber({ citations: titles.map((title, index) => citation(index + 1, `v${index + 1}`, title)) });
  const classed = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({
    ...claim(0, [passage('v1')]), class: 'sourced', cites: [1], ...overrides,
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
    expect(checkWords(classed({ label }), cited('Tesseract documentation'))).toBe(expected);
  });

  it('names the passage behind the first number the statement cites, with a label and without', () => {
    const citations = cited('Release notes', 'API reference');
    expect(checkWords(classed({ cites: [2, 1], label: 'attributed' }), citations)).toBe('According to API reference');
    expect(checkWords(classed({ cites: [2, 1], label: null }), citations)).toBe('Stated by API reference');
    expect(checkWords(classed({ cites: [1, 2], label: null }), citations)).toBe('Stated by Release notes');
  });

  it('names no source for a Sourced statement that cites none, or one the report does not hold', () => {
    const citations = cited('Release notes');
    expect(checkWords(classed({ cites: [] }), citations)).toBeNull();
    expect(checkWords(classed({ cites: undefined }), citations)).toBeNull();
    expect(checkWords(classed({ cites: [9] }), citations)).toBeNull();
  });

  it('says a Reasoning statement is consistent with the sources it cites, each once, in order', () => {
    const named = (...numbers: number[]) => classed({ class: 'reasoning', cites: numbers });
    expect(checkWords(named(1), cited('Rust book'))).toBe('Consistent with Rust book');
    expect(checkWords(named(1, 2, 3), cited('Rust book', 'Cargo book', 'Rust book'))).toBe('Consistent with Rust book and Cargo book');
    expect(checkWords(named(1, 2, 3), cited('A', 'B', 'C'))).toBe('Consistent with A, B and C');
    expect(checkWords(named(1, 2, 3, 4), cited('A', 'B', 'C', 'D'))).toBe('Consistent with A, B and 2 more');
    expect(checkWords(named(), cited('A'))).toBeNull();
  });

  it('names no source for a Common statement, which has none, or for one that was never sorted', () => {
    const citations = cited('Tesseract documentation');
    expect(checkWords(classed({ class: 'common' }), citations)).toBeNull();
    expect(checkWords(classed({ class: null }), citations)).toBeNull();
    expect(checkWords(claim(0, [passage('v1')]), citations)).toBeNull();
  });

  it('lists the parts of a Sourced statement that are common knowledge, not the ones a passage states', () => {
    const parts = [
      { part: 'Quillet is a lexer', kind: 'stated' as const, sourceVersionId: 'v1' },
      { part: 'a lexer turns characters into tokens', kind: 'common' as const, sourceVersionId: null },
    ];
    expect(partsOfKind(classed({ parts }), 'common')).toEqual(['a lexer turns characters into tokens']);
    expect(partsOfKind(classed({ parts: [parts[0]] }), 'common')).toEqual([]);
    expect(partsOfKind(classed({}), 'common')).toEqual([]);
  });

  it('labels the two kinds of unstated part', () => {
    expect(UNSTATED_PARTS.map(({ kind }) => kind)).toEqual(['common', 'own']);
  });
});

describe('how a statement is marked in the lesson', () => {
  const settled = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({ ...claim(0, []), class: 'sourced', ...overrides });

  it.each([
    ['a verified Sourced statement', { resolution: 'verified', class: 'sourced' }, 'supported'],
    ['a verified Reasoning statement', { resolution: 'verified', class: 'reasoning' }, 'supported'],
    ['a verified statement that was never sorted', { resolution: 'verified', class: null }, 'supported'],
    ['a verified Common statement, which nothing checked against a source', { resolution: 'verified', class: 'common' }, 'unverified'],
    ['a statement the learner confirmed', { resolution: 'confirmed_by_you', class: 'sourced' }, 'unverified'],
    ['a statement whose source was removed', { resolution: 'source_removed', class: 'sourced' }, 'unverified'],
    ['a statement a source contradicts', { resolution: 'not_verified', unverifiedReason: 'contradicted' }, 'contradicted'],
    ['a statement no source states', { resolution: 'not_verified', unverifiedReason: 'unsupported' }, 'unsupported'],
    ['a statement whose check did not finish', { resolution: 'not_verified', unverifiedReason: 'unchecked' }, 'unverified'],
    ['an unverified statement with no reason on record', { resolution: 'not_verified', unverifiedReason: null }, 'unverified'],
  ] as const)('marks %s', (_name, overrides, expected) => {
    expect(claimMarkVerdict(settled(overrides))).toBe(expected);
  });
});

describe('how a statement stands, in words', () => {
  const settled = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({ ...claim(0, []), class: 'sourced', ...overrides });

  it.each([
    [{ resolution: 'not_verified' }, 'Not verified'],
    [{ resolution: 'confirmed_by_you' }, 'Confirmed by you'],
    [{ resolution: 'source_removed' }, 'Source removed · review'],
    [{ resolution: 'verified', class: 'common' }, 'Common knowledge · not checked against a source'],
    [{ resolution: 'verified', class: 'reasoning' }, 'Consistent'],
    [{ resolution: 'verified', class: 'sourced', verdict: 'supported' }, 'supported'],
    [{ resolution: 'verified', class: 'sourced', verdict: '' }, 'checked'],
  ] as const)('says %j is %s', (overrides, expected) => {
    expect(resolutionText(settled(overrides))).toBe(expected);
  });

  it('never reads a confirmation as verified', () => {
    expect(resolutionText(settled({ resolution: 'confirmed_by_you', verdict: 'supported' }))).not.toMatch(/verified|supported/i);
  });
});

import { describe, expect, it } from 'vitest';

import {
  badgeText,
  barLine,
  impactLine,
  insufficiencyExplanation,
  overrideLines,
  sufficiencyHeadline,
  sufficiencyNote,
  TRUST_LEVELS,
  verifiedByLevel,
} from '@/features/learning/sources/trust/trustPresentation';
import { blogOnlyClaim, entryOf, SPECIFICATION_BAR, trustOf } from '@/features/learning/sources/trust/trustTestData';
import type { LearningClaimEvidenceDto } from '@/lib/bindings';

describe('trust presentation', () => {
  it('names every level in words', () => {
    expect(TRUST_LEVELS.map((level) => badgeText(trustOf(level)))).toEqual([
      'Primary', 'Authoritative', 'Established', 'Community', 'Unknown standing', 'Distrusted',
    ]);
  });

  it('says what a primary source is primary for, and marks the user’s own overrides', () => {
    expect(badgeText(trustOf('primary', { subject: 'Python' }))).toBe('Primary for Python');
    expect(badgeText(trustOf('established', { subject: 'Python' }))).toBe('Established');
    expect(badgeText(trustOf('distrusted', { reasons: [{ code: 'distrusted_by_you', text: 'You distrusted example.org' }] }))).toBe('Distrusted (you)');
    expect(badgeText(trustOf('primary', { overrideScope: { scope: 'source', programId: 'p', sourceId: 's' } }))).toBe('Primary (you)');
    expect(badgeText(trustOf('unknown', { reasons: [{ code: 'added_by_you', text: 'Added by you' }] }))).toBe('Added by you · origin not stated');
  });

  it('groups site-for-subject overrides into one line per site and level', () => {
    const at = { note: null, setAt: 1 };
    expect(overrideLines([
      { scope: 'site_subject', registrableDomain: 'learn.microsoft.com', class: { label: '.NET', key: 'net' }, level: 'primary', ...at },
      { scope: 'site_subject', registrableDomain: 'learn.microsoft.com', class: { label: 'C#', key: 'c#' }, level: 'primary', ...at },
      { scope: 'site', registrableDomain: 'example.org', standing: 'institutional', ...at },
    ])).toEqual([
      'example.org treated as institutional everywhere',
      'Trust learn.microsoft.com as primary for: .NET, C#',
    ]);
  });

  it('says how many waiting claims an override re-decides', () => {
    expect(impactLine({ versions: 1, claims: 3, lessons: 2 }, 'source')).toBe('3 claims waiting for review in 2 lessons cite this source; they will be re-decided.');
    expect(impactLine({ versions: 4, claims: 1, lessons: 1 }, 'site')).toBe('1 claim waiting for review in 1 lesson cites this site; it will be re-decided.');
    expect(impactLine({ versions: 4, claims: 0, lessons: 0 }, 'site')).toBe('No claim waiting for review cites this site; later checks use the new level.');
  });

  it('states the bar in force, and says when only corroboration meets it', () => {
    expect(barLine(SPECIFICATION_BAR)).toBe('Accepted alone: a primary or authoritative source');
    expect(barLine({ ...SPECIFICATION_BAR, lowestAccepted: null })).toBe('No single source is enough for how an API, standard or rule is defined now; it needs independent sources that agree');
  });

  it('explains in plain words why a blog is not enough for an API specification', () => {
    const claim = blogOnlyClaim();
    expect(sufficiencyHeadline(claim)).toBe('Specification claim about OpenAI API · supported only by an established source · accepted alone: a primary or authoritative source');
    expect(insufficiencyExplanation(claim, 'machine')).toBe('An established source (an identified author, such as a blog) isn’t enough for how an API, standard or rule is defined; looking for the owner’s documentation.');
    expect(insufficiencyExplanation(claim, 'you')).toMatch(/waiting for you: accept this source for this claim/);
    const primary = { ...claim, supports: [entryOf(trustOf('primary', { subject: 'OpenAI API' }))] };
    expect(insufficiencyExplanation(primary, 'machine')).toBeNull();
  });

  it('shows a published claim’s kind, subject and label', () => {
    expect(sufficiencyNote({ claimKind: 'specification', subject: 'C#', sufficient: true, bar: null, label: 'secondary', reasons: [] }))
      .toBe('Specification claim about C# · per a secondary source, not the owner’s documentation');
    expect(sufficiencyNote({ claimKind: 'practice', subject: null, sufficient: true, bar: null, label: null, reasons: [] })).toBe('Practice claim');
  });

  it('counts verified claims by the best level of their support', () => {
    const passage = (level: 'primary' | 'established' | null) => ({
      sourceVersionId: 'v', title: 't', url: null, text: 'x', startByte: 0, endByte: 1, retrievalKind: 'hybrid',
      trust: level ? trustOf(level) : null,
    });
    const claim = (levels: Array<'primary' | 'established' | null>): LearningClaimEvidenceDto => ({
      sectionIndex: 0, contentQuote: 'q', claim: 'c', verdict: 'supported', reason: 'r', supportingQuote: null,
      passages: levels.map(passage), resolution: 'verified', note: null, sufficiency: null,
    });
    expect(verifiedByLevel([claim(['primary', 'established']), claim(['primary']), claim(['established']), claim([null])])).toEqual([
      '2 claims verified against primary sources', '1 against established sources', '1 against sources not weighed for standing',
    ]);
    expect(verifiedByLevel([claim([null])])).toEqual([]);
  });
});

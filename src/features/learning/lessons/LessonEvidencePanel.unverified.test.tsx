import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { evidenceSummary, LessonEvidencePanel } from '@/features/learning/lessons/LessonEvidencePanel';
import type { LearningClaimEvidenceDto, LearningLessonEvidenceDto } from '@/lib/bindings';

// A lesson publishes statements verification could not settle, marked "Not verified"
// (design-publish-unverified.md, "Wording"). The learner never reads one as verified.
const mocks = vi.hoisted(() => ({ evidence: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningLessonEvidence: mocks.evidence } }));

const claim = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({
  sectionIndex: 0, contentQuote: 'The sentence.', claim: 'A statement.', verdict: 'supported', reason: 'Checked.', supportingQuote: null,
  passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: null, ...overrides,
});

const report = (teachingClaims: LearningClaimEvidenceDto[], overrides: Partial<LearningLessonEvidenceDto> = {}): LearningLessonEvidenceDto => ({
  policy: 'lesson-evidence-v21', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash',
  claimCount: teachingClaims.length, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true, teachingClaims, ...overrides,
});

const unverified = (text: string, reason: 'unchecked' | 'unsupported' | 'contradicted') =>
  claim({ claim: text, resolution: 'not_verified', unverifiedReason: reason, verdict: 'unsupported', reason: 'No passage states it.' });

function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><LessonEvidencePanel programId="course" lessonId="lesson" /></QueryClientProvider>);
}

describe('lesson evidence with statements that are not verified', () => {
  beforeEach(() => vi.clearAllMocks());

  it('labels such a statement "Not verified", never with a verdict that reads as a pass', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: report([
      claim({ claim: 'Ice floats on water.' }),
      unverified('Water boils at 100 °C on Everest.', 'unsupported'),
    ]) });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    await screen.findByText('Water boils at 100 °C on Everest.');
    const chip = screen.getByText('Not verified');
    expect(chip).toBeVisible();
    expect(chip.className).not.toMatch(/emerald/);
    expect(screen.queryByText('unsupported')).not.toBeInTheDocument();
  });

  it('adds "2 not verified" to the summary, and the verified count leaves them out', async () => {
    const data = report([
      claim({ claim: 'Ice floats on water.' }),
      claim({ claim: 'Water expands as it freezes.' }),
      unverified('Water boils at 100 °C on Everest.', 'unsupported'),
      unverified('Steam is hotter than boiling water.', 'contradicted'),
    ]);
    expect(evidenceSummary(data)).toBe('2 statements verified against sources · 2 not verified');
    mocks.evidence.mockResolvedValue({ ok: true, data });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/2 statements verified against sources · 2 not verified/)).toBeVisible();
  });

  it('counts a lone unverified statement as "1 not verified" and never as verified', () => {
    expect(evidenceSummary(report([unverified('Water boils at 100 °C on Everest.', 'unchecked')]))).toBe('0 statements verified against sources · 1 not verified');
  });

  it('reads exactly as today for a lesson with nothing unverified', async () => {
    const data = report([claim({ claim: 'Ice floats on water.' }), claim({ claim: 'Water expands as it freezes.' })]);
    expect(evidenceSummary(data)).toBe('2 statements verified against sources');
    mocks.evidence.mockResolvedValue({ ok: true, data });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/2 statements verified against sources/)).toBeVisible();
    expect(screen.queryByText(/not verified/i)).not.toBeInTheDocument();
  });
});

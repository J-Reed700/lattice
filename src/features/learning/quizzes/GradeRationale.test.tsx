import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { GradeRationale } from '@/features/learning/quizzes/GradeRationale';
import { criterion, rationale } from '@/features/learning/quizzes/quizTestData';
import { listNumbers, numberQuotes } from '@/features/learning/quizzes/rationaleHighlights';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningSourceDto } from '@/lib/bindings';

const ANSWER = 'The inhibitor binds the ES complex, so both Vmax and Km fall. It cannot be outcompeted.';

describe('numbering the learner’s own words', () => {
  it('numbers quotes in reading order and ties each number to its criteria', () => {
    const criteria = [
      { yourWords: ['It cannot be outcompeted'] },
      { yourWords: ['binds the ES complex', 'It cannot be outcompeted'] },
      { yourWords: [] },
    ];
    const { segments, byCriterion, loose } = numberQuotes(ANSWER, criteria);
    expect(segments.filter((segment) => segment.quote).map((segment) => [segment.quote, segment.text])).toEqual([
      [1, 'binds the ES complex'],
      [2, 'It cannot be outcompeted'],
    ]);
    // A quote two criteria share keeps one number; an omission rests on none.
    expect(byCriterion).toEqual([[2], [1, 2], []]);
    expect(loose).toEqual([]);
    expect(segments.map((segment) => segment.text).join('')).toBe(ANSWER);
  });

  it('matches across line breaks the way the grader checks, and lists quotes it cannot place', () => {
    const { segments, loose } = numberQuotes('binds the\nES complex', [{ yourWords: ['binds the ES complex', 'never said this'] }]);
    expect(segments).toEqual([{ text: 'binds the\nES complex', quote: 1 }]);
    expect(loose).toEqual([{ quote: 2, text: 'never said this' }]);
  });

  it('lists numbers in words', () => {
    expect(listNumbers([1])).toBe('1');
    expect(listNumbers([1, 3])).toBe('1 and 3');
    expect(listNumbers([1, 2, 4])).toBe('1, 2 and 4');
  });
});

describe('Why this lost points', () => {
  const lost = rationale({
    summary: 'You explained how the inhibitor binds, but gave no concrete example, which the question asked for.',
    criteria: [
      criterion({ criterionId: 'accuracy', title: 'Accuracy', verdict: 'met', points: 2, maxPoints: 2, yourWords: ['inhibitor binds the ES complex'] }),
      criterion({ criterionId: 'example', title: 'Example', verdict: 'missed', points: 0, maxPoints: 2, whatWasMissing: 'No example given.', fullCreditIncludes: 'Names a real inhibitor and its effect.' }),
    ],
  });

  it('opens by default when points were lost, with the summary, numbered highlights, criteria and grader', () => {
    render(<GradeRationale rationale={lost} answer={ANSWER} lostPoints statusWord="provisional" sources={[]} />);
    const panel = screen.getByText('Why this lost points').closest('details')!;
    expect(panel).toHaveAttribute('open');
    expect(screen.getByText(/gave no concrete example/)).toBeVisible();
    const mark = panel.querySelector('mark[data-quote="1"]')!;
    expect(mark).toHaveTextContent('inhibitor binds the ES complex1 (quote 1)');
    const accuracy = within(screen.getByText('Accuracy').closest('li')!);
    expect(accuracy.getByText('Met')).toBeVisible();
    expect(accuracy.getByText('Rests on quote 1, in your words.')).toBeVisible();
    const example = within(screen.getByText('Example').closest('li')!);
    expect(example.getByText('Missed')).toBeVisible();
    expect(example.getByText('No example given.')).toBeVisible();
    expect(example.getByText('Names a real inhibitor and its effect.')).toBeVisible();
    expect(screen.getByText(/^Graded by Qwen3\.5-9B · provisional · /)).toBeVisible();
  });

  it('starts collapsed for a full-credit grade, behind a native disclosure', () => {
    render(<GradeRationale rationale={rationale()} answer={ANSWER} lostPoints={false} statusWord="provisional" sources={[]} />);
    const summary = screen.getByText('How this was graded').closest('summary')!;
    expect(summary.closest('details')).not.toHaveAttribute('open');
    expect(screen.queryByText('Why this lost points')).not.toBeInTheDocument();
  });
});

describe('a criterion’s source quote', () => {
  const sources: LearningSourceDto[] = [{ id: 'version-1', title: 'Field notes', url: null, excerpt: '', acquiredAt: 0 }];
  const versionSummary = {
    id: 'version-1', versionNumber: 1, title: 'Field notes', publisher: null, resolvedUrl: null, excerpt: '', contentSha256: 'sha-1',
    wordCount: 5, truncated: false, extractionVersion: 'reader-1', acquiredAt: 0, snapshotHash: null,
  };
  const workspace = {
    programId: 'program-1', recheck: null,
    sources: [{
      id: 'source-1', kind: 'web', addedBy: 'user', origin: 'web', requestedUrl: null, freshnessPolicy: 'manual', activeVersionId: 'version-1',
      pendingVersionId: null, revision: 1, deletedAt: null, deletionReason: null, activeVersion: versionSummary, pendingVersion: null,
      versions: [versionSummary], latestCheck: null, checks: [], createdAt: 0, updatedAt: 0,
    }],
  };
  const ok = <T,>(data: T) => ({ ok: true as const, data });
  const getWorkspace = vi.fn();
  const getVersion = vi.fn();
  const cited = rationale({
    criteria: [criterion({
      criterionId: 'example', title: 'Example', verdict: 'missed', points: 0, maxPoints: 2,
      whatWasMissing: 'No example given.', fullCreditIncludes: 'Names an inhibitor.',
      sourceQuote: 'inhibitors bind the enzyme', sourceVersionId: 'version-1',
    })],
  });

  function wrapper({ children }: { children: ReactNode }) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
    const api = { getLearningSourceWorkspace: getWorkspace, getLearningSourceVersion: getVersion } as unknown as ApiClient;
    return (
      <ApiProvider client={api}>
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      </ApiProvider>
    );
  }

  const renderRationale = (extra: { programId?: string; ownerKey?: string } = {}) => render(
    <GradeRationale rationale={cited} answer={ANSWER} lostPoints statusWord="provisional" sources={sources} {...extra} />,
    { wrapper },
  );

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      unobserve() {}
      disconnect() {}
    });
    getWorkspace.mockReset().mockResolvedValue(ok(workspace));
    getVersion.mockReset().mockResolvedValue(ok({ sourceId: 'source-1', version: versionSummary, fullText: 'Inhibitors bind the enzyme.', usage: [] }));
    useCitationDisplayStore.setState({ visible: true });
    useReaderStore.setState({ session: null });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    useCitationDisplayStore.setState({ visible: true });
    useReaderStore.setState({ session: null });
  });

  it('is an openable citation with the chip, quote and source title when the course and owner are known', async () => {
    renderRationale({ programId: 'program-1', ownerKey: 'answer-1' });

    const list = screen.getByRole('list', { name: 'Source' });
    const item = within(list).getByRole('listitem');
    expect(within(item).getByRole('button', { name: /Citation 1/ })).toBeInTheDocument();
    expect(within(item).getByText('inhibitors bind the enzyme')).toBeInTheDocument();
    expect(within(item).getByText(/Field notes/)).toBeInTheDocument();
    expect(document.querySelector('blockquote')).toBeNull();

    fireEvent.click(within(item).getByRole('button', { name: /Citation 1/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());
    expect(useReaderStore.getState().session!.ownerKey).toBe('answer-1:criterion:example');
  });

  it('stays the original blockquote without a course and owner', () => {
    renderRationale();

    const quote = document.querySelector('blockquote')!;
    expect(quote).toHaveTextContent('“inhibitors bind the enzyme” — Field notes');
    expect(screen.queryByRole('button', { name: /Citation 1/ })).not.toBeInTheDocument();
  });

  it('stays the blockquote when only the course is known', () => {
    renderRationale({ programId: 'program-1' });

    expect(document.querySelector('blockquote')).not.toBeNull();
    expect(screen.queryByRole('button', { name: /Citation 1/ })).not.toBeInTheDocument();
  });

  it('shows neither the citation nor the blockquote in reading mode', () => {
    useCitationDisplayStore.setState({ visible: false });
    renderRationale({ programId: 'program-1', ownerKey: 'answer-1' });

    expect(screen.queryByRole('list', { name: 'Source' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Citation 1/ })).not.toBeInTheDocument();
    expect(document.querySelector('blockquote')).toBeNull();
  });

  it('shows no blockquote in reading mode without a course either', () => {
    useCitationDisplayStore.setState({ visible: false });
    renderRationale();

    expect(document.querySelector('blockquote')).toBeNull();
  });
});

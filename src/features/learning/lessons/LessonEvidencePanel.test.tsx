import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonEvidencePanel } from '@/features/learning/lessons/LessonEvidencePanel';
import { useReaderStore } from '@/features/reading/stores/readerStore';

const mocks = vi.hoisted(() => ({ evidence: vi.fn(), version: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningLessonEvidence: mocks.evidence, getLearningSourceVersion: mocks.version } }));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: ({ isOpen }: { isOpen: boolean }) => (isOpen ? <div data-testid="preview-modal" /> : null),
}));
function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><LessonEvidencePanel programId="course" lessonId="lesson" /></QueryClientProvider>);
}
describe('lesson evidence', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useReaderStore.setState({ session: null, resolvedLocations: new Map() });
    mocks.version.mockResolvedValue({ ok: false, error: 'The saved text is unavailable.' });
  });
  it('loads on demand and makes limitations and source versions visible', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: {
      policy: 'lesson-evidence-v2', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: 'embedding', contentSha256: 'hash', claimCount: 9, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: ['rust'], sourcesCurrent: false,
      citations: [], sections: [],
      teachingClaims: [{ sectionIndex: 0, contentQuote: 'Borrowing leaves ownership with the owner.', claim: 'Borrowing does not transfer ownership.', verdict: 'supported', reason: 'The source supports this.', supportingQuote: 'Borrowing gives access.', passages: [{ sourceVersionId: 'version-1', title: 'Saved Rust reference', url: 'javascript:alert(1)', text: 'Borrowing gives access.', startByte: 0, endByte: 23, retrievalKind: 'hybrid' }] }],
    } });
    show();
    expect(mocks.evidence).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/9 statements verified against sources/)).toBeVisible();
    expect(screen.getByText(/were not compiled or executed/)).toBeVisible();
    expect(screen.getByText(/collection has changed/)).toBeVisible();
    await userEvent.click(screen.getByText('Borrowing does not transfer ownership.'));
    await userEvent.click(screen.getByText('Saved Rust reference'));
    expect(screen.getByText(/Saved version version-1/)).toBeVisible();
    expect(screen.queryByRole('link', { name: 'Open original page' })).not.toBeInTheDocument();
  });
  it('does not imply older lessons passed verification', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: null });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/no saved verification report/)).toBeVisible();
  });

  it('shows invalid saved evidence as unavailable without presenting it as checked', async () => {
    mocks.evidence.mockResolvedValue({ ok: false, error: 'Saved lesson evidence is unavailable: invalid report.' });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('evidence is unavailable');
    expect(screen.queryByText(/statements verified against sources/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry evidence' })).toBeVisible();
  });

  it('labels a statement the checks found to be common knowledge neutrally and counts it apart', async () => {
    const data = {
      policy: 'lesson-evidence-v18', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 2, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      citations: [], sections: [],
      teachingClaims: [
        { sectionIndex: 0, contentQuote: 'Queries only read.', claim: 'Query handlers read state.', verdict: 'supported', reason: 'Exact passage agrees.', supportingQuote: null, passages: [{ sourceVersionId: 'version-3', title: 'CQRS notes', url: null, text: 'Queries read state.', startByte: 0, endByte: 19, retrievalKind: 'hybrid' }], resolution: 'verified', class: 'sourced' },
        { sectionIndex: 0, contentQuote: 'Handlers stay small.', claim: 'Small handlers are easier to test.', verdict: 'supported', reason: 'Standard knowledge of the field.', supportingQuote: null, passages: [], resolution: 'verified', class: 'common' },
      ],
    };
    mocks.evidence.mockResolvedValue({ ok: true, data });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/1 statement verified against sources · 1 common knowledge, not checked against a source/)).toBeVisible();
    const label = screen.getByText('Common knowledge · not checked against a source');
    expect(label).toBeVisible();
    expect(label.className).not.toMatch(/rose|red|amber|emerald/);
  });

  it('shows a confirmation as the user’s, apart from machine verification, and flags removed sources', async () => {
    const passage = { sourceVersionId: 'version-4', title: 'Steam tables', url: null, text: 'Water boils at 99.97 °C at one atmosphere.', startByte: 0, endByte: 42, retrievalKind: 'hybrid' };
    mocks.evidence.mockResolvedValue({ ok: true, data: {
      policy: 'lesson-evidence-v19', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 15, confirmedCount: 3, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      citations: [], sections: [],
      teachingClaims: [
        { sectionIndex: 0, contentQuote: 'Water boils at 100 °C.', claim: 'Water boils at 100 °C at sea level.', verdict: 'contradicted', reason: 'The passage gives 99.97 °C.', supportingQuote: null, passages: [passage], resolution: 'confirmed_by_you', note: 'Rounded for beginners.' },
        { sectionIndex: 0, contentQuote: 'Ice floats.', claim: 'Ice floats on water.', verdict: 'supported', reason: 'Supported.', supportingQuote: null, passages: [passage], resolution: 'source_removed', note: null },
      ],
    } });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/11 statements verified against sources · 3 confirmed by you/)).toBeVisible();
    const confirmed = screen.getByText('Confirmed by you');
    expect(confirmed.className).not.toMatch(/emerald/);
    expect(screen.getByText(/Rounded for beginners/)).toBeVisible();
    expect(screen.getByText('Source removed · review')).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('1 statement lost its source — use Review to look at it.');
    expect(screen.getByText('The source that supported this statement was deleted. The lesson stays readable; use Review to attach another source or confirm it.')).toBeInTheDocument();
    expect(screen.getByText('2 teaching statements')).toBeInTheDocument();
  });

  describe('opening a passage', () => {
    const passage = (version: string, title: string, url: string | null) => ({ sourceVersionId: version, title, url, text: `Text of ${title}.`, startByte: 0, endByte: 20, retrievalKind: 'hybrid' });
    const guide = passage('version-7', 'Field guide', 'https://example.org/guide');
    const notes = passage('version-8', 'Lecture notes', 'https://example.org/notes');
    const data = {
      policy: 'lesson-evidence-v22', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 1, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      citations: [{ number: 1, sourceId: 'guide', passage: guide }, { number: 2, sourceId: 'notes', passage: notes }], sections: [],
      teachingClaims: [{
        sectionIndex: 0, contentQuote: 'The sentence.', claim: 'A statement.', verdict: 'supported', reason: 'Checked.', supportingQuote: null,
        passages: [notes, guide], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], cites: [2],
      }],
    };

    it('shows each passage under the number the report gave it, and names the source behind the number the statement cites', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
      expect(await screen.findByText('Stated by Lecture notes')).toBeVisible();
      expect(screen.getByText('Lecture notes').closest('summary')).toHaveTextContent(/^2\s*Lecture notes/);
      expect(screen.getByText('Field guide').closest('summary')).toHaveTextContent(/^1\s*Field guide/);
    });

    it('opens the docked reader on the lesson’s sources, at the passage’s number', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
      await userEvent.click(await screen.findByText('Lecture notes'));
      await userEvent.click(screen.getAllByRole('button', { name: /Open original page/ })[0]!);
      expect(screen.queryByTestId('preview-modal')).not.toBeInTheDocument();
      const session = useReaderStore.getState().session;
      expect(session?.ownerKey).toBe('lesson:lesson:evidence');
      expect(session?.citations.map((source) => source.citationId)).toEqual([1, 2]);
      expect(session?.index).toBe(1);
      expect(session?.citations[1]?.fileName).toBe('Lecture notes');
    });

    it('closes the reader it opened when the panel leaves the screen', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data });
      const view = show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
      await userEvent.click(await screen.findByText('Lecture notes'));
      await userEvent.click(screen.getAllByRole('button', { name: /Open original page/ })[0]!);
      expect(useReaderStore.getState().session?.ownerKey).toBe('lesson:lesson:evidence');
      view.unmount();
      expect(useReaderStore.getState().session).toBeNull();
    });
  });

  describe('statements sorted into classes', () => {
    const passage = (version: string, title: string, text: string) => ({ sourceVersionId: version, title, url: null, text, startByte: 0, endByte: text.length, retrievalKind: 'hybrid' });
    const manual = passage('version-5', 'Quillet manual', 'Quillet is a lexer written in Rust.');
    const notes = passage('version-6', 'Compiler notes', 'A parser consumes the tokens a lexer makes.');
    const claim = (overrides: Record<string, unknown>) => ({
      sectionIndex: 0, contentQuote: 'The sentence.', claim: 'A statement.', verdict: 'supported', reason: 'Checked.', supportingQuote: null,
      passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], ...overrides,
    });
    const data = {
      policy: 'lesson-evidence-v20', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 3, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      citations: [{ number: 1, sourceId: 'quillet', passage: manual }, { number: 2, sourceId: 'compilers', passage: notes }], sections: [],
      teachingClaims: [
        claim({
          claim: 'Quillet is a lexer, and a lexer turns characters into tokens.', reason: 'The manual says Quillet is a lexer; the rest is standard.',
          passages: [manual], label: 'attributed', cites: [1],
          parts: [{ part: 'Quillet is a lexer', kind: 'stated', sourceVersionId: 'version-5' }, { part: 'a lexer turns characters into tokens', kind: 'common', sourceVersionId: null }],
        }),
        claim({ claim: 'Quillet suits the front end of a parser.', class: 'reasoning', passages: [manual, notes], reason: 'Consistent with both.', cites: [1, 2] }),
        claim({ claim: 'Tokens are small pieces of text.', class: 'common', reason: 'Standard knowledge of the field.' }),
      ],
    };

    it('tells the learner how each statement was checked, naming its source, and what in a sourced one is common knowledge', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findByText(/1 statement verified against sources · 1 consistent with their sources · 1 common knowledge, not checked against a source/)).toBeVisible();
      expect(screen.getByText('According to Quillet manual')).toBeVisible();
      expect(screen.getByText(/Common knowledge, not in the source/)).toBeVisible();
      expect(screen.getByText('a lexer turns characters into tokens')).toBeVisible();
      expect(screen.getByText('Consistent with Quillet manual and Compiler notes')).toBeVisible();
      expect(screen.getByText('Consistent')).toBeVisible();
      expect(screen.getByText('Common knowledge · not checked against a source')).toBeVisible();
      expect(screen.getByText('Standard knowledge of the field.')).toBeVisible();
    });

    it('names the parts of a sourced statement that are the lesson’s own figures, beside the common ones', async () => {
      const stated = { part: 'Quillet is a lexer', kind: 'stated', sourceVersionId: 'version-5' };
      const own = { part: '12 of 48 is 25%', kind: 'own', sourceVersionId: null };
      const common = { part: 'a lexer turns characters into tokens', kind: 'common', sourceVersionId: null };
      mocks.evidence.mockResolvedValue({ ok: true, data: {
        ...data,
        teachingClaims: [
          claim({ claim: 'Quillet is a lexer, and 12 of 48 is 25%.', passages: [manual], parts: [stated, own] }),
          claim({ claim: 'Quillet is a lexer that turns characters into tokens, and 12 of 48 is 25%.', passages: [manual], parts: [stated, common, own] }),
        ],
      } });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findAllByText(/The lesson's own figures, not in the source/)).toHaveLength(2);
      expect(screen.getAllByText('12 of 48 is 25%')).toHaveLength(2);
      expect(screen.getAllByText(/Common knowledge, not in the source/)).toHaveLength(1);
    });

    it('says beside the verdict how a statement uses its source', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data: {
        ...data,
        teachingClaims: [
          claim({ claim: 'Quillet is a lexer.', passages: [manual], cites: [1], relation: 'restates' }),
          claim({ claim: 'Tokens are small pieces of text.', class: 'common', relation: null }),
        ],
      } });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findByText('Paraphrased from')).toBeVisible();
      expect(screen.getAllByText(/Paraphrased from|Quoted from|Summarized from|Inferred from|Supported by/)).toHaveLength(1);
    });

    it('lists a part that follows from the source beside the common and own ones', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data: {
        ...data,
        teachingClaims: [claim({
          claim: 'Quillet is a lexer, so it needs a path.', passages: [manual], cites: [1], relation: 'infers',
          parts: [{ part: 'so it needs a path', kind: 'follows', sourceVersionId: 'version-5', anchors: [] }],
        })],
      } });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findByText(/Follows from the source/)).toBeVisible();
      expect(screen.getByText('so it needs a path')).toBeVisible();
    });

    it('opens the reader on the sentences a part rests on, as that statement’s relation', async () => {
      const sentence = { exact: 'Quillet is a lexer written in Rust.', span: { start: 0, end: 35 } };
      mocks.evidence.mockResolvedValue({ ok: true, data: {
        ...data,
        teachingClaims: [claim({
          claim: 'Quillet is a lexer.', passages: [manual], cites: [1], relation: 'restates',
          parts: [{ part: 'Quillet is a lexer', kind: 'stated', sourceVersionId: null, anchors: [sentence] }],
        })],
      } });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
      await userEvent.click(await screen.findByRole('button', { name: 'Show in the source: Quillet is a lexer' }));

      const session = useReaderStore.getState().session;
      expect(session?.index).toBe(0);
      expect(session?.citations[0]?.anchoredBy).toEqual([{ occurrence: 0, relation: 'restates', anchors: [sentence] }]);
      expect(session?.citations[1]?.anchoredBy).toBeUndefined();
    });

    it('offers no way into the source for a part that names no sentence', async () => {
      mocks.evidence.mockResolvedValue({ ok: true, data: {
        ...data,
        teachingClaims: [claim({
          claim: 'Quillet is a lexer.', passages: [manual], cites: [1],
          parts: [{ part: 'Quillet is a lexer', kind: 'stated', sourceVersionId: 'version-5' }],
        })],
      } });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      await screen.findByText('Quillet is a lexer.');
      expect(screen.queryByRole('button', { name: /Show in the source/ })).not.toBeInTheDocument();
    });

    it('does not count a statement the user settled as having passed its class’s check', async () => {
      const confirmed = { ...data, claimCount: 3, confirmedCount: 1, teachingClaims: [...data.teachingClaims.slice(0, 2), claim({ claim: 'Tokens are small pieces of text.', class: 'common', resolution: 'confirmed_by_you', note: 'Fine.' })] };
      mocks.evidence.mockResolvedValue({ ok: true, data: confirmed });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findByText(/1 statement verified against sources · 1 consistent with their sources · 1 confirmed by you/)).toBeVisible();
      expect(screen.queryByText(/common knowledge, not checked against a source/)).not.toBeInTheDocument();
    });

    it('does not count a claim whose source was removed as verified', async () => {
      const removed = { ...data, claimCount: 2, confirmedCount: 0, teachingClaims: [claim({ claim: 'Kept.' }), claim({ claim: 'Lost its source.', resolution: 'source_removed' })] };
      mocks.evidence.mockResolvedValue({ ok: true, data: removed });
      show();
      await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));

      expect(await screen.findByText(/1 statement verified against sources/)).toBeVisible();
    });
  });
});

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import '@/tests/interactionDom';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CitedSection } from '@/features/learning/lessons/CitedSection';
import { fromLearningEvidencePassage } from '@/features/reading/model/sourceAdapter';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningCitationDto, LearningClaimEvidenceDto, LearningLessonEvidenceDto } from '@/lib/bindings';
import { cites, said } from '@/tests/replyFixtures';

const BOILING = 'Boiling needs a vapour pressure that meets the air pressure.';
const ICE = 'Ice floats on water because it is less dense.';
const HEAT = 'Heat moves from hot things to cold things.';

const citation = (number: number, title: string): LearningCitationDto => ({
  number, sourceId: `source-${number}`,
  passage: { sourceVersionId: `version-${number}`, title, url: null, text: `Passage of ${title}`, startByte: 0, endByte: 20, retrievalKind: 'hybrid', trust: null },
});
const citations = [citation(1, 'Steam tables'), citation(2, 'Field guide'), citation(3, 'Lecture notes')];
const sources = citations.map((entry) => fromLearningEvidencePassage(entry.passage, entry.number - 1).source);

const claim = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({
  sectionIndex: 1, contentQuote: BOILING, claim: BOILING, verdict: 'supported', reason: 'Checked.', supportingQuote: null,
  passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: null, cites: [], ...overrides,
});

const report = (overrides: Partial<LearningLessonEvidenceDto> = {}): LearningLessonEvidenceDto => ({
  policy: 'lesson-evidence-v22', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash',
  claimCount: 4, confirmedCount: 1, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
  teachingClaims: [
    claim({ sectionIndex: 0, contentQuote: 'Section zero says something else entirely.', cites: [1] }),
    claim({ contentQuote: BOILING, cites: [2] }),
    claim({ contentQuote: ICE, resolution: 'not_verified', unverifiedReason: 'contradicted', verdict: 'contradicted', cites: [3] }),
    claim({ contentQuote: HEAT, resolution: 'confirmed_by_you', note: 'Fine for beginners.', cites: [] }),
  ],
  citations,
  sections: [
    { sectionIndex: 0, pieces: [said('Section zero says something else entirely.'), cites(1)] },
    { sectionIndex: 1, pieces: [said(BOILING), cites(2), said(` ${ICE}`), cites(3), said(` ${HEAT}`)] },
  ],
  ...overrides,
});

const BODY = `${BOILING} ${ICE} ${HEAT}`;
// A mark runs from a sentence's first letter to its last, as chat draws it.
const letters = (sentence: string) => sentence.replace(/\.$/, '');

function show(options: { data?: LearningLessonEvidenceDto | null; showCitations?: boolean } = {}) {
  const data = options.data === undefined ? report() : options.data;
  return render(
    <CitedSection
      lessonId="lesson-1" sectionIndex={1} body={BODY} report={data} sources={data ? sources : []}
      showCitations={options.showCitations ?? true} wrap={(viewer) => <div data-testid="wrapped">{viewer}</div>}
    />,
  );
}

describe('a lesson section drawn like a chat answer', () => {
  beforeEach(() => useReaderStore.setState({ session: null, resolvedLocations: new Map() }));
  afterEach(() => useReaderStore.setState({ session: null, resolvedLocations: new Map() }));

  it('draws a chip with its number after each sentence that stands on a source, inside the wrapper', async () => {
    show();
    await waitFor(() => expect(document.querySelectorAll('[data-cite]')).toHaveLength(2));
    expect(Array.from(document.querySelectorAll<HTMLElement>('[data-cite]')).map((chip) => chip.dataset.cite)).toEqual(['2', '3']);
    expect(screen.getByTestId('wrapped').querySelector('[data-cite="2"]')).not.toBeNull();
  });

  it('marks each checked sentence by how it stands, and only this section’s', async () => {
    show();
    await waitFor(() => expect(document.querySelector('.claim-contradicted')).not.toBeNull());
    expect(document.querySelector('.claim-contradicted')?.textContent).toBe(letters(ICE));
    expect(document.querySelector('.claim-supported')?.textContent).toBe(letters(BOILING));
    expect(document.querySelector('.claim-unverified')?.textContent).toBe(letters(HEAT));
    expect(document.querySelectorAll('.claim')).toHaveLength(3);
  });

  it('lists exactly this section’s numbers in the Sources row', async () => {
    show();
    const row = (await screen.findByText('Sources')).parentElement!;
    expect(within(row).getAllByRole('button').map((button) => button.textContent)).toEqual(['2', '3']);
    expect(within(row).getByRole('button', { name: 'Citation 2: Field guide' })).toBeInTheDocument();
    expect(within(row).queryByRole('button', { name: /Steam tables/ })).not.toBeInTheDocument();
  });

  it('opens the docked reader on the clicked chip, among every source of the lesson in number order', async () => {
    show();
    await waitFor(() => expect(document.querySelector('[data-cite="3"]')).not.toBeNull());
    fireEvent.click(document.querySelector('[data-cite="3"]')!);
    const session = useReaderStore.getState().session;
    expect(session?.ownerKey).toBe('lesson:lesson-1:1');
    expect(session?.citations.map((source) => source.citationId)).toEqual([1, 2, 3]);
    expect(session?.index).toBe(2);
  });

  it('opens the reader from the Sources row too', async () => {
    show();
    const row = (await screen.findByText('Sources')).parentElement!;
    fireEvent.click(within(row).getByRole('button', { name: 'Citation 2: Field guide' }));
    // The footnote is a popover: its own button opens the source.
    fireEvent.click(await screen.findByRole('button', { name: /View source/ }));
    expect(useReaderStore.getState().session?.index).toBe(1);
  });

  it('says on hover that the learner confirmed a sentence, with their note', async () => {
    show();
    await waitFor(() => expect(document.querySelector('.claim-unverified')).not.toBeNull());
    fireEvent.mouseOver(document.querySelector('.claim-unverified')!);
    const card = await screen.findByRole('tooltip');
    expect(card).toHaveTextContent('Confirmed by you');
    expect(card).toHaveTextContent('Your note: Fine for beginners.');
  });

  it('names the sources a hovered sentence stands on, and what a source says against it', async () => {
    show();
    await waitFor(() => expect(document.querySelector('.claim-contradicted')).not.toBeNull());
    fireEvent.mouseOver(document.querySelector('.claim-contradicted')!);
    const card = await screen.findByRole('tooltip');
    expect(card).toHaveTextContent('Not verified');
    expect(card).toHaveTextContent('Checked.');
    expect(card).toHaveTextContent('Lecture notes');
  });

  it('shows no chip, no row and no card with citations hidden', async () => {
    show({ showCitations: false });
    await waitFor(() => expect(document.querySelector('.citation-hidden')).not.toBeNull());
    expect(document.querySelector('[data-cite]')).toBeNull();
    expect(document.querySelector('.claim')).toBeNull();
    expect(screen.queryByText('Sources')).not.toBeInTheDocument();
    expect(screen.getByTestId('wrapped')).toHaveTextContent(BOILING);
    fireEvent.mouseOver(screen.getByText(/Heat moves/));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    expect(useReaderStore.getState().session).toBeNull();
  });

  it.each([
    ['without a report', null],
    ['with a report that has no text for this section', report({ sections: [] })],
  ])('draws the body unmarked %s', async (_name, data) => {
    show({ data });
    await screen.findByText(/Boiling needs a vapour pressure/);
    expect(document.querySelector('[data-cite]')).toBeNull();
    expect(document.querySelector('.claim')).toBeNull();
    expect(screen.queryByText('Sources')).not.toBeInTheDocument();
    expect(screen.getByTestId('wrapped')).toHaveTextContent(BODY);
  });
});

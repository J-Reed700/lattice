import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import type {
  MessageVerificationSummary,
  RetrievalTrace,
  TurnRecord as TurnRecordData,
  TurnStep,
} from '@/types/conversation';

import { TurnRecord, formatDuration } from '../TurnRecord';

const step = (overrides: Partial<TurnStep> & Pick<TurnStep, 'id' | 'kind'>): TurnStep => ({
  label: 'Doing something',
  state: 'done',
  startedAtMs: 0,
  durationMs: 1200,
  ...overrides,
});

const record = (overrides: Partial<TurnRecordData> = {}): TurnRecordData => ({
  model: { id: 'qwen3-30b', name: 'qwen3-30b' },
  steps: [],
  timing: {
    totalMs: 12_400,
    routerMs: 0,
    retrievalMs: 0,
    generationMs: 0,
    verificationMs: 0,
    toolMs: 0,
  },
  tokens: { completion: null, contextUsed: null },
  router: null,
  ...overrides,
});

const trace = (overrides: Partial<RetrievalTrace> = {}): RetrievalTrace => ({
  searchedDocuments: 1247,
  passages: 8,
  files: 3,
  scope: 'vault',
  ...overrides,
});

const verification = (
  overrides: Partial<MessageVerificationSummary> = {}
): MessageVerificationSummary => ({
  enabled: true,
  claimsEvaluated: 5,
  supportedClaims: 4,
  ...overrides,
});

describe('TurnRecord', () => {
  it('says what was searched, how much is backed, which model and how long', () => {
    render(
      <TurnRecord
        trace={trace()}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={verification()}
      />
    );

    const line = screen.getByRole('button');
    expect(line).toHaveTextContent('Searched 1,247 documents');
    expect(line).toHaveTextContent('8 passages from 3 files');
    expect(line).toHaveTextContent('4 of 5 claims backed');
    expect(line).toHaveTextContent('qwen3-30b');
    expect(line).toHaveTextContent('12.4s');
  });

  /**
   * The honesty rule carried over from `RetrievalTrace`: a turn that did not
   * search did not search zero documents.
   */
  it('never claims to have searched zero documents', () => {
    render(
      <TurnRecord
        trace={trace({
          searchedDocuments: 0,
          passages: 0,
          files: 0,
          unavailableReason: 'the embedding model is not ready',
        })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    expect(screen.getByRole('button')).not.toHaveTextContent('Searched 0');
  });

  it('reports a web-only turn as web pages, not as files', () => {
    // The turn that prompted this: a chat in a space holding no documents,
    // answered entirely from the web. Counting those pages as "files" claimed
    // it had read ten of the user's documents, which reads as a scope leak.
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 0, passages: 0, files: 0, webPages: 10, scope: 'linked' })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Retrieved 10 web pages · this space');
    expect(container.textContent).not.toContain('file');
  });

  it('keeps documents and web pages apart on a mixed turn', () => {
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 43, passages: 6, files: 3, webPages: 2 })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Searched 43 documents · 6 passages from 3 files · 2 web pages');
  });

  it('shows recovered tool results without inventing a searched document count', () => {
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 0, passages: 7, files: 7 })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Retrieved 7 passages from 7 files');
    expect(container.textContent).not.toMatch(/Searched 0/);
  });

  it('shows a known file count when passage counts are unavailable', () => {
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 0, passages: 0, files: 7 })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Retrieved 7 files');
  });

  it('uses singular forms for one of each', () => {
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 1, passages: 1, files: 1 })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Searched 1 document · 1 passage from 1 file');
    expect(container.textContent).not.toMatch(/documents|passages|files/);
  });

  it('omits the passage clause when nothing was retrieved', () => {
    const { container } = render(
      <TurnRecord
        trace={trace({ searchedDocuments: 12, passages: 0, files: 0 })}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(container.textContent).toContain('Searched 12 documents');
    expect(container.textContent).not.toContain('passage');
  });

  it('renders nothing at all for a turn with no trace, no record and no steps', () => {
    const { container } = render(
      <TurnRecord
        trace={null}
        record={null}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    expect(container).toBeEmptyDOMElement();
  });

  /** A badge on every ordinary answer is noise, not disclosure. */
  it('says nothing about a plain sufficient verdict', async () => {
    render(
      <TurnRecord
        trace={trace({ kbSufficient: true, kbCorrectiveRetries: 0 })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    await userEvent.click(screen.getByRole('button'));

    expect(screen.queryByText(/evidence judged/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/searched again/i)).not.toBeInTheDocument();
  });

  it('notes a skipped planner, and combines it with a corrective pass', async () => {
    const { rerender } = render(
      <TurnRecord
        trace={trace({ kbPlannerSkipped: true })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    await userEvent.click(screen.getByRole('button'));
    expect(screen.getByText(/Reused the previous topic instead/)).toBeInTheDocument();

    rerender(
      <TurnRecord
        trace={trace({ kbPlannerSkipped: true, kbCorrectiveRetries: 1 })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );
    expect(screen.getByText(/Reused the previous topic, then searched again/)).toBeInTheDocument();
  });

  it('gives the sufficiency reasons and the corrective pass when opened', async () => {
    render(
      <TurnRecord
        trace={trace({
          kbSufficient: false,
          kbCorrectiveRetries: 1,
          sufficiencyReasons: ['low_term_coverage'],
        })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    await userEvent.click(screen.getByRole('button'));

    expect(screen.getByText(/searched again/i)).toBeInTheDocument();
    expect(screen.getByText(/missed most of your terms/i)).toBeInTheDocument();
  });

  it('shows the steps with their durations when opened', async () => {
    render(
      <TurnRecord
        trace={trace()}
        record={record({
          steps: [
            step({ id: 's0', kind: 'plan', label: 'Planning what to search for', durationMs: 820 }),
            step({
              id: 's1',
              kind: 'search_documents',
              label: 'Searching your documents',
              result: '8 passages from 3 files',
              durationMs: 2400,
            }),
          ],
        })}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    await userEvent.click(screen.getByRole('button'));

    const timeline = within(screen.getByRole('list'));
    expect(timeline.getByText('Planning what to search for')).toBeInTheDocument();
    expect(timeline.getByText('820ms')).toBeInTheDocument();
    expect(timeline.getByText('2.4s')).toBeInTheDocument();
    expect(timeline.getByText(/8 passages from 3 files/)).toBeInTheDocument();
  });

  it('shows the router rationale that used to be discarded', async () => {
    render(
      <TurnRecord
        trace={null}
        record={record({
          router: {
            action: 'use_last_document',
            confidence: 0.82,
            rationale: 'The question refers to "it" with no new subject.',
          },
        })}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    await userEvent.click(screen.getByRole('button'));

    expect(screen.getByText(/refers to "it" with no new subject/)).toBeInTheDocument();
  });

  /** A pending turn is the whole reason the steps are streamed at all. */
  it('opens itself while the turn is running and shows the live steps', () => {
    render(
      <TurnRecord
        trace={null}
        record={null}
        liveSteps={[
          step({ id: 's0', kind: 'plan', label: 'Planning what to search for' }),
          step({
            id: 's1',
            kind: 'search_documents',
            label: 'Searching your documents',
            state: 'running',
            durationMs: undefined,
          }),
        ]}
        isPending
        verification={null}
      />
    );

    expect(screen.getByRole('button')).toHaveAttribute('aria-expanded', 'true');
    // The header mirrors the running step while there is nothing else to say;
    // the list is the history.
    expect(
      within(screen.getByRole('list')).getByText('Searching your documents')
    ).toBeInTheDocument();
  });

  /** Once the model is writing, the answer is the progress indicator. */
  it('folds itself once generation starts', () => {
    render(
      <TurnRecord
        trace={null}
        record={null}
        liveSteps={[
          step({ id: 's0', kind: 'plan', label: 'Planning what to search for' }),
          step({ id: 's1', kind: 'generate', label: 'Thinking', state: 'running' }),
        ]}
        isPending
        verification={null}
      />
    );

    expect(screen.getByRole('button')).toHaveAttribute('aria-expanded', 'false');
  });

  it('can be opened and closed by hand', async () => {
    render(
      <TurnRecord
        trace={trace()}
        record={record({ steps: [step({ id: 's0', kind: 'verify', label: 'Checking the answer' })] })}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    const toggle = screen.getByRole('button');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
  });

  /**
   * Fail-closed focus, said out loud. Zero pinned documents means the request
   * named files this chat cannot reach — not that it searched everything.
   */
  it('says when a pinned turn could reach none of the documents it named', () => {
    render(
      <TurnRecord
        trace={trace({ searchedDocuments: 0, passages: 0, files: 0, focusedDocuments: 0 })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    expect(screen.getByRole('button')).toHaveTextContent('pinned to documents outside this space');
  });

  it('says how many documents a pinned turn was confined to', () => {
    render(
      <TurnRecord
        trace={trace({ searchedDocuments: 2, focusedDocuments: 2 })}
        record={record()}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    expect(screen.getByRole('button')).toHaveTextContent('pinned to 2 documents');
  });

  /** A retry is a step now, and must not appear as answer text anywhere. */
  it('shows a retry as its own step', async () => {
    render(
      <TurnRecord
        trace={null}
        record={record({
          steps: [
            step({
              id: 's0',
              kind: 'retry',
              label: 'Asking again — the model returned nothing',
              result: 'attempt 2',
            }),
          ],
        })}
        liveSteps={null}
        isPending={false}
        verification={null}
      />
    );

    await userEvent.click(screen.getByRole('button'));

    expect(screen.getByText(/Asking again/)).toBeInTheDocument();
    expect(screen.getByText(/attempt 2/)).toBeInTheDocument();
  });
});

describe('formatDuration', () => {
  it('reads as milliseconds, seconds, then minutes', () => {
    expect(formatDuration(820)).toBe('820ms');
    expect(formatDuration(2400)).toBe('2.4s');
    expect(formatDuration(64_000)).toBe('1m 04s');
  });
});

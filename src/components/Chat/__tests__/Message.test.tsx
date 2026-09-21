import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useChatReaderStore } from '@/stores/chatReaderStore';
import type { MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';

import { Message } from '../Message';

const DEFAULT_SUMMARY: MessageVerificationSummary = {
  enabled: true,
  claimsEvaluated: 2,
  supportedClaims: 1,
  supportedClaimNotes: ['Supported statement'],
  unsupportedClaims: ['Unverified statement'],
};

let verificationSummary: MessageVerificationSummary = DEFAULT_SUMMARY;

vi.mock('../../../stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) => selector({
    messageVerification: new Map([['answer', verificationSummary]]),
    messageBookmarkMap: new Map(), lastMessageSources: new Map(), messageRetrieval: new Map(),
    liveRetrieval: new Map(), liveSteps: new Map(), messageTurn: new Map(), inFlightGenerations: new Map(), conversations: [],
  }),
}));
vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return { ...actual, useNavigate: () => vi.fn() };
});
vi.mock('@/hooks/queries' , () => ({ useSettingsQuery: () => ({ data: undefined }), usePassageReferenceIds: () => [] }));
vi.mock('@/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ activeModel: null }) }));
// The chips the answer body draws are what the reader is opened from, so the
// viewer stands in for tiptap by drawing them.
vi.mock('../../TiptapEditor', () => ({
  TiptapViewer: ({ citationNumbers, claims }: { citationNumbers?: number[]; claims?: { sentence: string }[] }) => (
    <div>
      Answer body
      {(citationNumbers ?? []).map((number) => (
        <span key={number} className="cite-chip" data-cite={number}>{number}</span>
      ))}
      {(claims ?? []).map((claim, index) => (
        // Marked, not repeated: the real viewer decorates text already in the answer.
        <span key={claim.sentence} className="claim" data-claim={index}>checked sentence</span>
      ))}
    </div>
  ),
}));
// What the popover offers is its own file's business; here, only that it opens.
vi.mock('../actions/ClaimActionsPopover', () => ({
  ClaimActionsPopover: ({ verdict }: { verdict: { sentence: string } }) => (
    <div role="dialog" aria-label="Claim actions">{verdict.sentence}</div>
  ),
}));
vi.mock('../MessageActions', () => ({ MessageActions: () => null }));
vi.mock('../turn/TurnRecord', () => ({ TurnRecord: () => null }));
// Owned by another track; this file is about what the answer itself does.
vi.mock('../EvidenceMargin', () => ({ EvidenceMargin: () => null }));
vi.mock('../SourceCitations', () => ({ SourceCitations: () => null }));

const source = (number: number): SourceWithMetadata => ({
  documentId: `document-${number}`,
  chunkId: `chunk-${number}`,
  fileName: `Source ${number}.md`,
  filePath: `/vault/source-${number}.md`,
  mimeType: 'text/markdown',
  category: 'note',
  content: `Passage ${number}`,
  score: 0.5,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: number,
});

function renderAnswer(sources?: SourceWithMetadata[]) {
  render(<Message message={{ id: 'answer', role: 'assistant', content: 'Answer', createdAt: new Date().toISOString(), conversationId: 'conversation', ...(sources ? { sources } : {}) } as Parameters<typeof Message>[0]['message']} />);
}

beforeEach(() => {
  verificationSummary = DEFAULT_SUMMARY;
  useChatReaderStore.setState({ session: null, resolvedLocations: new Map() });
});

describe('message verification disclosure', () => {
  it('opens details above the answer on click and closes on a second click', () => {
    renderAnswer();
    const badge = screen.getByRole('button', { name: /Partially verified/ });
    fireEvent.click(badge);
    expect(screen.getByRole('region', { name: 'Verification details' })).toBeVisible();
    expect(screen.getByText('Unverified statement')).toBeVisible();
    expect(badge).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(badge);
    expect(screen.queryByRole('region', { name: 'Verification details' })).not.toBeInTheDocument();
  });

  it('reports a contradiction as worse than an ungrounded claim', () => {
    // The backend lists contradictions inside `unsupportedClaims` too, so the
    // badge must not report the stronger finding as the milder one.
    verificationSummary = {
      enabled: true,
      claimsEvaluated: 3,
      supportedClaims: 1,
      supportedClaimNotes: ['Supported statement'],
      unsupportedClaims: ['Refuted statement', 'Unverified statement'],
      contradictedClaims: ['Refuted statement'],
      verdictCounts: { supported: 1, contradicted: 1, unsupported: 1 },
      claimVerdicts: [
        {
          sentence: 'Refuted statement',
          citationIds: [1],
          verdict: 'contradicted',
          evidenceQuote: 'The report states the opposite.',
          method: 'judge',
        },
      ],
      judgeUsed: true,
    };
    renderAnswer();

    const badge = screen.getByRole('button', { name: /Contradicted · 1/ });
    expect(screen.queryByRole('button', { name: /Partially verified/ })).not.toBeInTheDocument();

    fireEvent.click(badge);
    expect(screen.getByText('Contradicted by your sources')).toBeVisible();
    // The evidence the judge read, so the verdict can be checked rather than
    // taken on faith.
    expect(screen.getByText(/The report states the opposite\./)).toBeVisible();
    // The contradicted claim is not repeated in the unverified column.
    expect(screen.getByText('Unverified statement')).toBeVisible();
    expect(screen.getAllByText('Refuted statement')).toHaveLength(1);
  });

  it('keeps the old badge for messages verified before the judge existed', () => {
    // No `contradictedClaims` at all is "not judged", never "nothing was
    // contradicted".
    renderAnswer();
    expect(screen.getByRole('button', { name: /Partially verified · 1/ })).toBeInTheDocument();
  });
});

describe('a checked sentence of the answer', () => {
  const summaryWithVerdict: MessageVerificationSummary = {
    ...DEFAULT_SUMMARY,
    claimVerdicts: [
      { sentence: 'Canopy cools streets by 1.2 °C.', citationIds: [1], verdict: 'unsupported', method: 'judge' },
    ],
  };

  it('offers its actions when it is clicked', () => {
    verificationSummary = summaryWithVerdict;
    renderAnswer([source(1)]);

    fireEvent.click(document.querySelector('[data-claim="0"]')!);

    expect(screen.getByRole('dialog', { name: 'Claim actions' })).toHaveTextContent('Canopy cools streets');
    // A sentence is not a citation: the reader stays shut.
    expect(useChatReaderStore.getState().session).toBeNull();
  });

  it('is not drawn when verification was off for the turn', () => {
    verificationSummary = { ...summaryWithVerdict, enabled: false };
    renderAnswer([source(1)]);

    expect(document.querySelector('[data-claim]')).toBeNull();
  });
});

describe('opening the source reader from an answer', () => {
  it('hands the reader every citation on the answer, opened at the chip that was clicked', () => {
    renderAnswer([source(1), source(2)]);

    fireEvent.click(document.querySelector('[data-cite="2"]')!);

    const session = useChatReaderStore.getState().session;
    expect(session?.ownerKey).toBe('answer');
    expect(session?.index).toBe(1);
    // All of them, so `[` / `]` can travel the whole answer from here.
    expect(session?.citations.map((citation) => citation.chunkId)).toEqual(['chunk-1', 'chunk-2']);
  });

  it('lights the citation the reader is showing, and only in the answer it came from', () => {
    renderAnswer([source(1), source(2)]);
    const chip = () => document.querySelector('[data-cite="2"]')!;

    act(() => {
      useChatReaderStore.getState().open('another-answer', [source(2)], 0);
    });
    // Same number, different answer: that is a different passage.
    expect(chip().classList.contains('is-lit')).toBe(false);

    act(() => {
      useChatReaderStore.getState().open('answer', [source(1), source(2)], 1);
    });
    expect(chip().classList.contains('is-lit')).toBe(true);

    act(() => {
      useChatReaderStore.getState().close();
    });
    expect(chip().classList.contains('is-lit')).toBe(false);
  });
});

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { MessageVerificationSummary } from '@/types/conversation';

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
    liveRetrieval: new Map(), inFlightGenerations: new Map(),
  }),
}));
vi.mock('@/hooks/queries' , () => ({ useSettingsQuery: () => ({ data: undefined }), usePassageReferenceIds: () => [] }));
vi.mock('@/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ activeModel: null }) }));
vi.mock('../../TiptapEditor', () => ({ TiptapViewer: () => <div>Answer body</div> }));
vi.mock('../FilePreviewModal', () => ({ FilePreviewModal: () => null }));
vi.mock('../MessageActions', () => ({ MessageActions: () => null }));
vi.mock('../RetrievalTrace', () => ({ RetrievalTrace: () => null }));

function renderAnswer() {
  render(<Message message={{ id: 'answer', role: 'assistant', content: 'Answer', createdAt: new Date().toISOString(), conversationId: 'conversation' } as Parameters<typeof Message>[0]['message']} />);
}

beforeEach(() => {
  verificationSummary = DEFAULT_SUMMARY;
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

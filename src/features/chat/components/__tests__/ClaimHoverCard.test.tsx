import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ClaimHoverCard } from '@/features/chat/components/ClaimHoverCard';
import type { ClaimVerdict } from '@/types/conversation';


const hover = { index: 0, rect: new DOMRect(40, 400, 200, 20), maxRight: 800 };

function renderCard(verdict: ClaimVerdict) {
  render(<ClaimHoverCard hover={hover} verdict={verdict} citationMap={new Map()} />);
}

describe('ClaimHoverCard', () => {
  it('says a claim nothing checked was not checked, and why', () => {
    renderCard({
      sentence: 'Yields rose by 42 percent.',
      citationIds: [1],
      verdict: 'unverified',
      method: 'lexical',
      unverifiedReason: 'budget',
    });
    expect(screen.getByText('Not checked')).toBeInTheDocument();
    expect(screen.queryByText('Not found in the cited passage')).not.toBeInTheDocument();
    expect(screen.getByText(/ran out of time/)).toBeInTheDocument();
  });

  it('names a page with no saved text as the reason', () => {
    renderCard({
      sentence: 'The page says so.',
      citationIds: [2],
      verdict: 'unverified',
      method: 'lexical',
      unverifiedReason: 'no_text',
    });
    expect(screen.getByText(/no saved text/)).toBeInTheDocument();
  });

  it('gives the judge\'s probability when it has one', () => {
    renderCard({
      sentence: 'Tomatoes need 6-8 hours of sun.',
      citationIds: [1],
      verdict: 'supported',
      method: 'judge',
      confidence: 0.93,
    });
    expect(screen.getByText('Backed by the source')).toBeInTheDocument();
    expect(screen.getByText(/93% sure/)).toBeInTheDocument();
  });

  it('explains a contradiction with the comparison and the source passage', () => {
    renderCard({
      sentence: 'The treatment improved cold tolerance.',
      citationIds: [1],
      verdict: 'contradicted',
      method: 'judge',
      confidence: 0.91,
      reason: 'The claim says improved; the source says reduced.',
      evidenceQuote: 'The treatment reduced cold tolerance by 12 percent.',
    });
    expect(screen.getByText('The claim says improved; the source says reduced.')).toBeInTheDocument();
    expect(screen.getByText('What the source says')).toBeInTheDocument();
    expect(screen.getByText(/reduced cold tolerance by 12 percent/)).toBeInTheDocument();
  });

  it('does not present a low-confidence check as a finding', () => {
    renderCard({
      sentence: 'The treatment improved cold tolerance.',
      citationIds: [1],
      verdict: 'unverified',
      method: 'judge',
      confidence: 0.46,
      unverifiedReason: 'low_confidence',
    });
    expect(screen.getByText('Not checked')).toBeInTheDocument();
    expect(screen.getByText(/not confident enough/)).toBeInTheDocument();
    expect(screen.getByText(/46% sure/)).toBeInTheDocument();
    expect(screen.queryByText('The source says otherwise')).not.toBeInTheDocument();
  });
});

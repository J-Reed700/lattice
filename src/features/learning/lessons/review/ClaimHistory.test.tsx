import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ClaimHistory } from '@/features/learning/lessons/review/ClaimHistory';
import type { ClaimEventDto } from '@/lib/bindings';

const AT = 1_760_000_000_000;
const event = (ordinal: number, overrides: Partial<ClaimEventDto> = {}): ClaimEventDto => ({
  ordinal, actor: 'machine', at: AT + ordinal * 60_000, jobId: null, appVersion: 'test', checkerModel: null,
  event: { kind: 'researched', queries: ['monarch migration'], sourcesAdded: [], passagesFound: 1 }, ...overrides,
});

describe('ClaimHistory', () => {
  it('says nothing has been recorded for a claim with no events', () => {
    render(<ClaimHistory history={[]} />);
    expect(screen.getByText('Nothing has been recorded for this claim yet.')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('lists each event in order with who did it and a timestamp', () => {
    render(<ClaimHistory history={[
      event(1),
      event(2, { actor: 'user', event: { kind: 'user_action', action: { action: 'remove' }, note: null } }),
    ]} />);
    const items = within(screen.getByRole('list', { name: 'Claim history' })).getAllByRole('listitem');
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent('Searched: monarch migration · 1 passage found');
    expect(items[1]).toHaveTextContent('Removed the sentence');
    expect(items[0].querySelector('time')).toHaveAttribute('dateTime', new Date(AT + 60_000).toISOString());
    expect(items[0].textContent).not.toBe(items[1].textContent);
  });

  it('names the checker model on events other than a check', () => {
    render(<ClaimHistory history={[event(1, { checkerModel: 'qwen-9b' })]} />);
    expect(screen.getByText(/checker qwen-9b/)).toBeInTheDocument();
  });

  it('does not repeat the checker model on a check, which already names it', () => {
    render(<ClaimHistory history={[event(1, {
      checkerModel: 'qwen-9b',
      event: { kind: 'checked', check: { checker: { role: 'local', model: 'qwen-9b' }, reason: '', fingerprint: 'f', evidence: [], windowTokens: 1, result: { verdict: 'failed', kind: { kind: 'unsupported' } } } },
    })]} />);
    expect(screen.queryByText(/· checker qwen-9b/)).not.toBeInTheDocument();
    expect(screen.getByText(/Checker \(qwen-9b\)/)).toBeInTheDocument();
  });
});

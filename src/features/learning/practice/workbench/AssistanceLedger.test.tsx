import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { AssistanceLedger } from './AssistanceLedger';

const event = (overrides: Partial<Wire.LearningPracticeSessionDto['assistance'][number]>) => ({
  id: 'e1', kind: 'mode_changed', mode: 'practice', artifactRevision: 3, details: {}, createdAt: 1, ...overrides,
}) as Wire.LearningPracticeSessionDto['assistance'][number];

describe('AssistanceLedger', () => {
  it('speaks of help, with the help word and no raw mode or revision', () => {
    render(<AssistanceLedger events={[event({})]} />);
    expect(screen.getByText('Changed help')).toBeVisible();
    expect(screen.getByText(/Hints only/)).toBeVisible();
    expect(screen.queryByText(/practice ·|revision|aid contract/i)).not.toBeInTheDocument();
    expect(screen.getByText(/alongside the help used/)).toBeVisible();
  });

  it('calls an opened source a saved one', () => {
    render(<AssistanceLedger events={[event({ kind: 'source_opened' })]} />);
    expect(screen.getByText('Opened a saved source')).toBeVisible();
  });

  it('says what an empty ledger will list', () => {
    render(<AssistanceLedger events={[]} />);
    expect(screen.getByText(/help changes/)).toBeVisible();
  });
});

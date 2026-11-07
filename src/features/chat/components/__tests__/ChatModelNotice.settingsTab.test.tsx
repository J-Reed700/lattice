import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it } from 'vitest';

import { ChatModelNotice } from '@/features/chat/components/ChatModelNotice';

/**
 * "Open model settings" is the line's fix for a model problem, so it opens the
 * tab where the chat model is chosen, not Settings' first tab (Search).
 */

describe('ChatModelNotice settings links', () => {
  it('opens the models tab when document search was skipped for a model reason', () => {
    render(<MemoryRouter><ChatModelNotice hasChatModel warmupPhase="ready" retrievalUnavailableReason="the embedding model is not ready" /></MemoryRouter>);
    expect(screen.getByRole('link', { name: 'Open model settings' })).toHaveAttribute('href', '/settings?tab=downloaded-models');
  });

  it('still sends a document scope problem to the library', () => {
    render(<MemoryRouter><ChatModelNotice hasChatModel warmupPhase="ready" retrievalUnavailableReason="no indexed documents are assigned to this conversation’s space" /></MemoryRouter>);
    expect(screen.getByRole('link', { name: 'Review documents' })).toHaveAttribute('href', '/files');
  });
});

import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it } from 'vitest';

import { ChatModelNotice } from '../ChatModelNotice';

import type { ChatModelNoticeProps } from '../ChatModelNotice';

const renderNotice = (props: Partial<ChatModelNoticeProps> = {}) =>
  render(
    <MemoryRouter>
      <ChatModelNotice hasChatModel warmupPhase="ready" {...props} />
    </MemoryRouter>
  );

describe('ChatModelNotice', () => {
  it('renders nothing when everything is healthy', () => {
    const { container } = renderNotice();
    expect(container).toBeEmptyDOMElement();
  });

  it('says the model did not load, with the way to fix it', () => {
    renderNotice({ warmupPhase: 'failed' });
    expect(screen.getByText("The chat model didn't load.")).toBeInTheDocument();
    expect(screen.getByText('Open model settings')).toHaveAttribute('href', '/settings');
  });

  it('says there is no model rather than claiming a download', () => {
    renderNotice({ hasChatModel: false, warmupPhase: 'idle' });
    expect(screen.getByText('No chat model yet.')).toBeInTheDocument();
    expect(screen.getByText('Choose a model')).toHaveAttribute('href', '/settings');
  });

  it('says it is warming up, with no action to take', () => {
    renderNotice({ warmupPhase: 'started' });
    expect(screen.getByText('Warming up the model…')).toBeInTheDocument();
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
  });

  it('surfaces the backend reason retrieval was skipped', () => {
    renderNotice({ retrievalUnavailableReason: 'the embedding model is not ready' });
    expect(
      screen.getByText('Document search was unavailable for the last answer: the embedding model is not ready.')
    ).toBeInTheDocument();
  });

  /**
   * The regression guard for the bug this component replaces: an Ollama-only
   * install has no `is_active_for_chat` row, but the conversation controller
   * will happily use it. The old placeholder said "Chat model is downloading…"
   * forever and left the send button dead.
   */
  it('renders nothing for a working Ollama-only setup', () => {
    const { container } = renderNotice({ hasChatModel: true, warmupPhase: 'idle' });
    expect(container).toBeEmptyDOMElement();
  });

  it('links a document scope problem to the library', () => {
    renderNotice({ retrievalUnavailableReason: 'no indexed documents are assigned to this conversation’s space' });
    expect(screen.getByRole('link', { name: 'Review documents' })).toHaveAttribute('href', '/files');
    expect(screen.queryByText('Open model settings')).not.toBeInTheDocument();
  });

  it('reports the most blocking problem first', () => {
    renderNotice({
      hasChatModel: false,
      warmupPhase: 'failed',
      retrievalUnavailableReason: 'the embedding model is not ready',
    });
    expect(screen.getByText("The chat model didn't load.")).toBeInTheDocument();
    expect(screen.queryByText('No chat model yet.')).not.toBeInTheDocument();
  });
});

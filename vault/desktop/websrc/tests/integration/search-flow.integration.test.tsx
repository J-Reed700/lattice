import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';

import { QueryRewritePanel } from '../../components/QueryRewritePanel/QueryRewritePanel';
import { mockIPC } from '../setup';

describe('Search to Query Rewrite Flow', () => {
  const listenMock = vi.mocked(listen);
  let llmStreamHandler: ((event: { payload: any }) => void) | null = null;

  const emitStream = (payload: any) => {
    const handler = llmStreamHandler;
    if (!handler) {
      throw new Error('LLM stream handler not initialized');
    }
    act(() => {
      handler({ payload });
    });
  };

  beforeEach(() => {
    vi.clearAllMocks();
    llmStreamHandler = null;
    listenMock.mockImplementation((eventName, handler: any) => {
      if (eventName === 'llm-stream') {
        llmStreamHandler = handler;
      }
      return Promise.resolve(() => {});
    });
  });

  it('shows query rewrite panel and allows variant selection', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    const originalQuery = 'machine learning basics';

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery={originalQuery}
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate
      />
    );

    expect(screen.getByText('Query Suggestions')).toBeInTheDocument();
    expect(screen.getByText(originalQuery)).toBeInTheDocument();

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('ask_question_stream', expect.any(Object));
    });

    emitStream({
      type: 'token',
      content: '1. "machine learning fundamentals"\nReasoning: More specific terminology\n\n',
    });
    emitStream({
      type: 'token',
      content: '2. "introduction to ML"\nReasoning: Beginner-friendly\n\n',
    });
    emitStream({
      type: 'token',
      content: '3. "ML tutorial"\nReasoning: Practical approach',
    });
    emitStream({ type: 'done' });

    const variant1Label = await screen.findByText(/Variant 1/i, {}, { timeout: 1000 });
    const variant1 = variant1Label.closest('div[class*="cursor-pointer"]');
    expect(variant1).toBeInTheDocument();
    expect(variant1).toHaveTextContent(/machine\s+learning\s+fundamentals/i);

    if (variant1) {
      await user.click(variant1);
      expect(onVariantSelect).toHaveBeenCalledWith(expect.stringContaining('fundamentals'));
    }
  });

  it('handles keyboard shortcuts for variant selection', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="test query"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('handles regenerate button click', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="test query"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Regenerate')).toBeInTheDocument();
    });

    const regenerateBtn = screen.getByText('Regenerate');
    await user.click(regenerateBtn);

    expect(mockIPC).toHaveBeenCalledWith('ask_question_stream', expect.any(Object));
  });

  it('displays error when LLM fails', async () => {
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockRejectedValue(new Error('Ollama not available'));

    render(
      <QueryRewritePanel
        originalQuery="test query"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Failed to generate suggestions/i)).toBeInTheDocument();
    });

    expect(screen.getByText(/Make sure Ollama is running/i)).toBeInTheDocument();
  });

  it('handles empty original query', async () => {
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery=""
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    expect(screen.getByText(/Query Suggestions/i)).toBeInTheDocument();
  });

  it('allows closing panel with Escape key', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="test query"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('displays multiple query variants with descriptions', async () => {
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="original"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate
      />
    );

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('ask_question_stream', expect.any(Object));
    });

    emitStream({
      type: 'token',
      content: '1. "Variant A"\nReasoning: First variant\n\n',
    });
    emitStream({
      type: 'token',
      content: '2. "Variant B"\nReasoning: Second variant\n\n',
    });
    emitStream({ type: 'done' });

    const variant1Label = await screen.findByText(/Variant 1/i);
    const variant2Label = await screen.findByText(/Variant 2/i);
    const variant1 = variant1Label.closest('div[class*="cursor-pointer"]');
    const variant2 = variant2Label.closest('div[class*="cursor-pointer"]');
    expect(variant1).toBeInTheDocument();
    expect(variant2).toBeInTheDocument();
    expect(variant1).toHaveTextContent(/Variant\s+A/);
    expect(variant2).toHaveTextContent(/Variant\s+B/);
  });

  it('handles variant selection and calls callback with query', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="base query"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate
      />
    );

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('ask_question_stream', expect.any(Object));
    });

    emitStream({
      type: 'token',
      content: '1. "selected variant"\nReasoning: This one\n\n',
    });
    emitStream({ type: 'done' });

    const variant1Label = await screen.findByText(/Variant 1/i);
    const variantCard = variant1Label.closest('div[class*="cursor-pointer"]');
    expect(variantCard).toBeInTheDocument();
    expect(variantCard).toHaveTextContent(/selected\s+variant/i);
    if (variantCard) {
      await user.click(variantCard);
      expect(onVariantSelect).toHaveBeenCalled();
    }
  });

  it('hides panel when isVisible is false', () => {
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    const { rerender } = render(
      <QueryRewritePanel
        originalQuery="test"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    expect(screen.getByText(/Query Suggestions/i)).toBeInTheDocument();

    rerender(
      <QueryRewritePanel
        originalQuery="test"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible={false}
        autoGenerate={false}
      />
    );

    expect(screen.queryByText(/Query Suggestions/i)).not.toBeInTheDocument();
  });

  it('handles rapid variant generation calls', async () => {
    const user = userEvent.setup();
    const onVariantSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue(null);

    render(
      <QueryRewritePanel
        originalQuery="test"
        onVariantSelect={onVariantSelect}
        onClose={onClose}
        isVisible
        autoGenerate={false}
      />
    );

    const regenerateBtn = screen.getByText('Regenerate');
    await user.click(regenerateBtn);
    await user.click(regenerateBtn);

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('ask_question_stream', expect.any(Object));
    });
  });
});

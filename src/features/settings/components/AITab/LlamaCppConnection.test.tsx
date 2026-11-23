import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LlamaCppConnection } from './LlamaCppConnection';

const h = vi.hoisted(() => ({
  llm: {
    llmSettings: null as null | Record<string, unknown>,
    isLoading: false,
    authValueSet: { ollama: false, llamaCpp: false },
    authValueStatus: 'ready' as 'loading' | 'error' | 'ready',
    saveLlmUpdates: vi.fn(),
    saveAuthHeaderValue: vi.fn(),
    reload: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

describe('LlamaCppConnection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.llm.authValueSet = { ollama: false, llamaCpp: false };
    h.llm.authValueStatus = 'ready';
    h.llm.saveLlmUpdates.mockResolvedValue(true);
    h.llm.saveAuthHeaderValue.mockResolvedValue(true);
    h.llm.llmSettings = {
      llamaCpp: {
        url: 'http://localhost:8080',
        model: 'qwen',
        authHeaderName: '',
      },
    };
  });

  it('shows Clear header and inline warning when header name is saved without a Keychain value', async () => {
    h.llm.llmSettings = {
      llamaCpp: {
        url: 'http://localhost:8080',
        model: 'qwen',
        authHeaderName: 'Authorization',
      },
    };
    h.llm.authValueSet = { ollama: false, llamaCpp: false };

    render(<LlamaCppConnection />);

    expect(
      screen.getByText('Auth header name is saved without a value in Keychain. Enter a value or click Clear header.'),
    ).toBeInTheDocument();
    const clearButton = screen.getByRole('button', { name: 'Clear header' });
    fireEvent.click(clearButton);

    await waitFor(() =>
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({
        llamaCpp: { url: 'http://localhost:8080', model: 'qwen', authHeaderName: '' },
      }),
    );
    expect(h.llm.saveAuthHeaderValue).toHaveBeenCalledWith('llamaCpp', null);
  });

  it('hides Clear header and warning when neither header name nor Keychain value is set', () => {
    render(<LlamaCppConnection />);
    expect(screen.queryByRole('button', { name: 'Clear header' })).not.toBeInTheDocument();
    expect(
      screen.queryByText('Auth header name is saved without a value in Keychain. Enter a value or click Clear header.'),
    ).not.toBeInTheDocument();
  });

  it('does not show broken-header warning or Clear header while Keychain status is loading or errored', async () => {
    h.llm.llmSettings = {
      llamaCpp: {
        url: 'http://localhost:8080',
        model: 'qwen',
        authHeaderName: 'Authorization',
      },
    };
    h.llm.authValueSet = { ollama: false, llamaCpp: false };
    h.llm.authValueStatus = 'loading';

    const { rerender } = render(<LlamaCppConnection />);
    expect(
      screen.queryByText('Auth header name is saved without a value in Keychain. Enter a value or click Clear header.'),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear header' })).not.toBeInTheDocument();
    expect(screen.getByText('Checking Keychain for the saved header value…')).toBeInTheDocument();

    h.llm.authValueStatus = 'error';
    rerender(<LlamaCppConnection />);
    expect(
      screen.queryByText('Auth header name is saved without a value in Keychain. Enter a value or click Clear header.'),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear header' })).not.toBeInTheDocument();
    expect(screen.getByText('Couldn’t check Keychain status. Saved header settings are preserved.')).toBeInTheDocument();

    // Updating URL/model while Keychain status is unavailable still saves cleanly.
    fireEvent.change(screen.getByLabelText('llama.cpp URL'), { target: { value: 'http://localhost:9090' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save connection' }));
    await waitFor(() =>
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({
        llamaCpp: { url: 'http://localhost:9090', model: 'qwen', authHeaderName: 'Authorization' },
      }),
    );
  });
});

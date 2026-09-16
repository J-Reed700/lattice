import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { HuggingFaceSettings } from './HuggingFaceSettings';

const { mockGetStatus, mockSetToken, mockDeleteToken } = vi.hoisted(() => ({
  mockGetStatus: vi.fn(),
  mockSetToken: vi.fn(),
  mockDeleteToken: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  __esModule: true,
  default: {
    getHuggingFaceTokenStatus: mockGetStatus,
    setHuggingFaceToken: mockSetToken,
    deleteHuggingFaceToken: mockDeleteToken,
  },
}));

const TOKEN = 'hf_abcdefghijklmnopqrstuvwxyz0123456789';

function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<HuggingFaceSettings />, { wrapper });
}

describe('HuggingFaceSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSetToken.mockResolvedValue({ ok: true, data: undefined });
    mockDeleteToken.mockResolvedValue({ ok: true, data: undefined });
  });

  it('offers to add a token when none is stored', async () => {
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: false } });
    renderSection();

    expect(
      await screen.findByText('Optional. Public models can be browsed and downloaded without a token. Gated downloads require one.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Token')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove token' })).not.toBeInTheDocument();
  });

  it('says a token is set without inventing a suffix', async () => {
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: true } });
    renderSection();

    expect(await screen.findByText('Replace token')).toBeInTheDocument();
    expect(screen.getByText('Token set.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove token' })).toBeInTheDocument();
  });

  it('hides the token by default and reveals it only on request', async () => {
    const user = userEvent.setup();
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: false } });
    const { container } = renderSection();

    const input = await screen.findByPlaceholderText('hf_...');
    expect(input).toHaveAttribute('type', 'password');

    await user.click(screen.getByRole('button', { name: 'Show token' }));
    expect(container.querySelector('#hf-token')).toHaveAttribute('type', 'text');
  });

  it('saves the trimmed token', async () => {
    const user = userEvent.setup();
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: false } });
    renderSection();

    const input = await screen.findByPlaceholderText('hf_...');
    await user.type(input, `  ${TOKEN}  `);
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(mockSetToken).toHaveBeenCalledWith(TOKEN));
  });

  it('never leaves the token on screen after a save', async () => {
    const user = userEvent.setup();
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: false } });
    const { container } = renderSection();

    const input = await screen.findByPlaceholderText('hf_...');
    await user.type(input, TOKEN);
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(mockSetToken).toHaveBeenCalled());
    await waitFor(() => expect(input).toHaveValue(''));
    expect(container.textContent ?? '').not.toContain(TOKEN);
  });

  it('removes a stored token', async () => {
    const user = userEvent.setup();
    mockGetStatus.mockResolvedValue({ ok: true, data: { isSet: true } });
    renderSection();

    await user.click(await screen.findByRole('button', { name: 'Remove token' }));

    await waitFor(() => expect(mockDeleteToken).toHaveBeenCalled());
  });
});

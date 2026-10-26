import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { KnowledgePanel } from '@/features/chat/components/KnowledgePanel';
import type { KnowledgeItemDto } from '@/lib/bindings';

const manageKnowledge = vi.hoisted(() => vi.fn());
vi.mock('@/lib/api', () => {
  const mockApi = { manageKnowledge };
  return { VaultAPI: mockApi, default: mockApi };
});

const item = (overrides: Partial<KnowledgeItemDto> = {}): KnowledgeItemDto => ({
  id: 'fact-1', conversationId: 'one', conversationTitle: 'Herb garden', label: 'Basil needs full sun', kind: 'user_fact', state: 'active', scope: 'space',
  learnedAt: '2026-05-01T12:00:00Z', validFrom: '2026-05-01T00:00:00Z', validUntil: '2099-01-01T00:00:00Z', verifiedAt: '2026-05-02T00:00:00Z', forgotten: false, supersededBy: null,
  evidence: [{ messageId: 'm1', sequence: 3, role: 'user', purpose: 'assertion', startByte: 0, endByte: 5, text: null }],
  availabilityReason: 'Shared with this space',
  ...overrides,
} as KnowledgeItemDto);

const respond = (items: KnowledgeItemDto[], hasMore = false) => ({ ok: true, data: { items, hasMore, lastAnswerMemoryIds: [] } });
const requests = () => manageKnowledge.mock.calls.map(([request]) => request);

const renderPanel = (onOpenConversation?: (id: string) => void) => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>
    <KnowledgePanel conversationId="one" onOpenConversation={onOpenConversation} />
  </QueryClientProvider>,
);

describe('KnowledgePanel actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    manageKnowledge.mockImplementation(async (request: { action: string }) => (request.action === 'list' ? respond([item()]) : { ok: true, data: { items: [], hasMore: false, lastAnswerMemoryIds: [] } }));
  });

  it('shows dates, a check mark and evidence whose source changed', async () => {
    renderPanel();
    expect(await screen.findByText('Basil needs full sun')).toBeInTheDocument();
    expect(screen.getByText('Learned 2026-05-01 · From 2026-05-01 · Until 2099-01-01 · Checked 2026-05-02')).toBeInTheDocument();
    expect(screen.getByText('Source changed or was removed.')).toBeInTheDocument();
  });

  it('marks an item checked, reshares it and forgets it', async () => {
    renderPanel();
    await screen.findByText('Basil needs full sun');
    fireEvent.click(screen.getByRole('button', { name: 'Mark checked' }));
    await waitFor(() => expect(requests().some((request) => request.action === 'verify' && request.itemId === 'fact-1')).toBe(true));
    fireEvent.change(screen.getByRole('combobox', { name: 'Sharing for Basil needs full sun' }), { target: { value: 'personal' } });
    await waitFor(() => expect(requests().some((request) => request.action === 'scope' && request.scope === 'personal')).toBe(true));
    fireEvent.click(screen.getByRole('button', { name: 'Forget saved item' }));
    await waitFor(() => expect(requests().some((request) => request.action === 'forget')).toBe(true));
  });

  it('corrects an item with dates, or cancels the correction', async () => {
    renderPanel();
    await screen.findByText('Basil needs full sun');
    fireEvent.click(screen.getByRole('button', { name: 'Correct' }));
    expect(screen.getByLabelText('Memory text')).toHaveValue('Basil needs full sun');
    expect(screen.getByLabelText('Valid from')).toHaveValue('2026-05-01');
    expect(screen.queryByLabelText('Memory type')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel correction' }));
    expect(screen.getByLabelText('Memory text')).toHaveValue('');
    fireEvent.click(screen.getByRole('button', { name: 'Correct' }));
    fireEvent.change(screen.getByLabelText('Valid until'), { target: { value: '' } });
    fireEvent.change(screen.getByLabelText('Memory text'), { target: { value: 'Basil needs six hours of sun' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save correction' }));
    await waitFor(() => expect(requests().some((request) => request.action === 'correct' && request.itemId === 'fact-1' && request.validUntil === null && request.validFrom === '2026-05-01T00:00:00Z')).toBe(true));
    await waitFor(() => expect(screen.getByLabelText('Memory text')).toHaveValue(''));
  });

  it('marks an expired or forgotten item and offers no edits on it', async () => {
    manageKnowledge.mockImplementation(async () => respond([
      item({ id: 'old', label: 'Frost date is April 1', validUntil: '2020-01-01T00:00:00Z', validFrom: null, verifiedAt: null }),
      item({ id: 'gone', label: 'Old plot size', forgotten: true }),
    ]));
    renderPanel();
    expect(await screen.findByText(/· expired$/)).toBeInTheDocument();
    expect(screen.getByText(/forgotten/)).toBeInTheDocument();
    expect(screen.getByText(/Not manually verified/)).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Correct' })).toHaveLength(1);
  });

  it('opens the source conversation of a shared item', async () => {
    manageKnowledge.mockImplementation(async () => respond([item({ conversationId: 'other' })]));
    const onOpenConversation = vi.fn();
    renderPanel(onOpenConversation);
    fireEvent.click(await screen.findByRole('button', { name: 'Open source conversation' }));
    expect(onOpenConversation).toHaveBeenCalledWith('other');
  });

  it('pages through saved knowledge', async () => {
    manageKnowledge.mockImplementation(async (request: { offset?: number }) => respond([item({ id: `p${request.offset ?? 0}`, label: `Item at ${request.offset ?? 0}` })], (request.offset ?? 0) === 0));
    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: 'Next' }));
    expect(await screen.findByText('Item at 50')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Previous' }));
    expect(await screen.findByText('Item at 0')).toBeInTheDocument();
  });

  it('shows a failed read', async () => {
    manageKnowledge.mockResolvedValue({ ok: false, error: 'memory store locked' });
    renderPanel();
    expect(await screen.findByRole('alert')).toHaveTextContent('memory store locked');
  });
});

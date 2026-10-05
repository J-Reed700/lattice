import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ProgramBuilder } from '@/features/learning/curriculum/ProgramBuilder';
import { useConversationUiStore } from '@/stores/conversationUiStore';


const api = vi.hoisted(() => ({ spaces: vi.fn(), documents: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { listConversationSpaces: api.spaces, listSpaceDocuments: api.documents } }));

function show() {
  const generate = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><ProgramBuilder pending={false} onGenerate={generate} onCancel={vi.fn()} /></QueryClientProvider>);
  return generate;
}

beforeEach(() => {
  vi.resetAllMocks();
  useConversationUiStore.setState({ selectedSpaceId: 'statistics' });
  api.spaces.mockResolvedValue({ ok: true, data: [{ id: 'space_general', name: 'General' }, { id: 'statistics', name: 'Statistics' }, { id: 'design', name: 'Design' }] });
  api.documents.mockImplementation(async (space: string) => ({ ok: true, data: [{ documentId: `${space  }-doc`, fileName: `${space  }.pdf`, category: null, modifiedAt: null }] }));
});

describe('course creation', () => {
  it('creates a complete course from a topic without loading or requiring documents', async () => {
    const user = userEvent.setup();
    const generate = show();
    await user.type(screen.getByLabelText('Your goal'), 'Design and interpret experiments');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith({ goal: 'Design and interpret experiments', priorKnowledge: '', minutesPerSession: 30, documentIds: [], sourceUrls: [], courseDepth: 'course' });
    expect(api.documents).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('uses the explicitly displayed Space, retains named selections across Spaces, and sends only checked IDs', async () => {
    const user = userEvent.setup();
    const generate = show();
    await user.type(screen.getByLabelText('Your goal'), 'Compare research methods');
    await user.click(screen.getByRole('button', { name: 'Add optional materials' }));
    expect(await screen.findByLabelText('Document Space')).toHaveValue('statistics');
    await user.click(await screen.findByRole('checkbox', { name: 'statistics.pdf' }));
    let resolve!: (value: unknown) => void;
    api.documents.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    await user.selectOptions(screen.getByLabelText('Document Space'), 'design');
    expect(screen.queryByRole('checkbox', { name: 'statistics.pdf' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove statistics.pdf' })).toHaveTextContent('Statistics');
    await act(async () => resolve({ ok: true, data: [{ documentId: 'design-doc', fileName: 'design.pdf', category: null, modifiedAt: null }] }));
    await user.click(await screen.findByRole('checkbox', { name: 'design.pdf' }));
    await user.click(screen.getByRole('radio', { name: /Deep dive/ }));
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith(expect.objectContaining({ documentIds: ['statistics-doc', 'design-doc'], courseDepth: 'deep_dive' }));
    expect(api.documents).toHaveBeenLastCalledWith('design', null, '', 50);
    await user.click(screen.getByRole('button', { name: 'Remove statistics.pdf' }));
    expect(screen.queryByRole('button', { name: 'Remove statistics.pdf' })).not.toBeInTheDocument();
  });

  it('searches within the selected Space and keeps a failed library optional', async () => {
    const user = userEvent.setup();
    const generate = show();
    api.documents.mockResolvedValue({ ok: false, error: 'Library unavailable' });
    await user.type(screen.getByLabelText('Your goal'), 'Learn probability');
    await user.click(screen.getByRole('button', { name: 'Add optional materials' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Library unavailable');
    await user.type(screen.getByLabelText('Find a document'), 'bayes');
    await waitFor(() => expect(api.documents).toHaveBeenLastCalledWith('statistics', null, 'bayes', 50));
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith(expect.objectContaining({ documentIds: [] }));
  });

  it('validates supplied URLs while allowing commas inside an intact URL and deduplicating it', async () => {
    const user = userEvent.setup();
    const generate = show();
    await user.type(screen.getByLabelText('Your goal'), 'Learn probability');
    await user.click(screen.getByRole('button', { name: 'Add optional materials' }));
    const urls = screen.getByRole('textbox', { name: /Reference URLs/ });
    await user.type(urls, 'file:///private');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent('http or https');
    await user.clear(urls);
    await user.type(urls, 'https://example.com/a,b\nhttps://example.com/a,b');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith(expect.objectContaining({ sourceUrls: ['https://example.com/a,b'] }));
  });
});

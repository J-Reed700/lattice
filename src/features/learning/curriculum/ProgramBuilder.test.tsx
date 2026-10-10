import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useConversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { ProgramBuilder } from '@/features/learning/curriculum/ProgramBuilder';


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

  it('does not cap the goal or prior-knowledge fields', async () => {
    const user = userEvent.setup();
    const generate = show();
    const goal = 'Detailed learning goal. '.repeat(150);
    const priorKnowledge = 'Relevant experience and context. '.repeat(150);
    const goalField = screen.getByLabelText('Your goal');
    const priorField = screen.getByLabelText('What do you already know?');
    expect(goalField).not.toHaveAttribute('maxlength');
    expect(priorField).not.toHaveAttribute('maxlength');
    fireEvent.change(goalField, { target: { value: goal } });
    fireEvent.change(priorField, { target: { value: priorKnowledge } });
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith(expect.objectContaining({ goal: goal.trim(), priorKnowledge: priorKnowledge.trim() }));
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

  it('submits all selected references beyond the former source-count limits', async () => {
    const user = userEvent.setup();
    const generate = show();
    api.documents.mockResolvedValue({ ok: true, data: Array.from({ length: 15 }, (_, i) => ({ documentId: `doc-${i}`, fileName: `Notes ${i}.txt`, category: null, modifiedAt: null })) });
    await user.type(screen.getByLabelText('Your goal'), 'Study these reference materials');
    await user.click(screen.getByRole('button', { name: 'Add optional materials' }));
    for (const checkbox of await screen.findAllByRole('checkbox')) await user.click(checkbox);
    const sourceUrls = Array.from({ length: 101 }, (_, i) => `https://example.org/reference/${i}`);
    fireEvent.change(screen.getByRole('textbox', { name: /Reference URLs/ }), { target: { value: sourceUrls.join('\n') } });
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    expect(generate).toHaveBeenCalledWith(expect.objectContaining({ documentIds: Array.from({ length: 15 }, (_, i) => `doc-${i}`), sourceUrls }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
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

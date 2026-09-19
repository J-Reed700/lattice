import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import { AnswerActionsMenu } from '../actions/AnswerActionsMenu';

const navigate = vi.hoisted(() => vi.fn());
const quickCapture = vi.hoisted(() =>
  vi.fn().mockResolvedValue({ ok: true, data: { noteId: 'note-1', noteTitle: 'Week of Sep 14' } }),
);

vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return { ...actual, useNavigate: () => navigate };
});

vi.mock('@/lib/api', () => ({
  VaultAPI: { quickCapture },
  default: { quickCapture },
}));

const source = (documentId: string, fileName: string, extra: Partial<SourceWithMetadata> = {}) =>
  ({
    documentId,
    chunkId: `${documentId}#c1`,
    fileName,
    filePath: `/vault/${fileName}`,
    mimeType: 'application/pdf',
    category: 'Research Paper',
    content: 'A cited passage.',
    excerpt: 'A cited passage.',
    score: 0.9,
    fileSizeBytes: 100,
    modifiedAt: '2026-08-01T00:00:00.000Z',
    ...extra,
  }) as SourceWithMetadata;

const halvorsen = source('doc-halvorsen', 'Halvorsen 2024.pdf', { pageNumber: 14, citationId: 1 });
const transect = source('doc-transect', 'Transect.md', { citationId: 2 });
const webPage = source('web:https://example.org/piece', 'A web piece', {
  category: 'Web Article',
  citationId: 3,
});

async function openMenu(sources: SourceWithMetadata[]) {
  render(
    <MemoryRouter>
      <AnswerActionsMenu sources={sources} markdown="The pooled estimate was 1.2 °C [1]." conversationTitle="Canopy">
        <button type="button">Use this answer</button>
      </AnswerActionsMenu>
    </MemoryRouter>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Use this answer' }));
}

describe('AnswerActionsMenu', () => {
  beforeEach(() => {
    navigate.mockClear();
    quickCapture.mockClear();
  });

  it('offers both ways out of an answer', async () => {
    await openMenu([halvorsen, transect]);

    expect(screen.getByText('Add to journal')).toBeInTheDocument();
    expect(screen.getByText('Compare sources')).toBeInTheDocument();
  });

  it('opens Compare on the distinct documents the answer cites', async () => {
    await openMenu([halvorsen, transect, source('doc-halvorsen', 'Halvorsen 2024.pdf')]);

    expect(screen.getByText('2 documents, side by side')).toBeInTheDocument();
    await userEvent.click(screen.getByText('Compare sources'));
    expect(navigate).toHaveBeenCalledWith('/compare?ids=doc-halvorsen%2Cdoc-transect');
  });

  it('does not count a web page towards the two documents Compare needs', async () => {
    await openMenu([halvorsen, webPage]);

    const compare = screen.getByText('Compare sources').closest('button');
    expect(compare).toBeDisabled();
    expect(screen.getByText('Needs two of your documents; this answer cites one')).toBeInTheDocument();
  });

  it('writes the answer and its sources to the journal, and says which page took it', async () => {
    await openMenu([halvorsen, transect]);
    await userEvent.click(screen.getByText('Add to journal'));

    await waitFor(() => expect(quickCapture).toHaveBeenCalledTimes(1));
    const written = quickCapture.mock.calls[0]![0] as string;
    expect(written).toContain('## From chat · Canopy');
    expect(written).toContain('The pooled estimate was 1.2 °C [1].');
    expect(written).toContain('1. Halvorsen 2024.pdf — PDF p. 14');
  });

  it('reports a journal that refused the write instead of claiming a save', async () => {
    quickCapture.mockResolvedValueOnce({ ok: false, error: 'disk full' });
    await openMenu([halvorsen, transect]);
    await userEvent.click(screen.getByText('Add to journal'));

    await waitFor(() => expect(quickCapture).toHaveBeenCalledTimes(1));
    expect(navigate).not.toHaveBeenCalled();
  });
});

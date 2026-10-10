import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, expect, it, vi } from 'vitest';

import { InsightMessage } from '@/features/journal/components/InsightMessage';
import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { SnapshotMessage } from '@/types/api/dailyNotes';

beforeEach(() => citationDisplayStore.getState().setVisible(true));

it('opens and hides sparse original citations without treating other bracketed numbers as sources', async () => {
  const onOpenSource = vi.fn();
  const message: SnapshotMessage = {
    id: 'message', role: 'assistant', createdAt: '2026-10-08T12:00:00Z',
    content: 'Read the saved passage [7]. Step [1] is ordinary text.',
    metadata: JSON.stringify({ sources: [{ citationId: 7, documentId: 'document', filePath: '/docs/source.md', fileName: 'Saved source' }] }),
  };
  const { container } = render(<MemoryRouter><InsightMessage message={message} onOpenSource={onOpenSource} /></MemoryRouter>);

  const chip = await screen.findByRole('button', { name: 'Citation 7' });
  fireEvent.click(chip);
  expect(onOpenSource).toHaveBeenCalledWith(expect.objectContaining({ citationId: 7, documentId: 'document' }));
  expect(screen.getByRole('button', { name: /^7\s*Saved source$/ })).toBeVisible();

  act(() => citationDisplayStore.getState().setVisible(false));
  await waitFor(() => expect(container.querySelector('.citation-hidden')).toHaveTextContent('[7]'));
  expect(screen.queryByRole('button', { name: 'Citation 7' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^7\s*Saved source$/ })).not.toBeInTheDocument();
  expect(container.querySelector('.citation-hidden')).not.toHaveTextContent('[1]');
  expect(container).toHaveTextContent('Step [1] is ordinary text.');
});

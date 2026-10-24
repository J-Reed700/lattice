import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { NeighborhoodPanel } from './NeighborhoodPanel';

const h = vi.hoisted(() => ({ query: {} as Record<string, unknown>, args: [] as unknown[] }));
vi.mock('@/features/files/hooks/useNeighborhoodQuery', () => ({
  useNeighborhoodQuery: (...args: unknown[]) => { h.args = args; return h.query; },
}));

const renderPanel = () => {
  const props = { onOpenDocument: vi.fn(), onOpenConversation: vi.fn(), onClose: vi.fn() };
  const documentsById = new Map();
  render(<TooltipProvider><NeighborhoodPanel documentId="d1" documentTitle="Herbal.pdf" documentsById={documentsById} {...props} /></TooltipProvider>);
  return { ...props, documentsById };
};

describe('NeighborhoodPanel', () => {
  beforeEach(() => {
    h.query = { isLoading: false, data: undefined };
  });

  it('asks for the document’s neighbourhood and shows loading', () => {
    h.query = { isLoading: true };
    const { documentsById } = renderPanel();
    expect(h.args).toEqual(['d1', 'Herbal.pdf', documentsById]);
    expect(screen.getAllByText('Loading…')).toHaveLength(3);
  });

  it('says when each list is empty', () => {
    renderPanel();
    expect(screen.getByText('Nothing links here yet.')).toBeInTheDocument();
    expect(screen.getByText('Nothing similar yet.')).toBeInTheDocument();
    expect(screen.getByText('Not cited in a conversation yet.')).toBeInTheDocument();
  });

  it('opens linked and similar documents and citing conversations', () => {
    h.query = {
      isLoading: false,
      data: {
        links: [{ documentId: 'd2', title: 'Remedies.md' }],
        similar: [{ documentId: 'd3', title: 'Physic garden.pdf' }],
        citedIn: [{ conversationId: 'c1', title: 'Medieval medicine' }],
      },
    };
    const props = renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'Remedies.md' }));
    fireEvent.click(screen.getByRole('button', { name: 'Physic garden.pdf' }));
    fireEvent.click(screen.getByRole('button', { name: 'Medieval medicine' }));
    expect(props.onOpenDocument.mock.calls).toEqual([['d2'], ['d3']]);
    expect(props.onOpenConversation).toHaveBeenCalledWith('c1');
    fireEvent.click(screen.getByRole('button', { name: 'Hide related' }));
    expect(props.onClose).toHaveBeenCalled();
  });
});

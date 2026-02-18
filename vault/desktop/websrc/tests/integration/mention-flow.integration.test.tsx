import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { MentionAutocomplete } from '../../components/MentionAutocomplete/MentionAutocomplete';
import { mockIPC } from '../setup';

describe('Mention Autocomplete Flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows autocomplete on [[ trigger for wikilinks', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockImplementation((cmd: string) => {
      if (cmd === 'search_mentions') {
        return Promise.resolve({
          mentions: [
            {
              id: '1',
              name: 'machine-learning',
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
            {
              id: '2',
              name: 'machine-vision',
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    render(
      <MentionAutocomplete
        query="mach"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('search_mentions', {
        query: 'mach',
        limit: 10,
      });
    });

    await waitFor(() => {
      expect(screen.getByText('machine-learning')).toBeInTheDocument();
      expect(screen.getByText('machine-vision')).toBeInTheDocument();
    });
  });

  it('shows autocomplete on @ trigger for person mentions', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockImplementation((cmd: string) => {
      if (cmd === 'search_mentions') {
        return Promise.resolve({
          mentions: [
            {
              id: '1',
              name: 'alice-smith',
              type: 'person',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
            {
              id: '2',
              name: 'albert-jones',
              type: 'person',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    render(
      <MentionAutocomplete
        query="al"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('alice-smith')).toBeInTheDocument();
      expect(screen.getByText('albert-jones')).toBeInTheDocument();
    });
  });

  it('handles keyboard navigation', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockImplementation((cmd: string) => {
      if (cmd === 'search_mentions') {
        return Promise.resolve({
          mentions: [
            {
              id: '1',
              name: 'first-option',
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
            {
              id: '2',
              name: 'second-option',
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    render(
      <MentionAutocomplete
        query="opt"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('first-option')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });

    await waitFor(() => {
      expect(onSelect).toHaveBeenCalled();
    });
  });

  it('handles escape to close', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({ mentions: [] });

    render(
      <MentionAutocomplete
        query=""
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    fireEvent.keyDown(window, { key: 'Escape' });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('displays loading state', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    vi.useFakeTimers();
    mockIPC.mockImplementation(() => new Promise((resolve) => {
      setTimeout(() => {
        resolve({ mentions: [] });
      }, 1000);
    }));

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await act(async () => {
      vi.advanceTimersByTime(300);
    });

    expect(screen.getByText('Searching...')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('displays no results message', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({ mentions: [] });

    render(
      <MentionAutocomplete
        query="nonexistent"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/No matches for "nonexistent"/i)).toBeInTheDocument();
    });
  });

  it('allows selection by click', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'clickable-option',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <MentionAutocomplete
        query="click"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('clickable-option')).toBeInTheDocument();
    });

    const option = screen.getByText('clickable-option');
    await user.click(option);

    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'clickable-option',
        type: 'wikilink',
      })
    );
  });

  it('filters mentions by type correctly', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockImplementation((cmd: string) => {
      if (cmd === 'search_mentions') {
        return Promise.resolve({
          mentions: [
            {
              id: '1',
              name: 'alice',
              type: 'person',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
            {
              id: '2',
              name: 'alice-note',
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    const { rerender } = render(
      <MentionAutocomplete
        query="alice"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeInTheDocument();
    });

    expect(screen.queryByText('alice-note')).not.toBeInTheDocument();

    rerender(
      <MentionAutocomplete
        query="alice"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('alice-note')).toBeInTheDocument();
    });

    expect(screen.queryByText(/person/)).not.toBeInTheDocument();
  });

  it('handles error during search', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockRejectedValue(new Error('Search failed'));

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/No matches/i)).toBeInTheDocument();
    });
  });

  it('handles empty query', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({ mentions: [] });

    render(
      <MentionAutocomplete
        query=""
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Type to search people or concepts/i)).toBeInTheDocument();
    });
  });

  it('navigates through results with arrow keys', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'first',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
        {
          id: '2',
          name: 'second',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
        {
          id: '3',
          name: 'third',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('first')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });

    await waitFor(() => {
      expect(onSelect).toHaveBeenCalledWith(
        expect.objectContaining({ name: 'third' })
      );
    });
  });

  it('wraps around when navigating past last result', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'only-option',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('only-option')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });

    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'only-option' })
    );
  });

  it('closes on click outside', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'test-mention',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <>
        <MentionAutocomplete
          query="test"
          position={{ top: 100, left: 50 }}
          onSelect={onSelect}
          onClose={onClose}
          type="wikilink"
        />
        <div data-testid="outside">Outside element</div>
      </>
    );

    await waitFor(() => {
      expect(screen.getByText('test-mention')).toBeInTheDocument();
    });

    const outside = screen.getByTestId('outside');
    fireEvent.mouseDown(outside);

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('displays mention type indicators', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'alice-smith',
          type: 'person',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
        {
          id: '2',
          name: 'machine-learning',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="mention"
      />
    );

    await waitFor(() => {
      expect(screen.getByText('person')).toBeInTheDocument();
    });
  });

  it('handles query updates', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    let callCount = 0;
    mockIPC.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'search_mentions') {
        callCount++;
        return Promise.resolve({
          mentions: [
            {
              id: `${callCount}`,
              name: `result-${args.query}`,
              type: 'wikilink',
              metadata: null,
              createdAt: new Date().toISOString(),
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    const { rerender } = render(
      <MentionAutocomplete
        query="a"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('search_mentions', {
        query: 'a',
        limit: 10,
      });
    });

    rerender(
      <MentionAutocomplete
        query="ab"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(mockIPC).toHaveBeenCalledWith('search_mentions', {
        query: 'ab',
        limit: 10,
      });
    });
  });

  it('supports keyboard help text', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();

    mockIPC.mockResolvedValue({
      mentions: [
        {
          id: '1',
          name: 'test-option',
          type: 'wikilink',
          metadata: null,
          createdAt: new Date().toISOString(),
        },
      ],
    });

    render(
      <MentionAutocomplete
        query="test"
        position={{ top: 100, left: 50 }}
        onSelect={onSelect}
        onClose={onClose}
        type="wikilink"
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Navigate/i)).toBeInTheDocument();
      expect(screen.getByText(/Select/i)).toBeInTheDocument();
    });
  });
});

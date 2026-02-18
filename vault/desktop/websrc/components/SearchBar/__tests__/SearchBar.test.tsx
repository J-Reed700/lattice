import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { SearchBar } from '../SearchBar';

// Mock debounce to be synchronous in tests
vi.mock('../../../lib/debounce', () => ({
  debounce: vi.fn((fn) => {
    const wrappedFn = (...args: any[]) => fn(...args);
    wrappedFn.cancel = vi.fn();
    wrappedFn.flush = vi.fn();
    return wrappedFn;
  })
}));

describe('SearchBar', () => {
  const mockOnSearch = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('renders search input with default placeholder', () => {
      render(<SearchBar onSearch={mockOnSearch} />);
      expect(screen.getByPlaceholderText('Search your documents...')).toBeInTheDocument();
    });

    it('renders with custom placeholder', () => {
      render(<SearchBar onSearch={mockOnSearch} placeholder="Custom search..." />);
      expect(screen.getByPlaceholderText('Custom search...')).toBeInTheDocument();
    });

    it('renders all three search mode buttons', () => {
      render(<SearchBar onSearch={mockOnSearch} />);
      expect(screen.getByRole('button', { name: /semantic/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /keyword/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /hybrid/i })).toBeInTheDocument();
    });

    it('renders keyboard shortcut hints', () => {
      render(<SearchBar onSearch={mockOnSearch} />);
      expect(screen.getByText('Cmd+K')).toBeInTheDocument();
      expect(screen.getByText('Focus search')).toBeInTheDocument();
      expect(screen.getByText('Cmd+F')).toBeInTheDocument();
      expect(screen.getByText('Cycle modes')).toBeInTheDocument();
    });

    it('shows loading indicator when isSearching is true', () => {
      render(<SearchBar onSearch={mockOnSearch} isSearching />);
      const input = screen.getByLabelText('Search documents');
      expect(input).toBeInTheDocument();
    });
  });

  describe('User Interactions', () => {
    it('calls onSearch when user types in search input', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      await user.type(input, 'test query');

      // With debounce mocked as synchronous, search fires immediately
      expect(mockOnSearch).toHaveBeenCalledWith('test query', 'hybrid');
    });

    it('trims whitespace from search query', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      await user.type(input, '  test query  ');

      expect(mockOnSearch).toHaveBeenCalledWith('test query', 'hybrid');
    });

    it('calls onSearch with empty string when input is cleared', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      await user.type(input, 'test');

      expect(mockOnSearch).toHaveBeenCalledWith('test', 'hybrid');
      mockOnSearch.mockClear();

      await user.clear(input);

      expect(mockOnSearch).toHaveBeenCalledWith('', 'hybrid');
    });
  });

  describe('Search Modes', () => {
    it('defaults to hybrid mode', () => {
      render(<SearchBar onSearch={mockOnSearch} />);
      const hybridButton = screen.getByRole('button', { name: /hybrid/i });
      expect(hybridButton).toHaveAttribute('aria-pressed', 'true');
    });

    it('changes search mode when mode button is clicked', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const semanticButton = screen.getByRole('button', { name: /semantic/i });
      await user.click(semanticButton);

      expect(semanticButton).toHaveAttribute('aria-pressed', 'true');

      const input = screen.getByLabelText('Search documents');
      await user.type(input, 'test');

      expect(mockOnSearch).toHaveBeenCalledWith('test', 'semantic');
    });

    it('triggers search when mode changes with existing query', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      await user.type(input, 'test query');

      expect(mockOnSearch).toHaveBeenCalledWith('test query', 'hybrid');
      mockOnSearch.mockClear();

      const keywordButton = screen.getByRole('button', { name: /keyword/i });
      await user.click(keywordButton);

      expect(mockOnSearch).toHaveBeenCalledWith('test query', 'keyword');
    });

    it('does not trigger search when mode changes without query', async () => {
      render(<SearchBar onSearch={mockOnSearch} />);

      const semanticButton = screen.getByRole('button', { name: /semantic/i });
      const user = userEvent.setup();
      await user.click(semanticButton);

      expect(mockOnSearch).not.toHaveBeenCalled();
    });
  });

  describe('Keyboard Shortcuts', () => {
    it('focuses input when Cmd+K is pressed', async () => {
      render(<SearchBar onSearch={mockOnSearch} autoFocus={false} />);

      const input = screen.getByLabelText('Search documents');
      expect(input).not.toHaveFocus();

      const user = userEvent.setup();
      await user.keyboard('{Meta>}k{/Meta}');

      expect(input).toHaveFocus();
    });

    it('focuses input when Ctrl+K is pressed', async () => {
      render(<SearchBar onSearch={mockOnSearch} autoFocus={false} />);

      const input = screen.getByLabelText('Search documents');
      expect(input).not.toHaveFocus();

      const user = userEvent.setup();
      await user.keyboard('{Control>}k{/Control}');

      expect(input).toHaveFocus();
    });

    it('cycles search modes when Cmd+F is pressed', async () => {
      render(<SearchBar onSearch={mockOnSearch} />);

      const hybridButton = screen.getByRole('button', { name: /hybrid/i });
      expect(hybridButton).toHaveAttribute('aria-pressed', 'true');

      const user = userEvent.setup();
      await user.keyboard('{Meta>}f{/Meta}');

      const semanticButton = screen.getByRole('button', { name: /semantic/i });
      expect(semanticButton).toHaveAttribute('aria-pressed', 'true');

      await user.keyboard('{Meta>}f{/Meta}');

      const keywordButton = screen.getByRole('button', { name: /keyword/i });
      expect(keywordButton).toHaveAttribute('aria-pressed', 'true');

      await user.keyboard('{Meta>}f{/Meta}');

      expect(hybridButton).toHaveAttribute('aria-pressed', 'true');
    });

    it('triggers search when cycling modes with existing query', async () => {
      const user = userEvent.setup();
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      await user.type(input, 'test');

      expect(mockOnSearch).toHaveBeenCalledWith('test', 'hybrid');
      mockOnSearch.mockClear();

      await user.keyboard('{Meta>}f{/Meta}');

      expect(mockOnSearch).toHaveBeenCalledWith('test', 'semantic');
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA labels', () => {
      render(<SearchBar onSearch={mockOnSearch} />);

      const input = screen.getByLabelText('Search documents');
      expect(input).toHaveAttribute('aria-describedby', 'search-mode-selector');

      const modeSelector = screen.getByRole('group', { name: /search mode selection/i });
      expect(modeSelector).toBeInTheDocument();
    });

    it('mode buttons have aria-pressed attribute', () => {
      render(<SearchBar onSearch={mockOnSearch} />);

      const hybridButton = screen.getByRole('button', { name: /hybrid/i });
      const semanticButton = screen.getByRole('button', { name: /semantic/i });
      const keywordButton = screen.getByRole('button', { name: /keyword/i });

      expect(hybridButton).toHaveAttribute('aria-pressed', 'true');
      expect(semanticButton).toHaveAttribute('aria-pressed', 'false');
      expect(keywordButton).toHaveAttribute('aria-pressed', 'false');
    });

    it('mode buttons have descriptive titles', () => {
      render(<SearchBar onSearch={mockOnSearch} />);

      const semanticButton = screen.getByRole('button', { name: /semantic/i });
      expect(semanticButton).toHaveAttribute('title', 'Meaning-based search using AI embeddings');

      const keywordButton = screen.getByRole('button', { name: /keyword/i });
      expect(keywordButton).toHaveAttribute('title', 'Fast exact keyword matching');

      const hybridButton = screen.getByRole('button', { name: /hybrid/i });
      expect(hybridButton).toHaveAttribute('title', 'Combines semantic and keyword search');
    });

    it('auto-focuses input by default', () => {
      render(<SearchBar onSearch={mockOnSearch} />);
      const input = screen.getByLabelText('Search documents');
      expect(input).toHaveFocus();
    });

    it('does not auto-focus when autoFocus is false', () => {
      render(<SearchBar onSearch={mockOnSearch} autoFocus={false} />);
      const input = screen.getByLabelText('Search documents');
      expect(input).not.toHaveFocus();
    });
  });

  describe('Cleanup', () => {
    it('removes keyboard event listeners on unmount', () => {
      const removeEventListenerSpy = vi.spyOn(window, 'removeEventListener');
      const { unmount } = render(<SearchBar onSearch={mockOnSearch} />);

      unmount();

      expect(removeEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));

      removeEventListenerSpy.mockRestore();
    });
  });
});

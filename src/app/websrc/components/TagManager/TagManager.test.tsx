import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { VaultAPI } from '@/lib/api';

import { TagManager } from './TagManager';


import type { Tag } from '../../types/api/tags';

interface TagBadgeProps {
  tag: Tag;
  onRemove: (id: string) => void;
  removable: boolean;
}

vi.mock('../TagBadge', () => ({
  TagBadge: ({ tag, onRemove, removable }: TagBadgeProps) => (
    <div data-testid={`tag-${tag.id}`}>
      {tag.name}
      {removable && (
        <button onClick={() => onRemove(tag.id)} aria-label={`Remove ${tag.name}`}>
          ×
        </button>
      )}
    </div>
  ),
}));

const mockTags: Tag[] = [
  { id: '1', name: 'javascript', color: '#f7df1e', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 },
  { id: '2', name: 'react', color: '#61dafb', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 },
];

const mockAllTags: Tag[] = [
  ...mockTags,
  { id: '3', name: 'typescript', color: '#3178c6', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 },
  { id: '4', name: 'nodejs', color: '#339933', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 },
];

describe('TagManager', () => {
  const mockGetDocumentTags = vi.mocked(VaultAPI.getDocumentTags);
  const mockListAllTags = vi.mocked(VaultAPI.listAllTags);
  const mockGenerateTagsForDocument = vi.mocked(VaultAPI.generateTagsForDocument);
  const mockApplyTags = vi.mocked(VaultAPI.applyTags);
  const mockRemoveTagFromDocument = vi.mocked(VaultAPI.removeTagFromDocument);

  beforeEach(() => {
    vi.clearAllMocks();
    mockGetDocumentTags.mockResolvedValue({ ok: true, data: { tags: mockTags, documentId: 'doc123' } });
    mockListAllTags.mockResolvedValue({ ok: true, data: { tags: mockAllTags } });
    mockGenerateTagsForDocument.mockResolvedValue({ ok: true, data: [] });
    mockApplyTags.mockResolvedValue({ ok: true, data: mockTags });
    mockRemoveTagFromDocument.mockResolvedValue({ ok: true, data: undefined });
  });

  describe('Error Handling - Tag Generation', () => {
    it('should show error when tag generation fails with Python bridge error', async () => {
      mockGenerateTagsForDocument.mockResolvedValueOnce({
        ok: false,
        error: 'Python bridge unavailable',
      });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.getByText(/Python bridge unavailable/i)).toBeInTheDocument();
      });
    });

    it('should show error when tag generation fails with Ollama error', async () => {
      mockGenerateTagsForDocument.mockResolvedValueOnce({
        ok: false,
        error: 'Ollama connection failed',
      });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.getByText(/Ollama connection failed/i)).toBeInTheDocument();
      });
    });

    it('should show generic error message for unknown errors', async () => {
      mockGenerateTagsForDocument.mockResolvedValueOnce({
        ok: false,
        error: 'Some unexpected error',
      });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.getByText(/Some unexpected error/i)).toBeInTheDocument();
      });
    });

    it('should disable generate button while generating', async () => {
      mockGenerateTagsForDocument.mockImplementationOnce(
        () => new Promise((resolve) => setTimeout(() => resolve({ ok: true, data: ['newtag'] }), 100))
      );
      mockApplyTags.mockResolvedValueOnce({
        ok: true,
        data: [...mockTags, { id: '5', name: 'newtag', color: '#000', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 }],
      });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      expect(generateButton).toBeDisabled();
      expect(screen.getByText(/generating/i)).toBeInTheDocument();

      await waitFor(() => {
        expect(generateButton).not.toBeDisabled();
      });
    });

    it('should clear error message on successful operation', async () => {
      const newTag: Tag = { id: '5', name: 'newtag', color: '#000', createdAt: '2024-01-01', updatedAt: '2024-01-01', documentCount: 0 };
      mockGenerateTagsForDocument
        .mockResolvedValueOnce({ ok: false, error: 'Ollama connection failed' })
        .mockResolvedValueOnce({ ok: true, data: ['newtag'] });
      mockApplyTags.mockResolvedValueOnce({ ok: true, data: [...mockTags, newTag] });
      mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: mockAllTags } });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.getByText(/Ollama connection failed/i)).toBeInTheDocument();
      });

      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.queryByText(/Ollama connection failed/i)).not.toBeInTheDocument();
      });
    });
  });

  describe('Error Handling - Tag Loading', () => {
    it('should show error when loading document tags fails', async () => {
      mockGetDocumentTags.mockResolvedValueOnce({ ok: false, error: 'Database error' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.getByText(/Database error/i)).toBeInTheDocument();
      });
    });

    it('should handle database errors with user-friendly message', async () => {
      mockGetDocumentTags.mockResolvedValueOnce({ ok: false, error: 'SQLite database locked' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.getByText(/SQLite database locked/i)).toBeInTheDocument();
      });
    });
  });

  describe('Error Handling - Manual Tag Operations', () => {
    it('should show error when adding tag fails', async () => {
      const user = userEvent.setup();
      mockApplyTags.mockResolvedValueOnce({ ok: false, error: 'Failed to add tag' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/add tag/i);
      await user.type(input, 'newtag{enter}');

      await waitFor(() => {
        expect(screen.getByText(/Failed to add tag/i)).toBeInTheDocument();
      });
    });

    it('should show error when removing tag fails', async () => {
      mockRemoveTagFromDocument.mockResolvedValueOnce({ ok: false, error: 'Permission denied' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.getByTestId('tag-1')).toBeInTheDocument();
      });

      const removeButton = screen.getByLabelText(/remove javascript/i);
      fireEvent.click(removeButton);

      await waitFor(() => {
        expect(screen.getByText(/Permission denied/i)).toBeInTheDocument();
      });
    });

    it('should handle timeout errors gracefully', async () => {
      mockGenerateTagsForDocument.mockResolvedValueOnce({
        ok: false,
        error: 'Operation timeout',
      });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(screen.getByText(/Operation timeout/i)).toBeInTheDocument();
      });
    });
  });

  describe('Error Handling - Race Conditions', () => {
    it('should prevent multiple simultaneous operations', async () => {
      mockGenerateTagsForDocument.mockImplementationOnce(
        () => new Promise((resolve) => setTimeout(() => resolve({ ok: true, data: ['tag1'] }), 200))
      );

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);

      fireEvent.click(generateButton);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(mockGenerateTagsForDocument).toHaveBeenCalledTimes(1);
        expect(generateButton).toBeDisabled();
      });
    });

    it('should block manual operations during auto-generation', async () => {
      const user = userEvent.setup();
      mockGenerateTagsForDocument.mockImplementationOnce(
        () => new Promise((resolve) => setTimeout(() => resolve({ ok: true, data: ['tag1'] }), 200))
      );

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      const input = screen.getByPlaceholderText(/add tag/i);
      await user.type(input, 'newtag');

      const addButton = screen.getByText(/^add$/i);
      fireEvent.click(addButton);

      await waitFor(() => {
        expect(screen.getByText(/wait.*operation.*complete/i)).toBeInTheDocument();
      });
    });
  });

  describe('Error Display', () => {
    it('should display errors in a visible error banner', async () => {
      mockGenerateTagsForDocument.mockResolvedValueOnce({ ok: false, error: 'Test error' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        const errorBanner = screen.getByText(/test error/i).closest('div');
        expect(errorBanner).toHaveClass('bg-[var(--error-light)]/20');
        expect(errorBanner).toHaveClass('text-[var(--error)]');
      });
    });

    it('should log errors to console', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      mockGenerateTagsForDocument.mockResolvedValueOnce({ ok: false, error: 'Test error' });

      render(<TagManager documentId="doc123" />);

      await waitFor(() => {
        expect(screen.queryByText(/loading/i)).not.toBeInTheDocument();
      });

      const generateButton = screen.getByText(/auto-generate/i);
      fireEvent.click(generateButton);

      await waitFor(() => {
        expect(consoleError).toHaveBeenCalledWith(
          'Failed to generate tags:',
          'Test error'
        );
      });

      consoleError.mockRestore();
    });
  });
});

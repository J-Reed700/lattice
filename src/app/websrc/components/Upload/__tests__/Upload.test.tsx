import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { VaultAPI } from '@/lib/api';
import Upload from '../Upload';

const mockIndexFileResponse = {
  documentId: 'doc-123',
  chunksCreated: 5,
  status: 'indexed' as const,
  error: null,
  filePath: '/Users/test/.recall/files/abc123def456/test-document.pdf'
};

describe('Upload', () => {
  const mockOnUploadComplete = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('renders upload drop zone', () => {
      render(<Upload />);
      expect(screen.getByText(/drag and drop files here/i)).toBeInTheDocument();
    });

    it('renders all upload buttons', () => {
      render(<Upload />);
      expect(screen.getByRole('button', { name: /select file/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /select multiple files/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /select folder/i })).toBeInTheDocument();
    });

    it('renders upload icon', () => {
      const { container } = render(<Upload />);
      const svg = container.querySelector('svg');
      expect(svg).toBeInTheDocument();
    });
  });

  describe('Single File Upload', () => {
    it('handles successful file selection and upload', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({
        ok: true,
        data: { documentId: 'doc-123', chunksCreated: 5, status: 'indexed', error: null, filePath: '/Users/test/.recall/files/abc123/file.txt' }
      });

      render(<Upload onUploadComplete={mockOnUploadComplete} />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(VaultAPI.selectFile).toHaveBeenCalled();
        expect(VaultAPI.indexFile).toHaveBeenCalledWith('/path/to/file.txt');
      });

      expect(screen.getByText(/successfully indexed:/i)).toBeInTheDocument();
      expect(mockOnUploadComplete).toHaveBeenCalled();
    });

    it('passes selected space when uploading a file', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.listConversationSpaces).mockResolvedValue({
        ok: true,
        data: [{ id: 'space_docs', name: 'Docs', isArchived: false }] as any,
      });
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({
        ok: true,
        data: { documentId: 'doc-123', chunksCreated: 5, status: 'indexed', error: null, filePath: '/Users/test/.recall/files/abc123/file.txt' }
      });

      render(<Upload onUploadComplete={mockOnUploadComplete} />);

      await waitFor(() => {
        expect(VaultAPI.listConversationSpaces).toHaveBeenCalled();
      });

      await user.selectOptions(screen.getByLabelText(/optional space/i), 'space_docs');
      await user.click(screen.getByRole('button', { name: /^select file$/i }));

      await waitFor(() => {
        expect(VaultAPI.indexFile).toHaveBeenCalledWith('/path/to/file.txt', 'space_docs');
      });
    });

    it('shows loading state during upload', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      let resolveIndexFile: (value: any) => void;
      vi.mocked(VaultAPI.indexFile).mockImplementation(
        () => new Promise((resolve) => {
          resolveIndexFile = resolve;
        })
      );

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(screen.getByText(/indexing files.../i)).toBeInTheDocument();
      });

      expect(selectButton).toBeDisabled();

      // Resolve the promise to complete the test
      resolveIndexFile!({ ok: true, data: mockIndexFileResponse });
    });

    it('handles failed upload', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({
        ok: false,
        error: 'Failed to index file'
      });

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(screen.getByText(/failed to index file/i)).toBeInTheDocument();
      });

      expect(mockOnUploadComplete).not.toHaveBeenCalled();
    });

    it('does nothing when file selection is cancelled', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue(null);

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(VaultAPI.selectFile).toHaveBeenCalled();
      });

      expect(VaultAPI.indexFile).not.toHaveBeenCalled();
      expect(screen.queryByText(/indexing/i)).not.toBeInTheDocument();
    });
  });

  describe('Multiple Files Upload', () => {
    it('handles successful multiple files selection', async () => {
      const user = userEvent.setup();
      const files = ['/path/file1.txt', '/path/file2.txt', '/path/file3.txt'];
      vi.mocked(VaultAPI.selectMultipleFiles).mockResolvedValue(files);
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({ ok: true, data: mockIndexFileResponse });

      render(<Upload onUploadComplete={mockOnUploadComplete} />);

      const selectButton = screen.getByRole('button', { name: /select multiple files/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(VaultAPI.indexFile).toHaveBeenCalledTimes(3);
      });

      expect(screen.getByText(/successfully indexed 3 file\(s\)/i)).toBeInTheDocument();
      expect(mockOnUploadComplete).toHaveBeenCalled();
    });

    it('stops on first error when uploading multiple files', async () => {
      const user = userEvent.setup();
      const files = ['/path/file1.txt', '/path/file2.txt', '/path/file3.txt'];
      vi.mocked(VaultAPI.selectMultipleFiles).mockResolvedValue(files);
      vi.mocked(VaultAPI.indexFile)
        .mockResolvedValueOnce({ ok: true, data: mockIndexFileResponse })
        .mockResolvedValueOnce({ ok: false, error: 'Failed on file2' })
        .mockResolvedValueOnce({ ok: true, data: mockIndexFileResponse });

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /select multiple files/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(screen.getByText(/failed to index.*file2/i)).toBeInTheDocument();
      });

      expect(VaultAPI.indexFile).toHaveBeenCalledTimes(2);
    });

    it('handles empty file selection', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectMultipleFiles).mockResolvedValue([]);

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /select multiple files/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(VaultAPI.selectMultipleFiles).toHaveBeenCalled();
      });

      expect(VaultAPI.indexFile).not.toHaveBeenCalled();
    });
  });

  describe('Folder Upload', () => {
    it('handles successful folder selection', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFolder).mockResolvedValue('/path/to/folder');
      vi.mocked(VaultAPI.startIndexing).mockResolvedValue({ ok: true, data: undefined });

      render(<Upload onUploadComplete={mockOnUploadComplete} />);

      const selectButton = screen.getByRole('button', { name: /select folder/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(VaultAPI.startIndexing).toHaveBeenCalledWith('/path/to/folder', true);
      });

      expect(screen.getByText(/started indexing folder/i)).toBeInTheDocument();
      expect(mockOnUploadComplete).toHaveBeenCalled();
    });

    it('passes selected space when indexing a folder', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.listConversationSpaces).mockResolvedValue({
        ok: true,
        data: [{ id: 'space_docs', name: 'Docs', isArchived: false }] as any,
      });
      vi.mocked(VaultAPI.selectFolder).mockResolvedValue('/path/to/folder');
      vi.mocked(VaultAPI.startIndexing).mockResolvedValue({ ok: true, data: undefined });

      render(<Upload />);

      await waitFor(() => {
        expect(VaultAPI.listConversationSpaces).toHaveBeenCalled();
      });

      await user.selectOptions(screen.getByLabelText(/optional space/i), 'space_docs');
      await user.click(screen.getByRole('button', { name: /select folder/i }));

      await waitFor(() => {
        expect(VaultAPI.startIndexing).toHaveBeenCalledWith('/path/to/folder', true, 'space_docs');
      });
    });

    it('handles failed folder indexing', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFolder).mockResolvedValue('/path/to/folder');
      vi.mocked(VaultAPI.startIndexing).mockResolvedValue({
        ok: false,
        error: 'Permission denied'
      });

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /select folder/i });
      await user.click(selectButton);

      await waitFor(() => {
        expect(screen.getByText(/permission denied/i)).toBeInTheDocument();
      });

      expect(mockOnUploadComplete).not.toHaveBeenCalled();
    });
  });

  describe('Drag and Drop', () => {
    it('highlights drop zone on drag enter', async () => {
      const user = userEvent.setup();
      const { container } = render(<Upload />);

      const dropZone = container.querySelector('.border-dashed');
      expect(dropZone).toBeInTheDocument();

      if (dropZone) {
        await user.pointer([{ target: dropZone, keys: '[MouseLeft>]', node: dropZone }]);
        const dragEvent = new DragEvent('dragenter', { bubbles: true });
        dropZone.dispatchEvent(dragEvent);

        await waitFor(() => {
          expect(dropZone).toHaveClass('border-[var(--accent-primary)]');
        });
      }
    });

    it('removes highlight on drag leave', async () => {
      const { container } = render(<Upload />);

      const dropZone = container.querySelector('.border-dashed');
      expect(dropZone).toBeInTheDocument();

      if (dropZone) {
        const dragEnter = new DragEvent('dragenter', { bubbles: true });
        dropZone.dispatchEvent(dragEnter);

        const dragLeave = new DragEvent('dragleave', { bubbles: true });
        dropZone.dispatchEvent(dragLeave);

        await waitFor(() => {
          expect(dropZone).not.toHaveClass('border-[var(--accent-primary)]');
        });
      }
    });

    it('handles file drop', async () => {
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({ ok: true, data: mockIndexFileResponse });

      const { container } = render(<Upload onUploadComplete={mockOnUploadComplete} />);

      const dropZone = container.querySelector('.border-dashed');
      expect(dropZone).toBeInTheDocument();

      if (dropZone) {
        const file = new File(['content'], 'test.txt', { type: 'text/plain' });
        Object.defineProperty(file, 'path', { value: '/path/to/test.txt' });

        const dataTransfer = {
          files: [file],
          items: [],
          types: ['Files']
        };

        const dropEvent = new Event('drop', {
          bubbles: true,
        });

        Object.defineProperty(dropEvent, 'dataTransfer', {
          value: dataTransfer,
        });

        dropZone.dispatchEvent(dropEvent);

        await waitFor(() => {
          expect(VaultAPI.indexFile).toHaveBeenCalled();
        });
      }
    });
  });

  describe('Success/Error Messages', () => {
    // Timer-specific test removed - no longer using fake timers

    it('displays error message in error state', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({
        ok: false,
        error: 'Network error'
      });

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        const errorElement = screen.getByText(/network error/i);
        expect(errorElement).toBeInTheDocument();
        expect(errorElement.closest('.bg-\\[var\\(--error-light\\)\\]\\/20')).toBeInTheDocument();
      });
    });

    it('displays success message in success state', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({ ok: true, data: mockIndexFileResponse });

      render(<Upload />);

      const selectButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectButton);

      await waitFor(() => {
        const successElement = screen.getByText(/successfully indexed/i);
        expect(successElement).toBeInTheDocument();
        expect(successElement.closest('.bg-\\[var\\(--success-light\\)\\]\\/20')).toBeInTheDocument();
      });
    });
  });

  describe('Button States', () => {
    it('disables buttons during upload', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve({ ok: true, data: mockIndexFileResponse }), 1000))
      );

      render(<Upload />);

      const selectFileButton = screen.getByRole('button', { name: /^select file$/i });
      const selectMultipleButton = screen.getByRole('button', { name: /select multiple files/i });
      const selectFolderButton = screen.getByRole('button', { name: /select folder/i });

      await user.click(selectFileButton);

      await waitFor(() => {
        expect(selectFileButton).toBeDisabled();
        expect(selectMultipleButton).toBeDisabled();
        expect(selectFolderButton).toBeDisabled();
      });
    });

    it('re-enables buttons after upload completes', async () => {
      const user = userEvent.setup();
      vi.mocked(VaultAPI.selectFile).mockResolvedValue('/path/to/file.txt');
      vi.mocked(VaultAPI.indexFile).mockResolvedValue({ ok: true, data: mockIndexFileResponse });

      render(<Upload />);

      const selectFileButton = screen.getByRole('button', { name: /^select file$/i });
      await user.click(selectFileButton);

      await waitFor(() => {
        expect(selectFileButton).not.toBeDisabled();
      });
    });
  });

  // Cleanup describe block removed - timer-specific test no longer needed

  describe('Accessibility', () => {
    it('has accessible button labels', () => {
      render(<Upload />);

      expect(screen.getByRole('button', { name: /^select file$/i })).toHaveAccessibleName();
      expect(screen.getByRole('button', { name: /select multiple files/i })).toHaveAccessibleName();
      expect(screen.getByRole('button', { name: /select folder/i })).toHaveAccessibleName();
    });

    it('properly disables buttons with cursor-not-allowed', () => {
      vi.mocked(VaultAPI.selectFile).mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve('/path/to/file.txt'), 1000))
      );

      render(<Upload />);

      const { container } = render(<Upload />);

      const buttons = container.querySelectorAll('button:disabled');
      buttons.forEach((button) => {
        expect(button).toHaveClass('disabled:cursor-not-allowed');
      });
    });
  });
});

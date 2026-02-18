import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { useIndexProgress } from '../../../hooks/useIndexProgress';
import { IndexProgress } from '../IndexProgress';

// Mock the useIndexProgress hook
vi.mock('../../../hooks/useIndexProgress', () => ({
  useIndexProgress: vi.fn(),
}));

describe('IndexProgress', () => {
  const mockOnComplete = vi.fn();
  const mockOnError = vi.fn();
  const mockOnCancel = vi.fn();
  const mockHandleCancel = vi.fn();

  const mockProgressData = {
    status: 'processing' as const,
    processed: 50,
    totalFiles: 100,
    percentage: 50,
    currentFile: '/path/to/current/file.md',
    failed: 2,
    estimatedRemainingMs: 30000,
  };

  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock implementation - no progress
    (useIndexProgress as any).mockReturnValue({
      progress: null,
      error: null,
      isActive: false,
      handleCancel: mockHandleCancel,
    });
  });

  describe('Rendering', () => {
    it('does not render when no progress data', () => {
      const { container } = render(<IndexProgress />);
      expect(container.firstChild).toBeNull();
    });

    it('renders compact mode correctly', async () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress compact />);

      expect(screen.getByLabelText(/cancel indexing/i)).toBeInTheDocument();
      expect(screen.getByText('50/100')).toBeInTheDocument();
    });

    it('renders full mode with all details', async () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Processing')).toBeInTheDocument();
      expect(screen.getByText('50 / 100 files')).toBeInTheDocument();
      expect(screen.getByText('50.0%')).toBeInTheDocument();
      expect(screen.getByText('file.md')).toBeInTheDocument();
      expect(screen.getByText('/path/to/current/file.md')).toBeInTheDocument();
    });

    it('displays error message when error present', () => {
      const errorMessage = 'Failed to index file';
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, status: 'error' },
        error: errorMessage,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Error')).toBeInTheDocument();
      expect(screen.getByText(errorMessage)).toBeInTheDocument();
    });
  });

  describe('Hook Integration', () => {
    it('passes callbacks to the hook', () => {
      render(
        <IndexProgress
          onComplete={mockOnComplete}
          onError={mockOnError}
          onCancel={mockOnCancel}
        />
      );

      expect(useIndexProgress).toHaveBeenCalledWith({
        onComplete: mockOnComplete,
        onError: mockOnError,
        onCancel: mockOnCancel,
      });
    });
  });

  describe('Progress Display', () => {
    it('displays progress bar with correct percentage', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      const progressBar = screen.getByRole('progressbar');
      expect(progressBar).toHaveAttribute('aria-valuenow', '50');
      expect(progressBar).toHaveAttribute('aria-valuemin', '0');
      expect(progressBar).toHaveAttribute('aria-valuemax', '100');
    });

    it('displays current file being processed', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('file.md')).toBeInTheDocument();
      expect(screen.getByText('/path/to/current/file.md')).toBeInTheDocument();
    });

    it('displays failed count when present', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, failed: 5 },
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('5')).toBeInTheDocument();
    });
  });

  describe('Status States', () => {
    it('displays scanning status', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, status: 'scanning' },
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Scanning')).toBeInTheDocument();
    });

    it('displays complete status', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: {
          ...mockProgressData,
          status: 'complete',
          processed: 100,
          totalFiles: 100,
          percentage: 100,
          currentFile: undefined,
        },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Complete')).toBeInTheDocument();
      expect(screen.getByText(/Successfully indexed 100 documents/)).toBeInTheDocument();
    });

    it('displays cancelled status', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, status: 'cancelled' },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Cancelled')).toBeInTheDocument();
    });

    it('displays error status', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, status: 'error' },
        error: 'Test error',
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText('Error')).toBeInTheDocument();
      expect(screen.getByText('Test error')).toBeInTheDocument();
    });
  });

  describe('User Actions', () => {
    it('cancels indexing when cancel button clicked', async () => {
      const user = userEvent.setup();

      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      await user.click(cancelButton);

      expect(mockHandleCancel).toHaveBeenCalled();
    });

    it('shows cancel button only when active', () => {
      // Active state
      const { rerender } = render(<IndexProgress />);
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });
      rerender(<IndexProgress />);
      expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();

      // Inactive state
      (useIndexProgress as any).mockReturnValue({
        progress: { ...mockProgressData, status: 'complete' },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });
      rerender(<IndexProgress />);
      expect(screen.queryByRole('button', { name: /cancel/i })).not.toBeInTheDocument();
    });

    it('shows cancel button in compact mode when active', async () => {
      const user = userEvent.setup();

      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress compact />);

      const cancelButton = screen.getByLabelText(/cancel indexing/i);
      await user.click(cancelButton);

      expect(mockHandleCancel).toHaveBeenCalled();
    });
  });

  describe('Success Messages', () => {
    it('displays success message with no failures', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: {
          status: 'complete',
          processed: 50,
          totalFiles: 50,
          percentage: 100,
          failed: 0,
          currentFile: undefined,
        },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText(/Successfully indexed 50 documents/)).toBeInTheDocument();
    });

    it('displays success message with failures', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: {
          status: 'complete',
          processed: 45,
          totalFiles: 50,
          percentage: 100,
          failed: 5,
          currentFile: undefined,
        },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText(/Successfully indexed 45 documents \(5 failed\)/)).toBeInTheDocument();
    });

    it('uses singular form for single document', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: {
          status: 'complete',
          processed: 1,
          totalFiles: 1,
          percentage: 100,
          failed: 0,
          currentFile: undefined,
        },
        error: null,
        isActive: false,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      expect(screen.getByText(/Successfully indexed 1 document/)).toBeInTheDocument();
    });
  });

  describe('Accessibility', () => {
    it('has accessible progress bar', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      const progressBar = screen.getByRole('progressbar');
      expect(progressBar).toHaveAttribute('aria-label', 'Indexing progress');
    });

    it('has accessible cancel button', () => {
      (useIndexProgress as any).mockReturnValue({
        progress: mockProgressData,
        error: null,
        isActive: true,
        handleCancel: mockHandleCancel,
      });

      render(<IndexProgress />);

      const cancelButton = screen.getByLabelText(/cancel indexing/i);
      expect(cancelButton).toBeInTheDocument();
    });
  });
});
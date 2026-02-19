import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ProgressCard } from './ProgressCard';
import { useProgressStore } from '../../stores/progressStore';
import { type ProgressOperation } from '../../types/progress';

vi.mock('../../stores/progressStore');

describe('ProgressCard', () => {
  const mockCancelOperation = vi.fn();
  const mockRemoveOperation = vi.fn();

  const baseOperation: ProgressOperation = {
    id: 'test-1',
    type: 'indexing',
    status: 'running',
    progress: 50,
    current: 5,
    total: 10,
    message: 'Indexing files...',
    startTime: new Date('2024-01-01T10:00:00'),
    cancellable: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (useProgressStore as any).mockReturnValue({
      cancelOperation: mockCancelOperation,
      removeOperation: mockRemoveOperation,
    });
  });

  it('renders operation type and message', () => {
    render(<ProgressCard operation={baseOperation} />);

    expect(screen.getByText('indexing')).toBeInTheDocument();
    expect(screen.getByText('Indexing files...')).toBeInTheDocument();
  });

  it('displays progress information', () => {
    render(<ProgressCard operation={baseOperation} />);

    expect(screen.getByText('5 / 10')).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
  });

  describe('Status rendering', () => {
    it('shows running status with spinner', () => {
      const { container } = render(<ProgressCard operation={baseOperation} />);

      // Look for the spinner by checking for animate-spin class
      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeTruthy();
    });

    it('shows completed status with check icon', () => {
      const completedOp: ProgressOperation = {
        ...baseOperation,
        status: 'completed',
        progress: 100,
        endTime: new Date('2024-01-01T10:05:00'),
      };

      render(<ProgressCard operation={completedOp} />);

      expect(screen.getByText('Took 5m')).toBeInTheDocument();
    });

    it('shows failed status with error icon', () => {
      const failedOp: ProgressOperation = {
        ...baseOperation,
        status: 'failed',
        errors: ['Connection error'],
        endTime: new Date('2024-01-01T10:02:00'),
      };

      render(<ProgressCard operation={failedOp} />);

      expect(screen.getByText('Took 2m')).toBeInTheDocument();
    });

    it('shows cancelled status', () => {
      const cancelledOp: ProgressOperation = {
        ...baseOperation,
        status: 'cancelled',
        endTime: new Date('2024-01-01T10:01:00'),
      };

      render(<ProgressCard operation={cancelledOp} />);

      expect(screen.getByText('Took 1m')).toBeInTheDocument();
    });

    it('shows pending status', () => {
      const pendingOp: ProgressOperation = {
        ...baseOperation,
        status: 'pending',
        progress: 0,
      };

      render(<ProgressCard operation={pendingOp} />);

      expect(screen.getByText('Indexing files...')).toBeInTheDocument();
    });
  });

  describe('ETA display', () => {
    it('shows ETA in seconds for < 60s', () => {
      const opWithETA: ProgressOperation = {
        ...baseOperation,
        eta: 45,
      };

      render(<ProgressCard operation={opWithETA} />);

      expect(screen.getByText('ETA: 45s')).toBeInTheDocument();
    });

    it('shows ETA in minutes for < 1 hour', () => {
      const opWithETA: ProgressOperation = {
        ...baseOperation,
        eta: 300,
      };

      render(<ProgressCard operation={opWithETA} />);

      expect(screen.getByText('ETA: 5m')).toBeInTheDocument();
    });

    it('shows ETA in hours and minutes for >= 1 hour', () => {
      const opWithETA: ProgressOperation = {
        ...baseOperation,
        eta: 7200,
      };

      render(<ProgressCard operation={opWithETA} />);

      expect(screen.getByText('ETA: 2h 0m')).toBeInTheDocument();
    });
  });

  describe('Actions', () => {
    it('shows cancel button for cancellable running operations', () => {
      render(<ProgressCard operation={baseOperation} />);

      expect(screen.getByLabelText('Cancel operation')).toBeInTheDocument();
    });

    it('does not show cancel button for non-cancellable operations', () => {
      const nonCancellable: ProgressOperation = {
        ...baseOperation,
        cancellable: false,
      };

      render(<ProgressCard operation={nonCancellable} />);

      expect(screen.queryByLabelText('Cancel operation')).not.toBeInTheDocument();
    });

    it('calls cancel when cancel button clicked', async () => {
      const user = userEvent.setup();
      render(<ProgressCard operation={baseOperation} />);

      const cancelButton = screen.getByLabelText('Cancel operation');
      await user.click(cancelButton);

      expect(mockCancelOperation).toHaveBeenCalledWith('test-1');
    });

    it('shows remove button for completed operations', () => {
      const completedOp: ProgressOperation = {
        ...baseOperation,
        status: 'completed',
        progress: 100,
        endTime: new Date(),
      };

      render(<ProgressCard operation={completedOp} />);

      expect(screen.getByLabelText('Remove operation')).toBeInTheDocument();
    });

    it('calls remove when remove button clicked', async () => {
      const user = userEvent.setup();
      const completedOp: ProgressOperation = {
        ...baseOperation,
        status: 'completed',
        progress: 100,
        endTime: new Date(),
      };

      render(<ProgressCard operation={completedOp} />);

      const removeButton = screen.getByLabelText('Remove operation');
      await user.click(removeButton);

      expect(mockRemoveOperation).toHaveBeenCalledWith('test-1');
    });
  });

  describe('Detailed view', () => {
    it('shows error count in detailed view', () => {
      const opWithErrors: ProgressOperation = {
        ...baseOperation,
        status: 'failed',
        errors: ['Error 1', 'Error 2', 'Error 3'],
        endTime: new Date(),
      };

      render(<ProgressCard operation={opWithErrors} detailed />);

      expect(screen.getByText('3 errors')).toBeInTheDocument();
    });

    it('shows first 3 errors in detailed view', () => {
      const opWithErrors: ProgressOperation = {
        ...baseOperation,
        status: 'failed',
        errors: ['Error 1', 'Error 2', 'Error 3', 'Error 4'],
        endTime: new Date(),
      };

      render(<ProgressCard operation={opWithErrors} detailed />);

      expect(screen.getByText('Error 1')).toBeInTheDocument();
      expect(screen.getByText('Error 2')).toBeInTheDocument();
      expect(screen.getByText('Error 3')).toBeInTheDocument();
      expect(screen.getByText('+1 more')).toBeInTheDocument();
    });

    it('does not show errors in non-detailed view', () => {
      const opWithErrors: ProgressOperation = {
        ...baseOperation,
        status: 'failed',
        errors: ['Error 1'],
        endTime: new Date(),
      };

      render(<ProgressCard operation={opWithErrors} detailed={false} />);

      expect(screen.queryByText('Error 1')).not.toBeInTheDocument();
    });
  });

  describe('Operation types', () => {
    it('renders upload icon', () => {
      const uploadOp: ProgressOperation = {
        ...baseOperation,
        type: 'upload',
      };

      render(<ProgressCard operation={uploadOp} />);
      expect(screen.getByText('upload')).toBeInTheDocument();
    });

    it('renders search icon', () => {
      const searchOp: ProgressOperation = {
        ...baseOperation,
        type: 'search',
      };

      render(<ProgressCard operation={searchOp} />);
      expect(screen.getByText('search')).toBeInTheDocument();
    });

    it('renders export icon', () => {
      const exportOp: ProgressOperation = {
        ...baseOperation,
        type: 'export',
      };

      render(<ProgressCard operation={exportOp} />);
      expect(screen.getByText('export')).toBeInTheDocument();
    });

    it('renders ocr icon', () => {
      const ocrOp: ProgressOperation = {
        ...baseOperation,
        type: 'ocr',
      };

      render(<ProgressCard operation={ocrOp} />);
      expect(screen.getByText('ocr')).toBeInTheDocument();
    });
  });

  it('applies custom className', () => {
    const { container } = render(
      <ProgressCard operation={baseOperation} className="custom-class" />
    );

    expect(container.firstChild).toHaveClass('custom-class');
  });

  it('shows progress bar only for active operations', () => {
    const { rerender } = render(<ProgressCard operation={baseOperation} />);

    expect(screen.getByRole('progressbar', { hidden: true })).toBeInTheDocument();

    const completedOp: ProgressOperation = {
      ...baseOperation,
      status: 'completed',
      endTime: new Date(),
    };

    rerender(<ProgressCard operation={completedOp} />);

    expect(screen.queryByRole('progressbar', { hidden: true })).not.toBeInTheDocument();
  });
});

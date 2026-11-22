import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { AggregatedDownload, DownloadStatus } from '@/types/downloads';

import { DownloadFailure } from './DownloadFailure';

describe('DownloadFailure', () => {
  it('gives the error message of a failed download', () => {
    render(<DownloadFailure download={{ state: 'Failed', error_message: 'Disk full.' } as DownloadStatus} />);
    expect(screen.getByRole('status')).toHaveTextContent("Couldn't download: Disk full.");
  });

  it('falls back when there is no message, and reads an aggregated download', () => {
    render(<DownloadFailure download={{ state: 'Failed', error_message: null } as AggregatedDownload} />);
    expect(screen.getByRole('status')).toHaveTextContent('The last download failed.');
  });

  it('renders nothing unless the download failed', () => {
    const { container } = render(<DownloadFailure download={{ state: 'Cancelled', error_message: 'x' } as DownloadStatus} />);
    expect(container).toBeEmptyDOMElement();
    const none = render(<DownloadFailure download={null} />);
    expect(none.container).toBeEmptyDOMElement();
  });
});

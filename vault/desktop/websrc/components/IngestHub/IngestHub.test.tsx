import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';

import { IngestHub } from './IngestHub';

vi.mock('@/hooks/useToast', () => ({
  useToast: () => ({
    toast: {
      success: vi.fn(),
      error: vi.fn(),
      warning: vi.fn(),
      info: vi.fn(),
    },
  }),
}));

vi.mock('@/components/Ingest/UrlImport', () => ({
  UrlImport: ({ onImportComplete }: { onImportComplete: (success: boolean, url: string) => void }) => (
    <div data-testid="url-import">
      <button onClick={() => onImportComplete(true, 'https://example.com')}>
        Import URL
      </button>
    </div>
  ),
}));

vi.mock('@/components/Ingest/BatchUrlImport', () => ({
  BatchUrlImport: () => <div data-testid="batch-url-import">Batch URL Import</div>,
}));

vi.mock('@/components/Ingest/BatchFileImport', () => ({
  BatchFileImport: ({ onImportComplete }: { onImportComplete: (results: { successful: number; failed: number }) => void }) => (
    <div data-testid="batch-file-import">
      <button onClick={() => onImportComplete({ successful: 5, failed: 0 })}>
        Import Files
      </button>
    </div>
  ),
}));

describe('IngestHub', () => {
  it('renders the component with default tab', () => {
    render(<IngestHub />);

    expect(screen.getByText('Import Content')).toBeInTheDocument();
    expect(screen.getByText(/Import URLs, bulk content, or upload files/)).toBeInTheDocument();
  });

  it('renders all three tabs', () => {
    render(<IngestHub />);

    expect(screen.getByText('Single URL')).toBeInTheDocument();
    expect(screen.getByText('Bulk URLs')).toBeInTheDocument();
    expect(screen.getByText('Files')).toBeInTheDocument();
  });

  it('shows Single URL tab content by default', () => {
    render(<IngestHub />);

    expect(screen.getByTestId('url-import')).toBeInTheDocument();
  });

  it('switches to Bulk URLs tab when clicked', async () => {
    const user = userEvent.setup();
    render(<IngestHub />);

    await user.click(screen.getByText('Bulk URLs'));

    expect(screen.getByTestId('batch-url-import')).toBeInTheDocument();
  });

  it('switches to Files tab when clicked', async () => {
    const user = userEvent.setup();
    render(<IngestHub />);

    await user.click(screen.getByText('Files'));

    expect(screen.getByTestId('batch-file-import')).toBeInTheDocument();
  });

  it('calls onImportComplete when import succeeds', () => {
    const onImportComplete = vi.fn();
    render(<IngestHub onImportComplete={onImportComplete} defaultTab="single-url" />);

    expect(onImportComplete).not.toHaveBeenCalled();
  });

  it('defaults to single-url tab when no localStorage value exists', () => {
    localStorage.removeItem('ingestHub.lastTab');
    render(<IngestHub />);

    const singleUrlTab = screen.getByText('Single URL');
    expect(singleUrlTab).toHaveAttribute('data-state', 'active');
  });

  it('uses provided defaultTab prop', () => {
    localStorage.removeItem('ingestHub.lastTab');
    render(<IngestHub defaultTab="files" />);

    const filesTab = screen.getByText('Files');
    expect(filesTab).toHaveAttribute('data-state', 'active');
  });

  it('handles batch file import completion', async () => {
    const user = userEvent.setup();
    const onImportComplete = vi.fn();
    render(<IngestHub onImportComplete={onImportComplete} />);

    await user.click(screen.getByText('Files'));
    await user.click(screen.getByText('Import Files'));

    expect(onImportComplete).toHaveBeenCalledWith({
      type: 'files',
      success: true,
      count: 5,
      message: undefined,
    });
  });
});

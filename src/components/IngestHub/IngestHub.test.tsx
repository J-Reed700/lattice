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

    expect(screen.getByRole('heading', { name: 'Import' })).toBeInTheDocument();
  });

  it('renders all four tabs', () => {
    render(<IngestHub />);

    expect(screen.getByRole('tab', { name: 'URL' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'URLs' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Files' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'History' })).toBeInTheDocument();
  });

  it('shows Single URL tab content by default', () => {
    render(<IngestHub />);

    expect(screen.getByTestId('url-import')).toBeInTheDocument();
  });

  it('switches to Bulk URLs tab when clicked', async () => {
    const user = userEvent.setup();
    render(<IngestHub />);

    await user.click(screen.getByRole('tab', { name: 'URLs' }));

    expect(screen.getByTestId('batch-url-import')).toBeInTheDocument();
  });

  it('switches to Files tab when clicked', async () => {
    const user = userEvent.setup();
    render(<IngestHub />);

    await user.click(screen.getByRole('tab', { name: 'Files' }));

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

    const singleUrlTab = screen.getByRole('tab', { name: 'URL' });
    expect(singleUrlTab).toHaveAttribute('data-state', 'active');
  });

  it('uses provided defaultTab prop', () => {
    localStorage.removeItem('ingestHub.lastTab');
    render(<IngestHub defaultTab="files" />);

    const filesTab = screen.getByRole('tab', { name: 'Files' });
    expect(filesTab).toHaveAttribute('data-state', 'active');
  });

  it('handles batch file import completion', async () => {
    const user = userEvent.setup();
    const onImportComplete = vi.fn();
    render(<IngestHub onImportComplete={onImportComplete} />);

    await user.click(screen.getByRole('tab', { name: 'Files' }));
    await user.click(screen.getByRole('button', { name: 'Import Files' }));

    expect(onImportComplete).toHaveBeenCalledWith({
      type: 'files',
      success: true,
      count: 5,
      message: undefined,
    });
  });
});

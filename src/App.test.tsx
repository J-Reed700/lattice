import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';

const startup = vi.hoisted(() => ({
  initialization: { status: 'pending' } as ReturnType<typeof import('./hooks/useAppInitialization').useAppInitialization>,
  shutdown: { ready: false, error: null as string | null, isQuitting: false },
}));

vi.mock('./hooks/useAppInitialization', () => ({ useAppInitialization: () => startup.initialization }));
vi.mock('./hooks/useNativeShutdown', () => ({ useNativeShutdown: () => startup.shutdown }));
vi.mock('./hooks/useApplyTheme', () => ({ useApplyTheme: () => {} }));
vi.mock('./features/model/hooks/useDownloadedModels', () => ({ useDownloadedModelsListener: () => {} }));
vi.mock('./features/model/hooks/useDownloads', () => ({ useDownloadsListener: () => {} }));
vi.mock('./features/model/hooks/useModelWarmupListener', () => ({ useModelWarmupListener: () => {} }));
vi.mock('./features/vault/hooks/useVaultFocusRescan', () => ({ useVaultFocusRescan: () => {} }));
vi.mock('./features/vault/hooks/useVaultImportListener', () => ({ useVaultImportListener: () => {} }));
vi.mock('./features/vault/hooks/useVaultWriteErrorListener', () => ({ useVaultWriteErrorListener: () => {} }));
vi.mock('./features/model/components/FirstRun', () => ({ FirstRunGate: () => null }));
vi.mock('./routes', () => ({ router: {} }));
vi.mock('react-router/dom', () => ({ RouterProvider: () => <p>App content</p> }));

beforeEach(() => {
  startup.initialization = { status: 'pending' };
  startup.shutdown = { ready: false, error: null, isQuitting: false };
});

describe('application startup', () => {
  it('shows a bridge failure while the shutdown handshake is still pending', () => {
    startup.initialization = { status: 'error', error: 'Bridge unavailable' };
    render(<App />);
    expect(screen.getByText('Bridge unavailable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    expect(screen.queryByText('App content')).not.toBeInTheDocument();
  });

  it('shows a handshake failure while the bridge is still pending', () => {
    startup.shutdown.error = 'Shutdown listener unavailable';
    render(<App />);
    expect(screen.getByText('Shutdown listener unavailable')).toBeInTheDocument();
  });

  it('waits for both dependencies, then renders the app', () => {
    const view = render(<App />);
    expect(screen.getByText('Lattice')).toBeInTheDocument();
    startup.initialization = { status: 'ready' };
    view.rerender(<App />);
    expect(screen.queryByText('App content')).not.toBeInTheDocument();
    startup.shutdown.ready = true;
    view.rerender(<App />);
    expect(screen.getByText('App content')).toBeInTheDocument();
  });
});

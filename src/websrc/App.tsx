import { useEffect, useState } from 'react';

import { RouterProvider } from 'react-router/dom';

import { ErrorToastContainer } from './components/ErrorToast';
import { FirstRunGate } from './components/FirstRun';
import { ToastContainer } from './components/Toast';
import { TooltipProvider } from './components/ui';
import { ErrorProvider, useError } from './contexts/ErrorContext';
import { useApplyTheme } from './hooks/useApplyTheme';
import { useDownloadedModelsListener } from './hooks/useDownloadedModels';
import { useDownloadsListener } from './hooks/useDownloads';
import { useModelWarmupListener } from './hooks/useModelWarmupListener';
import { useNativeShutdown } from './hooks/useNativeShutdown';
import { useProgressCleanup } from './hooks/useProgressCleanup';
import { useVaultFocusRescan } from './hooks/useVaultFocusRescan';
import { useVaultImportListener } from './hooks/useVaultImportListener';
import { useVaultWriteErrorListener } from './hooks/useVaultWriteErrorListener';
import VaultAPI from './lib/api';
import { router } from './routes';

function App() {
  const [isInitializing, setIsInitializing] = useState(true);
  const [initError, setInitError] = useState<string | null>(null);

  const shutdown = useNativeShutdown();

  useApplyTheme();
  useProgressCleanup();

  // Single-mount IPC listeners. Each must be mounted exactly once or every
  // event is applied twice. See each hook's docs.
  useDownloadsListener();
  useModelWarmupListener();
  useVaultWriteErrorListener();
  useVaultImportListener();
  useVaultFocusRescan();
  useDownloadedModelsListener();

  useEffect(() => {
    const initialize = async () => {
      setIsInitializing(true);
      setInitError(null);

      // The backend initializes the database during startup; this call only
      // confirms the bridge is alive.
      const dbResult = await VaultAPI.initializeDatabase();
      if (!dbResult.ok) {
        console.warn('Database initialization check failed:', dbResult.error);
      }

      setIsInitializing(false);
    };

    void initialize();
  }, []);

  if (isInitializing || (!shutdown.ready && !shutdown.error)) {
    return (
      <div className="flex h-screen items-center justify-center bg-bg">
        <span className="font-serif text-lg font-semibold text-text-tertiary">Lattice</span>
      </div>
    );
  }

  if (initError || !shutdown.ready) {
    return (
      <div className="flex h-screen items-center justify-center bg-bg">
        <div className="max-w-sm px-6 text-center">
          <p className="font-serif text-lg font-semibold text-text-primary">Lattice couldn't start.</p>
          <p className="mt-2 text-sm text-text-secondary">{initError ?? shutdown.error}</p>
          <button
            type="button"
            onClick={() => window.location.reload()}
            className="mt-5 inline-flex h-8 items-center rounded-md border border-border-default bg-surface px-3 text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised"
          >
            Try again
          </button>
        </div>
      </div>
    );
  }

  return (
    <TooltipProvider>
      <ErrorProvider>
        <div inert={shutdown.isQuitting || undefined}>
          <AppContent />
        </div>
        {shutdown.isQuitting && (
          <div role="status" className="fixed inset-0 z-[100] flex flex-col items-center justify-center gap-4 bg-bg/90 text-text-primary">
            <span>Saving your work before quitting…</span>
            {shutdown.canCancelQuit && <button type="button" onClick={() => void shutdown.cancelQuit()} className="rounded-md border border-border-default bg-surface px-4 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent">
              Keep working
            </button>}
          </div>
        )}
        {shutdown.error && (
          <div role="alert" className="fixed bottom-4 left-1/2 z-[100] max-w-lg -translate-x-1/2 rounded-lg border border-border-default bg-surface px-5 py-4 text-sm text-text-primary shadow-lg">
            {shutdown.error}
          </div>
        )}
        {/* Mounted after initialization, so the first-run check still runs
            once the database bridge has answered. */}
        <FirstRunGate />
      </ErrorProvider>
    </TooltipProvider>
  );
}

function AppContent() {
  const { errors, dismissError } = useError();

  return (
    <>
      <RouterProvider router={router} />
      <ErrorToastContainer errors={errors} onDismiss={dismissError} position="top-right" />
      <ToastContainer />
    </>
  );
}

export default App;

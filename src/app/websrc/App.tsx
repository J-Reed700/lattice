import { useState, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { RouterProvider } from 'react-router-dom';

import { ErrorToastContainer } from './components/ErrorToast';
import { ModelSetupModal } from './components/FirstRun';
import { ToastContainer } from './components/Toast';
import { TooltipProvider } from './components/ui';
import { ErrorProvider , useError } from './contexts/ErrorContext';
import { useApplyTheme } from './hooks/useApplyTheme';
import { useDownloadedModels } from './hooks/useDownloadedModels';
import { useDownloadsListener } from './hooks/useDownloads';
import { useModelWarmupListener } from './hooks/useModelWarmupListener';
import { useProgressCleanup } from './hooks/useProgressCleanup';
import VaultAPI from './lib/api';
import { router } from './routes';
import { startDownloadCleanup } from './stores/downloadStore';

interface FirstRunStatusResponse {
  needs_setup: boolean;
  recommended_model_id: string | null;
  recommended_model_name: string | null;
  estimated_size_bytes: number | null;
}

function App() {
  console.log('[APP] App component rendering...');
  const [isInitializing, setIsInitializing] = useState(true);
  const [initError, setInitError] = useState<string | null>(null);
  const [showModelSetup, setShowModelSetup] = useState(false);

  // Apply theme from settings store
  useApplyTheme();

  // Periodic cleanup for progress store (prevents memory leaks)
  useProgressCleanup();

  // Initialize the single global download IPC listener. Must NOT be
  // called from any other component or every event will apply twice.
  useDownloadsListener();

  // Subscribe once to backend boot-time model warmup events so the chat
  // input can mask while a cold-mmap is in flight. Single-mount, same as
  // useDownloadsListener.
  useModelWarmupListener();

  // Initialize downloaded models event listeners (always-on for toast notifications)
  useDownloadedModels();

  // Initialize download cleanup timers (prevents HMR accumulation)
  useEffect(() => {
    startDownloadCleanup();
  }, []);

  useEffect(() => {
    const initialize = async () => {
      setIsInitializing(true);
      setInitError(null);

      // Database is already initialized by the backend during startup
      // This call is just for frontend compatibility and returns immediately
      const dbResult = await VaultAPI.initializeDatabase();
      if (!dbResult.ok) {
        // Database is already initialized at startup, so this shouldn't fail
        // But if it does, just log it and continue
        console.warn('Database initialization check failed:', dbResult.error);
      }

      setIsInitializing(false);

      // Check if first-run setup is needed
      const skipped = localStorage.getItem('lattice:first-run-skipped');
      if (!skipped) {
        try {
          // Command returns JSON string to minimize Future state machine size (stack overflow fix)
          const statusJson = await invoke<string>('plugin:model|check_first_run_status');
          const status: FirstRunStatusResponse = JSON.parse(statusJson);
          if (status.needs_setup) {
            console.log('[APP] First run detected, showing model setup');
            setShowModelSetup(true);
          }
        } catch (error) {
          console.error('[APP] Failed to check first-run status:', error);
        }
      }
    };

    initialize();
  }, []);

  const handleModelSetupComplete = () => {
    console.log('[APP] Model setup completed');
    setShowModelSetup(false);
  };

  console.log('[APP] Rendering decision:', { isInitializing, initError });

  if (isInitializing) {
    console.log('[APP] Rendering loading screen');
    return (
      <div style={{ minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', backgroundColor: '#1a1a1a' }}>
        <div style={{ textAlign: 'center' }}>
          <div style={{ width: '48px', height: '48px', border: '2px solid #3b82f6', borderTop: '2px solid transparent', borderRadius: '50%', margin: '0 auto 16px', animation: 'spin 1s linear infinite' }} />
          <p style={{ color: '#ffffff', marginBottom: '8px' }}>Initializing Lattice...</p>
          <p style={{ color: '#9ca3af', fontSize: '14px' }}>Setting up your workspace</p>
        </div>
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      </div>
    );
  }

  if (initError) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-[var(--bg-primary)]">
        <div className="max-w-md p-6 bg-[var(--surface-elevated)] rounded-lg shadow-lg border border-[var(--border-color)]">
          <div className="flex items-center justify-center w-12 h-12 bg-[var(--error-light)] rounded-full mx-auto mb-4">
            <svg className="w-6 h-6 text-[var(--error)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </div>
          <h2 className="text-xl font-semibold text-center text-[var(--text-primary)] mb-2">
            Initialization Failed
          </h2>
          <p className="text-sm text-[var(--text-secondary)] text-center mb-4">{initError}</p>
          <button
            onClick={() => window.location.reload()}
            className="w-full px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  return (
    <TooltipProvider>
      <ErrorProvider>
        <AppContent />
        <ModelSetupModal
          open={showModelSetup}
          onOpenChange={setShowModelSetup}
          onComplete={handleModelSetupComplete}
        />
      </ErrorProvider>
    </TooltipProvider>
  );
}

function AppContent() {
  console.log('[APP] AppContent rendering...');
  const { errors, dismissError } = useError();
  console.log('[APP] AppContent - errors count:', errors.length);

  return (
    <>
      <RouterProvider router={router} />
      <ErrorToastContainer errors={errors} onDismiss={dismissError} position="top-right" />
      <ToastContainer />
    </>
  );
}

export default App;

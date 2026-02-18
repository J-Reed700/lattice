/**
 * Toast Demo Component
 *
 * Purpose: Interactive demo for testing toast notifications
 * Use this component to verify all toast features work correctly
 *
 * To use: Import and render in your app during development
 */

import React, { useState } from 'react';
import { useToast } from '../../hooks/useToast';
import { toastStore } from '../../stores/toastStore';
import { showPromiseToast } from '../../utils/toast';

export const ToastDemo: React.FC = () => {
  const { toast } = useToast();
  const [position, setPosition] = useState<'top-right' | 'top-left' | 'bottom-right' | 'bottom-left' | 'top-center' | 'bottom-center'>('top-right');
  const [duration, setDuration] = useState(4000);

  const handlePositionChange = (newPosition: typeof position) => {
    setPosition(newPosition);
    toastStore.updateConfig({ position: newPosition });
    toast.info(`Position: ${newPosition}`, { duration: 2000 });
  };

  const handleDurationChange = (newDuration: number) => {
    setDuration(newDuration);
    toastStore.updateConfig({ defaultDuration: newDuration });
    toast.info(`Duration: ${newDuration}ms`, { duration: 2000 });
  };

  const simulateAsyncOperation = () => {
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        Math.random() > 0.5 ? resolve({ count: 5 }) : reject(new Error('Operation failed'));
      }, 2000);
    });
  };

  return (
    <div className="fixed bottom-4 left-4 z-50 bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg shadow-lg p-6 max-w-sm">
      <h2 className="text-lg font-semibold mb-4 text-[var(--text-primary)]">
        Toast Demo
      </h2>

      {/* Toast Types */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Toast Types
        </h3>
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => toast.success('Success message')}
            className="px-3 py-2 text-sm bg-[var(--success-light)]0 text-white rounded hover:bg-[var(--success)]"
          >
            Success
          </button>
          <button
            onClick={() => toast.error('Error message', { message: 'Something went wrong' })}
            className="px-3 py-2 text-sm bg-[var(--error)] text-white rounded hover:bg-[var(--error)]"
          >
            Error
          </button>
          <button
            onClick={() => toast.warning('Warning message')}
            className="px-3 py-2 text-sm bg-[var(--warning)] text-white rounded hover:bg-[var(--warning)]"
          >
            Warning
          </button>
          <button
            onClick={() => toast.info('Info message')}
            className="px-3 py-2 text-sm bg-[var(--accent-primary)] text-white rounded hover:bg-[var(--accent-primary)]"
          >
            Info
          </button>
        </div>
      </div>

      {/* With Actions */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          With Actions
        </h3>
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => toast.success('File deleted', {
              action: {
                label: 'Undo',
                onClick: () => toast.info('File restored'),
              },
            })}
            className="px-3 py-2 text-sm bg-[var(--surface-elevated)] text-white rounded hover:bg-[var(--surface-elevated)]"
          >
            With Undo
          </button>
          <button
            onClick={() => toast.info('Update available', {
              action: {
                label: 'Update',
                onClick: () => toast.success('Updating...'),
              },
              duration: 0,
            })}
            className="px-3 py-2 text-sm bg-[var(--surface-elevated)] text-white rounded hover:bg-[var(--surface-elevated)]"
          >
            With Action
          </button>
        </div>
      </div>

      {/* Promise Toast */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Promise Toast
        </h3>
        <button
          onClick={() => {
            showPromiseToast(simulateAsyncOperation(), {
              loading: 'Processing...',
              success: (data: any) => `Processed ${data.count} items`,
              error: (error) => `Failed: ${error.message}`,
            });
          }}
          className="w-full px-3 py-2 text-sm bg-purple-500 text-white rounded hover:bg-purple-600"
        >
          Async Operation
        </button>
      </div>

      {/* Multiple Toasts */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Multiple Toasts
        </h3>
        <button
          onClick={() => {
            toast.success('First toast');
            setTimeout(() => toast.info('Second toast'), 500);
            setTimeout(() => toast.warning('Third toast'), 1000);
          }}
          className="w-full px-3 py-2 text-sm bg-indigo-500 text-white rounded hover:bg-indigo-600"
        >
          Show Multiple
        </button>
      </div>

      {/* Position Controls */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Position
        </h3>
        <div className="grid grid-cols-3 gap-2 text-xs">
          <button
            onClick={() => handlePositionChange('top-left')}
            className={`px-2 py-1 rounded ${position === 'top-left' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Top Left
          </button>
          <button
            onClick={() => handlePositionChange('top-center')}
            className={`px-2 py-1 rounded ${position === 'top-center' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Top Center
          </button>
          <button
            onClick={() => handlePositionChange('top-right')}
            className={`px-2 py-1 rounded ${position === 'top-right' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Top Right
          </button>
          <button
            onClick={() => handlePositionChange('bottom-left')}
            className={`px-2 py-1 rounded ${position === 'bottom-left' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Bottom Left
          </button>
          <button
            onClick={() => handlePositionChange('bottom-center')}
            className={`px-2 py-1 rounded ${position === 'bottom-center' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Bottom Center
          </button>
          <button
            onClick={() => handlePositionChange('bottom-right')}
            className={`px-2 py-1 rounded ${position === 'bottom-right' ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Bottom Right
          </button>
        </div>
      </div>

      {/* Duration Controls */}
      <div className="space-y-2 mb-6">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Duration
        </h3>
        <div className="flex gap-2">
          <button
            onClick={() => handleDurationChange(2000)}
            className={`flex-1 px-2 py-1 text-xs rounded ${duration === 2000 ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            2s
          </button>
          <button
            onClick={() => handleDurationChange(4000)}
            className={`flex-1 px-2 py-1 text-xs rounded ${duration === 4000 ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            4s
          </button>
          <button
            onClick={() => handleDurationChange(6000)}
            className={`flex-1 px-2 py-1 text-xs rounded ${duration === 6000 ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            6s
          </button>
          <button
            onClick={() => handleDurationChange(0)}
            className={`flex-1 px-2 py-1 text-xs rounded ${duration === 0 ? 'bg-[var(--accent-primary)] text-white' : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)]'}`}
          >
            Never
          </button>
        </div>
      </div>

      {/* Controls */}
      <div className="space-y-2">
        <h3 className="text-sm font-medium text-[var(--text-secondary)] mb-2">
          Controls
        </h3>
        <button
          onClick={() => toast.dismissAll()}
          className="w-full px-3 py-2 text-sm bg-[var(--error)] text-white rounded hover:bg-[var(--error)]"
        >
          Dismiss All
        </button>
      </div>

      {/* Keyboard Hint */}
      <div className="mt-4 pt-4 border-t border-[var(--border-color)]">
        <p className="text-xs text-[var(--text-secondary)]">
          Press <kbd className="px-1 py-0.5 bg-[var(--bg-tertiary)] rounded">Esc</kbd> to dismiss all toasts
        </p>
      </div>
    </div>
  );
};

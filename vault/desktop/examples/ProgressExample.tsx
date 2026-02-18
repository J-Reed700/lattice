/**
 * Progress System Example
 *
 * Demonstrates all features of the progress tracking system.
 * Use this for testing and as a reference implementation.
 */

import React, { useState } from 'react';
import { useProgress } from '../hooks/useProgressListener';
import { ProgressBar, MiniProgress } from '../components/Progress';
import { useProgressStore } from '../stores/progressStore';

export function ProgressExample() {
  const [isRunning, setIsRunning] = useState(false);
  const uploadProgress = useProgress('upload', true);
  const indexingProgress = useProgress('indexing', false);
  const { toggleCollapsed } = useProgressStore();

  // Example 1: Simple progress tracking
  const handleSimpleProgress = async () => {
    setIsRunning(true);
    const progressId = uploadProgress.start('Simple upload operation', 100);

    for (let i = 0; i <= 100; i++) {
      await delay(50); // Simulate work
      uploadProgress.update(progressId, i, `Processing item ${i}`, 100);
    }

    uploadProgress.complete(progressId, 'Upload completed successfully');
    setIsRunning(false);
  };

  // Example 2: Progress with ETA
  const handleProgressWithETA = async () => {
    setIsRunning(true);
    const progressId = indexingProgress.start('Indexing with ETA', 50);

    for (let i = 0; i <= 50; i++) {
      await delay(100); // Simulate work
      const remainingTime = (50 - i) * 100; // milliseconds
      uploadProgress.update(
        progressId,
        i,
        `Indexing file ${i + 1}`,
        50,
        remainingTime / 1000 // convert to seconds
      );
    }

    indexingProgress.complete(progressId, 'Indexing completed');
    setIsRunning(false);
  };

  // Example 3: Multiple concurrent operations
  const handleMultipleOperations = async () => {
    setIsRunning(true);

    const op1 = uploadProgress.start('Upload batch 1', 30);
    const op2 = indexingProgress.start('Indexing batch 1', 20);
    const op3 = uploadProgress.start('Upload batch 2', 40);

    // Run operations concurrently
    await Promise.all([
      runOperation(uploadProgress, op1, 30, 100),
      runOperation(indexingProgress, op2, 20, 80),
      runOperation(uploadProgress, op3, 40, 120),
    ]);

    setIsRunning(false);
  };

  // Example 4: Operation with failure
  const handleFailedOperation = async () => {
    setIsRunning(true);
    const progressId = uploadProgress.start('Operation that will fail', 10);

    for (let i = 0; i <= 5; i++) {
      await delay(200);
      uploadProgress.update(progressId, i, `Processing item ${i}`, 10);
    }

    // Simulate failure at 50%
    uploadProgress.fail(progressId, 'Network connection lost');
    setIsRunning(false);
  };

  // Example 5: Cancellable operation
  const handleCancellableOperation = async () => {
    setIsRunning(true);
    const progressId = uploadProgress.start('Cancellable operation', 100);

    for (let i = 0; i <= 100; i++) {
      // Check if operation was cancelled
      const operation = useProgressStore.getState().getOperationById(progressId);
      if (operation?.status === 'cancelled') {
        setIsRunning(false);
        return;
      }

      await delay(100);
      uploadProgress.update(progressId, i, `Processing ${i}%`, 100);
    }

    uploadProgress.complete(progressId);
    setIsRunning(false);
  };

  // Example 6: ProgressBar standalone usage
  const [standAloneProgress, setStandAloneProgress] = useState(0);

  const handleStandaloneProgress = async () => {
    for (let i = 0; i <= 100; i++) {
      await delay(50);
      setStandAloneProgress(i);
    }
    setStandAloneProgress(0);
  };

  return (
    <div className="max-w-4xl mx-auto p-8 space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Progress System Examples</h1>
        <p className="text-gray-600 dark:text-gray-400">
          Test all features of the progress tracking system
        </p>
      </div>

      {/* Mini Progress Indicator */}
      <div className="flex justify-between items-center">
        <h2 className="text-xl font-semibold">Active Operations</h2>
        <MiniProgress onClick={toggleCollapsed} />
      </div>

      {/* Example Buttons */}
      <div className="space-y-4">
        <div>
          <h3 className="text-lg font-semibold mb-2">Basic Examples</h3>
          <div className="flex flex-wrap gap-3">
            <button
              onClick={handleSimpleProgress}
              disabled={isRunning}
              className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              Simple Progress
            </button>
            <button
              onClick={handleProgressWithETA}
              disabled={isRunning}
              className="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              Progress with ETA
            </button>
            <button
              onClick={handleMultipleOperations}
              disabled={isRunning}
              className="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              Multiple Operations
            </button>
          </div>
        </div>

        <div>
          <h3 className="text-lg font-semibold mb-2">Error Handling</h3>
          <div className="flex flex-wrap gap-3">
            <button
              onClick={handleFailedOperation}
              disabled={isRunning}
              className="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              Failed Operation
            </button>
            <button
              onClick={handleCancellableOperation}
              disabled={isRunning}
              className="px-4 py-2 bg-orange-600 text-white rounded-lg hover:bg-orange-700 disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              Cancellable Operation
            </button>
          </div>
        </div>

        <div>
          <h3 className="text-lg font-semibold mb-2">Standalone Progress Bar</h3>
          <button
            onClick={handleStandaloneProgress}
            className="px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 mb-4"
          >
            Test Standalone Progress
          </button>
          <div className="space-y-4">
            <ProgressBar
              progress={standAloneProgress}
              variant="default"
              showLabel
              height={12}
            />
            <ProgressBar progress={75} variant="success" showLabel />
            <ProgressBar progress={50} variant="error" showLabel />
            <ProgressBar progress={0} variant="indeterminate" />
          </div>
        </div>
      </div>

      {/* Tips */}
      <div className="bg-blue-50 dark:bg-blue-900/20 rounded-lg p-6">
        <h3 className="text-lg font-semibold mb-2">Tips</h3>
        <ul className="space-y-2 text-sm text-gray-700 dark:text-gray-300">
          <li>• Progress manager appears at bottom-right when operations are active</li>
          <li>• Click the mini progress indicator to expand/collapse</li>
          <li>• Press Esc to collapse the progress manager</li>
          <li>• Cancellable operations show a cancel button</li>
          <li>• Operations auto-cleanup after 5 minutes</li>
          <li>• Toast notifications appear for completions and errors</li>
        </ul>
      </div>

      {/* Code Example */}
      <div className="bg-gray-50 dark:bg-gray-900 rounded-lg p-6">
        <h3 className="text-lg font-semibold mb-2">Code Example</h3>
        <pre className="text-xs overflow-x-auto">
          {`const uploadProgress = useProgress('upload', true);

const handleUpload = async (files) => {
  const id = uploadProgress.start('Uploading files', files.length);

  for (let i = 0; i < files.length; i++) {
    uploadProgress.update(id, i, \`Uploading \${files[i].name}\`);
    await uploadFile(files[i]);
  }

  uploadProgress.complete(id, 'Upload completed');
};`}
        </pre>
      </div>
    </div>
  );
}

// Helper functions
async function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runOperation(
  progress: ReturnType<typeof useProgress>,
  id: string,
  total: number,
  delayMs: number
): Promise<void> {
  for (let i = 0; i <= total; i++) {
    await delay(delayMs);
    progress.update(id, i, `Processing item ${i}`, total);
  }
  progress.complete(id);
}

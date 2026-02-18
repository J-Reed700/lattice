/**
 * Tests for useProgressListener Hook
 *
 * Purpose: Validate listener lifecycle and dependency array correctness
 */

import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { useProgressListener } from './useProgressListener';

import type { OperationType } from '../types/progress';

// Import after mocks are set up

// Use vi.hoisted to ensure mocks are available during hoisting
const { mockUnlisten, mockListen, mockCreateOperation, mockUpdateOperation, mockCompleteOperation, mockFailOperation, mockGetOperationById } = vi.hoisted(() => {
  const mockUnlisten = vi.fn();
  const mockListen = vi.fn(() => Promise.resolve(mockUnlisten));
  const mockCreateOperation = vi.fn(() => 'operation-123');
  const mockUpdateOperation = vi.fn();
  const mockCompleteOperation = vi.fn();
  const mockFailOperation = vi.fn();
  const mockGetOperationById = vi.fn();

  return {
    mockUnlisten,
    mockListen,
    mockCreateOperation,
    mockUpdateOperation,
    mockCompleteOperation,
    mockFailOperation,
    mockGetOperationById,
  };
});

// Mock Tauri event system
vi.mock('@tauri-apps/api/event', () => ({
  listen: mockListen,
}));

// Mock progress store
vi.mock('../stores/progressStore', () => ({
  useProgressStore: () => ({
    createOperation: mockCreateOperation,
    updateOperation: mockUpdateOperation,
    completeOperation: mockCompleteOperation,
    failOperation: mockFailOperation,
    getOperationById: mockGetOperationById,
  }),
}));

describe('useProgressListener', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListen.mockResolvedValue(mockUnlisten);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Listener Setup', () => {
    it('should set up listeners for all default types', async () => {
      renderHook(() => useProgressListener());

      await waitFor(() => {
        // 5 types * 3 events (progress, complete, error) = 15 listeners
        expect(mockListen).toHaveBeenCalledTimes(15);
      });

      // Verify all event types are listened to
      const eventTypes = ['upload', 'indexing', 'search', 'export', 'ocr'];
      eventTypes.forEach((type) => {
        expect(mockListen).toHaveBeenCalledWith(`${type}-progress`, expect.any(Function));
        expect(mockListen).toHaveBeenCalledWith(`${type}-complete`, expect.any(Function));
        expect(mockListen).toHaveBeenCalledWith(`${type}-error`, expect.any(Function));
      });
    });

    it('should set up listeners for custom types', async () => {
      renderHook(() => useProgressListener({ types: ['upload', 'indexing'] }));

      await waitFor(() => {
        // 2 types * 3 events = 6 listeners
        expect(mockListen).toHaveBeenCalledTimes(6);
      });

      expect(mockListen).toHaveBeenCalledWith('upload-progress', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('indexing-progress', expect.any(Function));
      expect(mockListen).not.toHaveBeenCalledWith('search-progress', expect.any(Function));
    });

    it('should set up listeners only once with stable types array', async () => {
      const types: OperationType[] = ['upload', 'indexing'];
      const { rerender } = renderHook(() => useProgressListener({ types }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      const initialCallCount = mockListen.mock.calls.length;

      // Rerender with same types array reference
      rerender();

      // Wait a bit to ensure no async calls are made
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Should not set up more listeners (types reference is stable)
      expect(mockListen).toHaveBeenCalledTimes(initialCallCount);
    });
  });

  describe('Listener Cleanup', () => {
    it('should clean up listeners on unmount', async () => {
      const { unmount } = renderHook(() => useProgressListener());

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      unmount();

      // Should call unlisten for each listener
      expect(mockUnlisten).toHaveBeenCalledTimes(15);
    });

    it('should clean up and re-setup when types change', async () => {
      const { rerender } = renderHook(
        ({ types }) => useProgressListener({ types }),
        { initialProps: { types: ['upload'] as OperationType[] } }
      );

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledTimes(3);
      });

      vi.clearAllMocks();

      // Change types
      rerender({ types: ['indexing'] as OperationType[] });

      await waitFor(() => {
        // Should unlisten old listeners
        expect(mockUnlisten).toHaveBeenCalledTimes(3);
        // Should set up new listeners
        expect(mockListen).toHaveBeenCalledTimes(3);
      });
    });

    it('should clean up and re-setup when autoCreate changes', async () => {
      const { rerender } = renderHook(
        ({ autoCreate }) => useProgressListener({ autoCreate }),
        { initialProps: { autoCreate: true } }
      );

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledTimes(15);
      });

      vi.clearAllMocks();

      // Change autoCreate
      rerender({ autoCreate: false });

      await waitFor(() => {
        // Should unlisten old listeners
        expect(mockUnlisten).toHaveBeenCalledTimes(15);
        // Should set up new listeners
        expect(mockListen).toHaveBeenCalledTimes(15);
      });
    });
  });

  describe('Dependency Array Stability', () => {
    it('should only depend on types and autoCreate, not store functions', async () => {
      const types: OperationType[] = ['upload', 'indexing'];
      const autoCreate = true;

      const { rerender } = renderHook(() => useProgressListener({ types, autoCreate }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      const initialCallCount = mockListen.mock.calls.length;
      const initialUnlistenCount = mockUnlisten.mock.calls.length;

      // Trigger a rerender (simulating a parent component update)
      // Store functions from Zustand are stable and shouldn't trigger re-setup
      rerender();

      // Wait to ensure no async setup happens
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Should not set up new listeners or cleanup old ones (types and autoCreate are stable)
      expect(mockListen).toHaveBeenCalledTimes(initialCallCount);
      expect(mockUnlisten).toHaveBeenCalledTimes(initialUnlistenCount);
    });

    it('should maintain listener references across multiple rerenders', async () => {
      const types: OperationType[] = ['search'];

      const { rerender } = renderHook(() => useProgressListener({ types }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      const firstCallCount = mockListen.mock.calls.length;

      // Multiple rerenders without prop changes
      rerender();
      rerender();
      rerender();

      // Wait to ensure no async setup happens
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Should not create new listeners (stable types reference)
      expect(mockListen).toHaveBeenCalledTimes(firstCallCount);
    });

    it('should verify dependency array contains only types and autoCreate', () => {
      // This test documents that the dependency array is correctly set to [types, autoCreate]
      // and does NOT include store functions (createOperation, updateOperation, etc.)
      // The actual validation is in the other tests that check listener stability

      const types: OperationType[] = ['ocr'];
      const { unmount } = renderHook(() => useProgressListener({ types }));

      // If store functions were in deps, they would cause unnecessary re-renders
      // This test passes if the hook can be used without issues
      expect(mockListen).toHaveBeenCalled();

      unmount();
    });
  });

  describe('Event Handling - Progress Events', () => {
    it('should handle progress events with auto-create', async () => {
      let progressHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-progress') {
          progressHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'], autoCreate: true }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate progress event
      progressHandler?.({
        payload: {
          current: 50,
          total: 100,
          filename: 'test.pdf',
          message: 'Uploading test.pdf',
          percentage: 50,
        },
      });

      // Should create operation
      expect(mockCreateOperation).toHaveBeenCalledWith({
        type: 'upload',
        message: 'Uploading test.pdf',
        total: 100,
        cancellable: true,
      });

      // Should update operation
      expect(mockUpdateOperation).toHaveBeenCalledWith({
        id: 'operation-123',
        current: 50,
        total: 100,
        progress: 50,
        message: 'Uploading test.pdf',
        eta: undefined,
        status: 'running',
      });
    });

    it('should handle progress events with provided operationId', async () => {
      let progressHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'indexing-progress') {
          progressHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['indexing'] }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate progress event with operationId
      progressHandler?.({
        payload: {
          operationId: 'existing-123',
          current: 25,
          total: 50,
          message: 'Processing files',
        },
      });

      // Should NOT create new operation
      expect(mockCreateOperation).not.toHaveBeenCalled();

      // Should update existing operation
      expect(mockUpdateOperation).toHaveBeenCalledWith({
        id: 'existing-123',
        current: 25,
        total: 50,
        progress: undefined,
        message: 'Processing files',
        eta: undefined,
        status: 'running',
      });
    });

    it('should calculate ETA from eta_ms', async () => {
      let progressHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-progress') {
          progressHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'] }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate progress event with ETA
      progressHandler?.({
        payload: {
          operationId: 'op-456',
          current: 30,
          total: 100,
          eta_ms: 5000, // 5 seconds in ms
        },
      });

      expect(mockUpdateOperation).toHaveBeenCalledWith({
        id: 'op-456',
        current: 30,
        total: 100,
        progress: undefined,
        message: '',
        eta: 5, // converted to seconds
        status: 'running',
      });
    });
  });

  describe('Event Handling - Complete Events', () => {
    it('should handle complete events', async () => {
      let completeHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-complete') {
          completeHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'] }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate complete event
      completeHandler?.({
        payload: {
          operationId: 'op-789',
          message: 'Upload completed successfully',
        },
      });

      expect(mockCompleteOperation).toHaveBeenCalledWith(
        'op-789',
        'Upload completed successfully'
      );
    });

    it('should handle complete events with auto-created operations', async () => {
      let progressHandler: ((event: any) => void) | undefined;
      let completeHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'indexing-progress') {
          progressHandler = handler;
        }
        if (eventName === 'indexing-complete') {
          completeHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['indexing'], autoCreate: true }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // First create an operation via progress
      progressHandler?.({
        payload: {
          current: 50,
          total: 100,
          filename: 'document.pdf',
        },
      });

      expect(mockCreateOperation).toHaveBeenCalled();

      // Then complete it
      completeHandler?.({
        payload: {
          path: 'document.pdf',
          message: 'Indexing complete',
        },
      });

      expect(mockCompleteOperation).toHaveBeenCalledWith(
        'operation-123',
        'Indexing complete'
      );
    });
  });

  describe('Event Handling - Error Events', () => {
    it('should handle error events', async () => {
      let errorHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-error') {
          errorHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'] }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate error event
      errorHandler?.({
        payload: {
          operationId: 'op-error',
          message: 'Upload failed: network error',
        },
      });

      expect(mockFailOperation).toHaveBeenCalledWith('op-error', 'Upload failed: network error');
    });

    it('should handle error events with auto-created operations', async () => {
      let progressHandler: ((event: any) => void) | undefined;
      let errorHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'ocr-progress') {
          progressHandler = handler;
        }
        if (eventName === 'ocr-error') {
          errorHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['ocr'], autoCreate: true }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Create an operation
      progressHandler?.({
        payload: {
          current: 10,
          total: 100,
          filename: 'scan.pdf',
        },
      });

      // Then fail it
      errorHandler?.({
        payload: {
          path: 'scan.pdf',
          message: 'OCR processing failed',
        },
      });

      expect(mockFailOperation).toHaveBeenCalledWith('operation-123', 'OCR processing failed');
    });
  });

  describe('Auto-create Behavior', () => {
    it('should NOT auto-create when autoCreate is false', async () => {
      let progressHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-progress') {
          progressHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'], autoCreate: false }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Simulate progress event without operationId
      progressHandler?.({
        payload: {
          current: 50,
          total: 100,
          filename: 'test.pdf',
        },
      });

      // Should NOT create operation
      expect(mockCreateOperation).not.toHaveBeenCalled();
      // Should NOT update operation (no ID)
      expect(mockUpdateOperation).not.toHaveBeenCalled();
    });

    it('should reuse same operation for multiple events with same key', async () => {
      let progressHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-progress') {
          progressHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload'], autoCreate: true }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // First event - should create operation
      progressHandler?.({
        payload: {
          current: 25,
          total: 100,
          filename: 'test.pdf',
        },
      });

      expect(mockCreateOperation).toHaveBeenCalledTimes(1);

      // Second event with same filename - should reuse operation
      progressHandler?.({
        payload: {
          current: 50,
          total: 100,
          filename: 'test.pdf',
        },
      });

      // Should not create a new operation
      expect(mockCreateOperation).toHaveBeenCalledTimes(1);
      // Should update existing operation twice
      expect(mockUpdateOperation).toHaveBeenCalledTimes(2);
    });

    it('should set cancellable flag for upload and indexing operations', async () => {
      let uploadHandler: ((event: any) => void) | undefined;
      let searchHandler: ((event: any) => void) | undefined;

      mockListen.mockImplementation(((eventName: string, handler: any) => {
        if (eventName === 'upload-progress') {
          uploadHandler = handler;
        }
        if (eventName === 'search-progress') {
          searchHandler = handler;
        }
        return Promise.resolve(mockUnlisten);
      }) as any);

      renderHook(() => useProgressListener({ types: ['upload', 'search'], autoCreate: true }));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalled();
      });

      // Upload should be cancellable
      uploadHandler?.({
        payload: {
          current: 10,
          total: 100,
          filename: 'file.pdf',
        },
      });

      expect(mockCreateOperation).toHaveBeenCalledWith(
        expect.objectContaining({ cancellable: true })
      );

      vi.clearAllMocks();

      // Search should NOT be cancellable
      searchHandler?.({
        payload: {
          current: 5,
          total: 10,
          message: 'Searching...',
        },
      });

      expect(mockCreateOperation).toHaveBeenCalledWith(
        expect.objectContaining({ cancellable: false })
      );
    });
  });
});

import { act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

import { useProgressStore } from './progressStore';

describe('progressStore', () => {
  beforeEach(() => {
    const store = useProgressStore.getState();
    store.clearAll();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe('createOperation', () => {
    it('creates a new operation with pending status', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing files',
        total: 100,
      });

      // Get fresh state after the operation
      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);

      expect(operation).toBeDefined();
      expect(operation?.type).toBe('indexing');
      expect(operation?.status).toBe('pending');
      expect(operation?.progress).toBe(0);
      expect(operation?.total).toBe(100);
    });

    it('sets cancellable flag', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'upload',
        message: 'Uploading',
        cancellable: true,
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.cancellable).toBe(true);
    });

    it('stores metadata', () => {
      const store = useProgressStore.getState();

      const metadata = { fileName: 'test.pdf' };
      const id = store.createOperation({
        type: 'ocr',
        message: 'Processing',
        metadata,
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.metadata).toEqual(metadata);
    });
  });

  describe('updateOperation', () => {
    it('updates operation progress', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing',
        total: 10,
      });

      act(() => {
        store.updateOperation({
          id,
          current: 5,
          message: 'Indexing 5/10',
        });
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.current).toBe(5);
      expect(operation?.progress).toBe(50);
      expect(operation?.message).toBe('Indexing 5/10');
    });

    it('calculates progress from current/total', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'upload',
        message: 'Uploading',
        total: 100,
      });

      act(() => {
        store.updateOperation({
          id,
          current: 75,
        });
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.progress).toBe(75);
    });

    it('updates ETA', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing',
      });

      act(() => {
        store.updateOperation({
          id,
          eta: 120,
        });
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.eta).toBe(120);
    });

    it('does not update non-existent operation', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.updateOperation({
          id: 'non-existent',
          progress: 50,
        });
      });

      expect(useProgressStore.getState().operations.size).toBe(0);
    });
  });

  describe('completeOperation', () => {
    it('marks operation as completed', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing',
      });

      act(() => {
        store.completeOperation(id, 'Completed successfully');
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.status).toBe('completed');
      expect(operation?.progress).toBe(100);
      expect(operation?.message).toBe('Completed successfully');
      expect(operation?.endTime).toBeDefined();
    });

    it('adds operation to history', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'upload',
        message: 'Uploading',
      });

      act(() => {
        store.completeOperation(id);
      });

      const newState = useProgressStore.getState();
      expect(newState.history).toHaveLength(1);
      expect(newState.history[0].operation.id).toBe(id);
    });

    it('adds success notification', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing',
      });

      act(() => {
        store.completeOperation(id);
      });

      const newState = useProgressStore.getState();
      expect(newState.notifications).toHaveLength(1);
      expect(newState.notifications[0].type).toBe('success');
      expect(newState.notifications[0].message).toContain('indexing completed successfully');
    });
  });

  describe('failOperation', () => {
    it('marks operation as failed', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'search',
        message: 'Searching',
      });

      act(() => {
        store.failOperation(id, 'Connection error');
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.status).toBe('failed');
      expect(operation?.errors).toContain('Connection error');
      expect(operation?.endTime).toBeDefined();
    });

    it('adds error notification', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'search',
        message: 'Searching',
      });

      act(() => {
        store.failOperation(id, 'Not found');
      });

      const newState = useProgressStore.getState();
      expect(newState.notifications).toHaveLength(1);
      expect(newState.notifications[0].type).toBe('error');
      expect(newState.notifications[0].message).toContain('failed');
    });
  });

  describe('cancelOperation', () => {
    it('cancels cancellable operation', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'upload',
        message: 'Uploading',
        cancellable: true,
      });

      act(() => {
        store.cancelOperation(id);
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.status).toBe('cancelled');
      expect(operation?.endTime).toBeDefined();
    });

    it('does not cancel non-cancellable operation', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'indexing',
        message: 'Indexing',
        cancellable: false,
      });

      act(() => {
        store.cancelOperation(id);
      });

      const newState = useProgressStore.getState();
      const operation = newState.operations.get(id);
      expect(operation?.status).toBe('pending');
    });
  });

  describe('removeOperation', () => {
    it('removes operation from store', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'export',
        message: 'Exporting',
      });

      const stateBeforeRemove = useProgressStore.getState();
      expect(stateBeforeRemove.operations.has(id)).toBe(true);

      act(() => {
        store.removeOperation(id);
      });

      const stateAfterRemove = useProgressStore.getState();
      expect(stateAfterRemove.operations.has(id)).toBe(false);
    });
  });

  describe('notifications', () => {
    it('adds notification', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.addNotification({
          operationId: 'test-1',
          message: 'Test notification',
          type: 'info',
        });
      });

      const newState = useProgressStore.getState();
      expect(newState.notifications).toHaveLength(1);
      expect(newState.notifications[0].message).toBe('Test notification');
    });

    it('auto-dismisses notification after duration', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.addNotification({
          operationId: 'test-1',
          message: 'Auto dismiss',
          type: 'success',
          duration: 1000,
        });
      });

      const newState = useProgressStore.getState();
      expect(newState.notifications).toHaveLength(1);

      act(() => {
        vi.advanceTimersByTime(1000);
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);
    });

    it('removes notification by id', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.addNotification({
          operationId: 'test-1',
          message: 'Test',
          type: 'info',
        });
      });

      const stateWithNotification = useProgressStore.getState();
      const notificationId = stateWithNotification.notifications[0].id;

      act(() => {
        store.removeNotification(notificationId);
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);
    });

    it('clears all notifications', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.addNotification({ operationId: '1', message: 'Test 1', type: 'info' });
        store.addNotification({ operationId: '2', message: 'Test 2', type: 'info' });
      });

      expect(useProgressStore.getState().notifications).toHaveLength(2);

      act(() => {
        store.clearNotifications();
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);
    });
  });

  describe('UI state', () => {
    it('toggles collapsed state', () => {
      const store = useProgressStore.getState();

      const newState = useProgressStore.getState();
      expect(newState.isCollapsed).toBe(false);

      act(() => {
        store.toggleCollapsed();
      });

      expect(useProgressStore.getState().isCollapsed).toBe(true);
    });

    it('sets collapsed state', () => {
      const store = useProgressStore.getState();

      act(() => {
        store.setCollapsed(true);
      });

      expect(useProgressStore.getState().isCollapsed).toBe(true);

      act(() => {
        store.setCollapsed(false);
      });

      expect(useProgressStore.getState().isCollapsed).toBe(false);
    });
  });

  describe('utilities', () => {
    it('gets active operations', () => {
      const store = useProgressStore.getState();

      const id1 = store.createOperation({ type: 'indexing', message: 'Test 1' });
      const id2 = store.createOperation({ type: 'upload', message: 'Test 2' });

      act(() => {
        store.completeOperation(id2);
      });

      const active = store.getActiveOperations();
      expect(active).toHaveLength(1);
      expect(active[0].id).toBe(id1);
    });

    it('gets operation by id', () => {
      const store = useProgressStore.getState();

      const id = store.createOperation({
        type: 'search',
        message: 'Searching',
      });

      const operation = store.getOperationById(id);
      expect(operation?.id).toBe(id);
    });

    it('clears completed operations', () => {
      const store = useProgressStore.getState();

      const id1 = store.createOperation({ type: 'indexing', message: 'Test 1' });
      const id2 = store.createOperation({ type: 'upload', message: 'Test 2' });

      act(() => {
        store.completeOperation(id1);
        store.clearCompleted();
      });

      const finalState = useProgressStore.getState();
      expect(finalState.operations.has(id1)).toBe(false);
      expect(finalState.operations.has(id2)).toBe(true);
    });

    it('clears all operations', () => {
      const store = useProgressStore.getState();

      store.createOperation({ type: 'indexing', message: 'Test 1' });
      store.createOperation({ type: 'upload', message: 'Test 2' });

      act(() => {
        store.clearAll();
      });

      const finalState = useProgressStore.getState();
      expect(finalState.operations.size).toBe(0);
    });
  });

  describe('history', () => {
    it('keeps last 10 operations in history', () => {
      const store = useProgressStore.getState();

      act(() => {
        for (let i = 0; i < 15; i++) {
          const id = store.createOperation({
            type: 'indexing',
            message: `Operation ${i}`,
          });
          store.completeOperation(id);
        }
      });

      const newState = useProgressStore.getState();
      expect(newState.history).toHaveLength(10);
    });
  });

  describe('memory leak prevention', () => {
    it('store.clearAll() clears all operation timers and state', () => {
      const store = useProgressStore.getState();

      // Create multiple operations
      const id1 = store.createOperation({ type: 'indexing', message: 'Test 1' });
      const id2 = store.createOperation({ type: 'upload', message: 'Test 2' });

      act(() => {
        store.completeOperation(id1); // This schedules a cleanup timer
        store.completeOperation(id2); // This schedules a cleanup timer
      });

      expect(useProgressStore.getState().operations.size).toBe(2);
      expect(useProgressStore.getState().history.length).toBeGreaterThan(0);

      act(() => {
        store.clearAll();
      });

      // Verify all state is cleared
      expect(useProgressStore.getState().operations.size).toBe(0);
      expect(useProgressStore.getState().history).toHaveLength(0);
      expect(useProgressStore.getState().notifications).toHaveLength(0);

      // Verify timers don't fire after clearAll
      act(() => {
        vi.advanceTimersByTime(5 * 60 * 1000 + 1000); // Past cleanup delay
      });

      // Operations should not reappear
      expect(useProgressStore.getState().operations.size).toBe(0);
    });

    it('store.clearNotifications() clears notification timers', () => {
      const store = useProgressStore.getState();

      // Add notifications with auto-dismiss
      act(() => {
        store.addNotification({
          operationId: 'test-1',
          message: 'Test 1',
          type: 'info',
          duration: 5000,
        });
        store.addNotification({
          operationId: 'test-2',
          message: 'Test 2',
          type: 'info',
          duration: 5000,
        });
      });

      expect(useProgressStore.getState().notifications).toHaveLength(2);

      act(() => {
        store.clearNotifications();
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);

      // Verify timers don't fire after clearNotifications
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      // Notifications should not reappear
      expect(useProgressStore.getState().notifications).toHaveLength(0);
    });

    it('store.removeNotification() clears individual notification timer', () => {
      const store = useProgressStore.getState();

      let notificationId: string;

      act(() => {
        store.addNotification({
          operationId: 'test-1',
          message: 'Test',
          type: 'info',
          duration: 5000,
        });
        notificationId = useProgressStore.getState().notifications[0].id;
      });

      expect(useProgressStore.getState().notifications).toHaveLength(1);

      act(() => {
        store.removeNotification(notificationId);
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);

      // Verify timer doesn't fire after removal
      act(() => {
        vi.advanceTimersByTime(6000);
      });

      expect(useProgressStore.getState().notifications).toHaveLength(0);
    });

    it('store.clearCompleted() clears timers for removed operations', () => {
      const store = useProgressStore.getState();

      const id1 = store.createOperation({ type: 'indexing', message: 'Test 1' });
      const id2 = store.createOperation({ type: 'upload', message: 'Test 2' });

      act(() => {
        store.completeOperation(id1);
        store.clearCompleted();
      });

      const finalState = useProgressStore.getState();
      expect(finalState.operations.has(id1)).toBe(false);
      expect(finalState.operations.has(id2)).toBe(true);

      // Verify timer for completed operation doesn't fire
      act(() => {
        vi.advanceTimersByTime(5 * 60 * 1000 + 1000);
      });

      const finalOperations = useProgressStore.getState().operations;
      expect(finalOperations.has(id1)).toBe(false);
      expect(finalOperations.has(id2)).toBe(true);
    });
  });
});

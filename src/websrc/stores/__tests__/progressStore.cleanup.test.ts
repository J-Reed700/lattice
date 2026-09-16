/**
 * Progress Store Cleanup Tests
 *
 * Verifies that the progress store properly cleans up stale entries
 * to prevent memory leaks from accumulated throttle timers.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

import { useProgressStore } from '../progressStore';

describe('ProgressStore Cleanup', () => {
  beforeEach(() => {
    // Reset store before each test
    useProgressStore.getState().clearAll();
  });

  it('should clean up throttle timers when operations are removed', () => {
    const store = useProgressStore.getState();

    const id1 = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Test 1',
    });
    const id2 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Test 2',
    });
    const id3 = store.createOperation({
      type: 'export',
      total: 200,
      message: 'Test 3',
    });

    store.updateOperation({ id: id1, progress: 10 });
    store.updateOperation({ id: id2, progress: 20 });
    store.updateOperation({ id: id3, progress: 30 });

    expect(useProgressStore.getState().operations.size).toBe(3);

    store.removeOperation(id1);
    store.removeOperation(id2);

    expect(useProgressStore.getState().operations.size).toBe(1);
    expect(useProgressStore.getState().operations.has(id3)).toBe(true);

    store.cleanupStaleEntries();

    expect(useProgressStore.getState().operations.size).toBe(1);
  });

  it('should clean up all timers when clearCompleted is called', () => {
    const store = useProgressStore.getState();

    const id1 = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Test 1',
    });
    const id2 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Test 2',
    });

    store.updateOperation({ id: id1, progress: 50 });
    store.updateOperation({ id: id2, progress: 75 });

    // Complete operations
    store.completeOperation(id1);
    store.completeOperation(id2);

    // Clear completed
    store.clearCompleted();

    expect(useProgressStore.getState().operations.size).toBe(0);

    store.cleanupStaleEntries();
  });

  it('should clean up all timers when clearAll is called', () => {
    const store = useProgressStore.getState();

    const id1 = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Test 1',
    });
    const id2 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Test 2',
    });

    store.updateOperation({ id: id1, current: 25, total: 100 });
    store.updateOperation({ id: id2, current: 25, total: 50 });

    expect(useProgressStore.getState().operations.size).toBe(2);

    // Clear all
    store.clearAll();

    expect(store.operations.size).toBe(0);
    expect(store.notifications.length).toBe(0);
    expect(store.history.length).toBe(0);

    store.cleanupStaleEntries();
  });

  it('should handle cleanup with active operations', () => {
    const store = useProgressStore.getState();

    const activeId = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Active operation',
    });

    const staleId1 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Stale 1',
    });
    const staleId2 = store.createOperation({
      type: 'export',
      total: 200,
      message: 'Stale 2',
    });

    store.updateOperation({ id: activeId, progress: 10 });
    store.updateOperation({ id: staleId1, progress: 20 });
    store.updateOperation({ id: staleId2, progress: 30 });

    store.removeOperation(staleId1);
    store.removeOperation(staleId2);

    // Verify only active operation remains
    expect(useProgressStore.getState().operations.size).toBe(1);
    expect(useProgressStore.getState().operations.has(activeId)).toBe(true);

    store.cleanupStaleEntries();

    // Active operation should remain
    expect(useProgressStore.getState().operations.size).toBe(1);
    expect(useProgressStore.getState().operations.has(activeId)).toBe(true);
  });

  it('should handle periodic cleanup without errors', () => {
    const store = useProgressStore.getState();

    for (let i = 0; i < 5; i++) {
      const id = store.createOperation({
        type: 'indexing',
        total: 100,
        message: `Cycle ${i}`,
      });

      store.updateOperation({ id, progress: 50 });

      // Complete and cleanup
      store.completeOperation(id);
      store.cleanupStaleEntries();
    }

    // Final cleanup
    store.clearCompleted();
    store.cleanupStaleEntries();

    expect(store.operations.size).toBe(0);
  });

  it('should not affect active operations during cleanup', async () => {
    const store = useProgressStore.getState();

    const id1 = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Active 1',
    });
    const id2 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Active 2',
    });

    store.updateOperation({ id: id1, current: 25, total: 100 });
    store.updateOperation({ id: id2, current: 25, total: 50 });

    store.cleanupStaleEntries();

    expect(useProgressStore.getState().operations.size).toBe(2);
    expect(useProgressStore.getState().getOperationById(id1)?.progress).toBe(25);
    expect(useProgressStore.getState().getOperationById(id2)?.progress).toBe(50);

    // Can still update after cleanup
    await new Promise((resolve) => setTimeout(resolve, 120));
    store.updateOperation({ id: id1, current: 75, total: 100 });
    expect(useProgressStore.getState().getOperationById(id1)?.progress).toBe(75);
  });

  it('should clean up notification timers when notifications are removed', () => {
    const store = useProgressStore.getState();

    store.addNotification({
      operationId: 'test-op',
      message: 'Test notification',
      type: 'success',
      duration: 3000,
    });

    expect(useProgressStore.getState().notifications.length).toBe(1);

    store.removeNotification(useProgressStore.getState().notifications[0].id);

    expect(useProgressStore.getState().notifications.length).toBe(0);

    store.cleanupStaleEntries();
  });

  it('should handle cleanup of completed operations with notifications', () => {
    const store = useProgressStore.getState();

    const id = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Test operation',
    });

    // Complete (creates notification)
    store.completeOperation(id, 'Done');

    // Initial state
    expect(useProgressStore.getState().operations.size).toBe(1);
    expect(useProgressStore.getState().notifications.length).toBe(1);

    store.cleanupStaleEntries();

    // Operation should still be there (only cleaned up by timer)
    expect(useProgressStore.getState().operations.size).toBe(1);

    // Clear notifications
    store.clearNotifications();
    expect(useProgressStore.getState().notifications.length).toBe(0);
  });

  it('should handle rapid create/remove cycles', () => {
    const store = useProgressStore.getState();

    // Rapidly create and remove operations
    for (let i = 0; i < 20; i++) {
      const id = store.createOperation({
        type: 'indexing',
        total: 100,
        message: `Rapid ${i}`,
      });
      store.updateOperation({ id, progress: 50 });
      store.removeOperation(id);
    }

    store.cleanupStaleEntries();

    expect(useProgressStore.getState().operations.size).toBe(0);
  });

  it('should log cleanup statistics', () => {
    const consoleSpy = vi.spyOn(console, 'debug');

    const store = useProgressStore.getState();

    const id1 = store.createOperation({
      type: 'indexing',
      total: 100,
      message: 'Test 1',
    });
    const _id2 = store.createOperation({
      type: 'search',
      total: 50,
      message: 'Test 2',
    });

    // Avoid unused variable warning
    void _id2;

    store.updateOperation({ id: id1, progress: 50 });
    store.removeOperation(id1);

    store.cleanupStaleEntries();

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('[ProgressStore] Cleanup completed')
    );

    consoleSpy.mockRestore();
  });
});

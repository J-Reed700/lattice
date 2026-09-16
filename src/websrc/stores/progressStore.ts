/**
 * Progress Store
 *
 * Central state management for progress tracking.
 * Tracks multiple concurrent operations with throttled updates.
 */

import { create } from 'zustand';

import {
  type ProgressOperation,
  type ProgressUpdate,
  type CreateOperationParams,
  type ProgressNotification,
  type ProgressHistory,
} from '../types/progress';

interface ProgressStore {
  // State
  operations: Map<string, ProgressOperation>;
  notifications: ProgressNotification[];
  history: ProgressHistory[];
  isCollapsed: boolean;

  createOperation: (_params: CreateOperationParams) => string;
  updateOperation: (_update: ProgressUpdate) => void;
  completeOperation: (_id: string, _message?: string) => void;
  failOperation: (_id: string, _error: string) => void;
  cancelOperation: (_id: string) => void;
  removeOperation: (_id: string) => void;

  // Notifications
  addNotification: (_notification: Omit<ProgressNotification, 'id'>) => void;
  removeNotification: (_id: string) => void;
  clearNotifications: () => void;

  // UI State
  toggleCollapsed: () => void;
  setCollapsed: (_collapsed: boolean) => void;

  // Utilities
  getActiveOperations: () => ProgressOperation[];
  getOperationById: (_id: string) => ProgressOperation | undefined;
  clearCompleted: () => void;
  clearAll: () => void;

  cleanupStaleEntries: () => void;
}

// Throttle update frequency to max 10 updates/second per operation
const updateThrottles = new Map<string, number>();
const THROTTLE_MS = 100;

function shouldThrottle(operationId: string): boolean {
  const lastUpdate = updateThrottles.get(operationId) || 0;
  const now = Date.now();
  if (now - lastUpdate < THROTTLE_MS) {
    return true;
  }
  updateThrottles.set(operationId, now);
  return false;
}

/**
 * Clean up stale throttle timers for operations that no longer exist.
 *
 * This prevents memory leaks from throttle timestamps accumulating over time.
 * Called periodically by the cleanup hook.
 *
 * @param activeOperationIds - Set of currently active operation IDs
 */
function cleanupStaleTimers(activeOperationIds: Set<string>): void {
  const staleIds: string[] = [];

  updateThrottles.forEach((_, operationId) => {
    if (!activeOperationIds.has(operationId)) {
      staleIds.push(operationId);
    }
  });

  staleIds.forEach((id) => {
    updateThrottles.delete(id);
  });

  if (staleIds.length > 0) {
    console.debug(`[ProgressStore] Cleaned up ${staleIds.length} stale throttle timers`);
  }
}

// Cleanup completed operations after 5 minutes
const CLEANUP_DELAY_MS = 5 * 60 * 1000;
const cleanupTimers = new Map<string, NodeJS.Timeout>();

// Track notification auto-dismiss timers for cleanup
const notificationTimers = new Map<string, NodeJS.Timeout>();

function scheduleCleanup(operationId: string, removeOperation: (_id: string) => void) {
  // Clear existing timer if any
  const existingTimer = cleanupTimers.get(operationId);
  if (existingTimer) {
    clearTimeout(existingTimer);
  }

  // Schedule new cleanup
  const timer = setTimeout(() => {
    removeOperation(operationId);
    cleanupTimers.delete(operationId);
    updateThrottles.delete(operationId);
  }, CLEANUP_DELAY_MS);

  cleanupTimers.set(operationId, timer);
}

export const useProgressStore = create<ProgressStore>((set, get) => ({
  operations: new Map(),
  notifications: [],
  history: [],
  isCollapsed: false,

  createOperation: (params: CreateOperationParams): string => {
    const id = `${params.type}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const operation: ProgressOperation = {
      id,
      type: params.type,
      status: 'pending',
      progress: 0,
      current: 0,
      total: params.total || 0,
      message: params.message,
      startTime: new Date(),
      cancellable: params.cancellable ?? false,
      metadata: params.metadata,
    };

    set((state) => {
      const newOperations = new Map(state.operations);
      newOperations.set(id, operation);
      return { operations: newOperations };
    });

    return id;
  },

  updateOperation: (update: ProgressUpdate) => {
    // Throttle updates to prevent excessive re-renders
    if (shouldThrottle(update.id)) {
      return;
    }

    set((state) => {
      const operation = state.operations.get(update.id);
      if (!operation) return state;

      const updatedOperation: ProgressOperation = {
        ...operation,
        progress: update.progress ?? operation.progress,
        current: update.current ?? operation.current,
        total: update.total ?? operation.total,
        message: update.message ?? operation.message,
        status: update.status ?? operation.status,
        eta: update.eta ?? operation.eta,
        errors: update.errors ?? operation.errors,
      };

      if (updatedOperation.total > 0) {
        updatedOperation.progress = Math.min(
          100,
          (updatedOperation.current / updatedOperation.total) * 100
        );
      }

      const newOperations = new Map(state.operations);
      newOperations.set(update.id, updatedOperation);
      return { operations: newOperations };
    });
  },

  completeOperation: (id: string, message?: string) => {
    set((state) => {
      const operation = state.operations.get(id);
      if (!operation) return state;

      const completedOperation: ProgressOperation = {
        ...operation,
        status: 'completed',
        progress: 100,
        endTime: new Date(),
        message: message || operation.message,
        eta: undefined,
      };

      const newOperations = new Map(state.operations);
      newOperations.set(id, completedOperation);

      const newHistory = [
        { operation: completedOperation, timestamp: new Date() },
        ...state.history,
      ].slice(0, 10);

      // Schedule cleanup
      scheduleCleanup(id, get().removeOperation);

      get().addNotification({
        operationId: id,
        message: `${operation.type} completed successfully`,
        type: 'success',
        duration: 3000,
      });

      return {
        operations: newOperations,
        history: newHistory,
      };
    });
  },

  failOperation: (id: string, error: string) => {
    set((state) => {
      const operation = state.operations.get(id);
      if (!operation) return state;

      const failedOperation: ProgressOperation = {
        ...operation,
        status: 'failed',
        endTime: new Date(),
        errors: [...(operation.errors || []), error],
      };

      const newOperations = new Map(state.operations);
      newOperations.set(id, failedOperation);

      const newHistory = [
        { operation: failedOperation, timestamp: new Date() },
        ...state.history,
      ].slice(0, 10);

      // Schedule cleanup
      scheduleCleanup(id, get().removeOperation);

      get().addNotification({
        operationId: id,
        message: `${operation.type} failed: ${error}`,
        type: 'error',
        duration: 5000,
      });

      return {
        operations: newOperations,
        history: newHistory,
      };
    });
  },

  cancelOperation: (id: string) => {
    set((state) => {
      const operation = state.operations.get(id);
      if (!operation?.cancellable) return state;

      const cancelledOperation: ProgressOperation = {
        ...operation,
        status: 'cancelled',
        endTime: new Date(),
      };

      const newOperations = new Map(state.operations);
      newOperations.set(id, cancelledOperation);

      const newHistory = [
        { operation: cancelledOperation, timestamp: new Date() },
        ...state.history,
      ].slice(0, 10);

      // Schedule cleanup
      scheduleCleanup(id, get().removeOperation);

      return {
        operations: newOperations,
        history: newHistory,
      };
    });
  },

  removeOperation: (id: string) => {
    set((state) => {
      const newOperations = new Map(state.operations);
      newOperations.delete(id);

      // Clear timers (always clean both maps)
      const timer = cleanupTimers.get(id);
      if (timer) {
        clearTimeout(timer);
        cleanupTimers.delete(id);
      }

      // Always remove throttle entry to prevent memory leaks
      updateThrottles.delete(id);

      return { operations: newOperations };
    });
  },

  addNotification: (notification: Omit<ProgressNotification, 'id'>) => {
    const id = `notif-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const newNotification: ProgressNotification = { ...notification, id };

    set((state) => ({
      notifications: [...state.notifications, newNotification],
    }));

    // Auto-dismiss after duration
    if (notification.duration) {
      const timer = setTimeout(() => {
        get().removeNotification(id);
      }, notification.duration);
      notificationTimers.set(id, timer);
    }
  },

  removeNotification: (id: string) => {
    // Clear timer if exists
    const timer = notificationTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      notificationTimers.delete(id);
    }

    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    }));
  },

  clearNotifications: () => {
    // Clear all notification timers
    notificationTimers.forEach((timer) => clearTimeout(timer));
    notificationTimers.clear();

    set({ notifications: [] });
  },

  toggleCollapsed: () => {
    set((state) => ({ isCollapsed: !state.isCollapsed }));
  },

  setCollapsed: (collapsed: boolean) => {
    set({ isCollapsed: collapsed });
  },

  getActiveOperations: () => {
    const state = get();
    return Array.from(state.operations.values()).filter(
      (op) => op.status === 'running' || op.status === 'pending'
    );
  },

  getOperationById: (id: string) => get().operations.get(id),

  clearCompleted: () => {
    set((state) => {
      const newOperations = new Map(state.operations);
      const toRemove: string[] = [];

      newOperations.forEach((op, id) => {
        if (op.status === 'completed' || op.status === 'failed' || op.status === 'cancelled') {
          toRemove.push(id);
        }
      });

      toRemove.forEach((id) => {
        newOperations.delete(id);
        const timer = cleanupTimers.get(id);
        if (timer) {
          clearTimeout(timer);
          cleanupTimers.delete(id);
        }
        updateThrottles.delete(id);
      });

      return { operations: newOperations };
    });
  },

  clearAll: () => {
    // Clear all operation timers
    cleanupTimers.forEach((timer) => clearTimeout(timer));
    cleanupTimers.clear();
    updateThrottles.clear();

    // Clear all notification timers
    notificationTimers.forEach((timer) => clearTimeout(timer));
    notificationTimers.clear();

    set({
      operations: new Map(),
      notifications: [],
      history: [],
    });
  },

  cleanupStaleEntries: () => {
    const state = get();
    const activeIds = new Set(state.operations.keys());

    cleanupStaleTimers(activeIds);

    // Optional: Log cleanup statistics
    console.debug(
      `[ProgressStore] Cleanup completed - Active operations: ${activeIds.size}, Throttle entries: ${updateThrottles.size}`
    );
  },
}));

export const selectActiveOperations = (state: ProgressStore) =>
  Array.from(state.operations.values()).filter(
    (op) => op.status === 'running' || op.status === 'pending'
  );

export const selectActiveCount = (state: ProgressStore) =>
  selectActiveOperations(state).length;

export const selectHasActiveOperations = (state: ProgressStore) =>
  selectActiveCount(state) > 0;

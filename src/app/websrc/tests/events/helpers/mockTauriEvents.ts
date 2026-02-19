/**
 * Mock Tauri Event System for Testing
 *
 * Provides a complete mock of Tauri's event system for integration testing.
 * Allows tests to emit events and verify listeners receive them correctly.
 *
 * Usage:
 * ```typescript
 * const mockEvents = new MockTauriEventEmitter();
 * mockEvents.mockListen();
 *
 * // In component
 * listen('download:progress', handler);
 *
 * // In test
 * mockEvents.emit('download:progress', { id: '123', percentage: 50 });
 *
 * // Cleanup
 * mockEvents.restore();
 * ```
 */

import { vi } from 'vitest';

import type { UnlistenFn } from '@tauri-apps/api/event';

type EventHandler<T> = (event: { payload: T }) => void;

export class MockTauriEventEmitter {
  private listeners: Map<string, Set<EventHandler<any>>> = new Map();
  private originalListen: any;

  /**
   * Mock the Tauri listen function
   * Call this in beforeEach()
   */
  mockListen(): void {
    // Store original if not already stored
    if (!this.originalListen) {
      this.originalListen = vi.fn();
    }

    // Mock the listen function
    const mockListenFn = vi.fn(<T>(
      eventName: string,
      handler: EventHandler<T>
    ): Promise<UnlistenFn> => {
      // Add handler to listeners map
      if (!this.listeners.has(eventName)) {
        this.listeners.set(eventName, new Set());
      }
      this.listeners.get(eventName)!.add(handler);

      // Return unlisten function
      const unlisten = () => {
        const handlers = this.listeners.get(eventName);
        if (handlers) {
          handlers.delete(handler);
          if (handlers.size === 0) {
            this.listeners.delete(eventName);
          }
        }
      };

      return Promise.resolve(unlisten);
    });

    // Replace global listen
    vi.mock('@tauri-apps/api/event', () => ({
      listen: mockListenFn,
    }));
  }

  /**
   * Emit an event to all registered listeners
   *
   * @param eventName - Name of the event
   * @param payload - Event payload
   */
  emit<T>(eventName: string, payload: T): void {
    const handlers = this.listeners.get(eventName);
    if (handlers) {
      handlers.forEach((handler) => {
        handler({ payload });
      });
    }
  }

  /**
   * Emit multiple events in sequence with delays
   * Useful for testing event flows
   *
   * @param events - Array of events to emit
   * @param delayMs - Delay between events (default: 10ms)
   */
  async emitSequence(
    events: Array<{ name: string; payload: any }>,
    delayMs: number = 10
  ): Promise<void> {
    for (const event of events) {
      this.emit(event.name, event.payload);
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
  }

  /**
   * Get the number of listeners for an event
   * Useful for verifying listener registration
   *
   * @param eventName - Name of the event
   * @returns Number of registered listeners
   */
  getListenerCount(eventName: string): number {
    return this.listeners.get(eventName)?.size || 0;
  }

  /**
   * Check if a specific event has any listeners
   *
   * @param eventName - Name of the event
   * @returns True if event has listeners
   */
  hasListeners(eventName: string): boolean {
    return this.getListenerCount(eventName) > 0;
  }

  /**
   * Get all registered event names
   *
   * @returns Array of event names
   */
  getEventNames(): string[] {
    return Array.from(this.listeners.keys());
  }

  /**
   * Clear all listeners for a specific event
   *
   * @param eventName - Name of the event
   */
  clearListeners(eventName: string): void {
    this.listeners.delete(eventName);
  }

  /**
   * Clear all listeners for all events
   */
  clearAllListeners(): void {
    this.listeners.clear();
  }

  /**
   * Restore the original listen function
   * Call this in afterEach()
   */
  restore(): void {
    this.clearAllListeners();
    vi.restoreAllMocks();
  }

  /**
   * Reset the mock (clear listeners but keep mock active)
   * Useful between test cases
   */
  reset(): void {
    this.clearAllListeners();
  }
}

/**
 * Helper function to wait for all pending promises
 * Useful for waiting for async event handlers to complete
 */
export async function flushPromises(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Helper function to wait for a specific condition
 * Useful for waiting for state updates after events
 *
 * @param condition - Function that returns true when condition is met
 * @param timeoutMs - Maximum time to wait (default: 1000ms)
 * @param intervalMs - Check interval (default: 10ms)
 */
export async function waitFor(
  condition: () => boolean,
  timeoutMs: number = 1000,
  intervalMs: number = 10
): Promise<void> {
  const startTime = Date.now();

  while (!condition()) {
    if (Date.now() - startTime > timeoutMs) {
      throw new Error(`Timeout waiting for condition after ${timeoutMs}ms`);
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

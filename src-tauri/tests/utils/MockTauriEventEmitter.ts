import type { Event, UnlistenFn } from '@tauri-apps/api/event';

type EventHandler<T> = (event: Event<T>) => void;

export class MockTauriEventEmitter {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private listeners: Map<string, Set<EventHandler<any>>> = new Map();

  async listen<T>(eventName: string, handler: EventHandler<T>): Promise<UnlistenFn> {
    if (!this.listeners.has(eventName)) {
      this.listeners.set(eventName, new Set());
    }

    const handlers = this.listeners.get(eventName)!;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    handlers.add(handler as EventHandler<any>);

    const unlisten: UnlistenFn = () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      handlers.delete(handler as EventHandler<any>);
      if (handlers.size === 0) {
        this.listeners.delete(eventName);
      }
    };

    return unlisten;
  }

  async emit<T>(eventName: string, payload: T): Promise<void> {
    const handlers = this.listeners.get(eventName);
    if (!handlers) {
      return;
    }

    const event: Event<T> = {
      event: eventName,
      id: Math.random(),
      payload,
    };

    for (const handler of handlers) {
      handler(event);
    }
  }

  clear(): void {
    this.listeners.clear();
  }

  getListenerCount(eventName: string): number {
    return this.listeners.get(eventName)?.size ?? 0;
  }
}

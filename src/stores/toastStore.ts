/**
 * Toast Store - Zustand Implementation
 *
 * Purpose: Centralized toast notification management with:
 * - Queue management (max concurrent toasts)
 * - Auto-dismiss with configurable duration
 * - Manual dismiss
 * - Toast types: success, error, warning, info
 * - Position configuration
 */

import { create } from 'zustand';

export interface Toast {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message?: string;
  duration?: number;
  action?: {
    label: string;
    onClick: () => void;
  };
  dismissible?: boolean;
  icon?: React.ReactNode;
  createdAt: number;
}

export interface ToastConfig {
  position: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left' | 'top-center' | 'bottom-center';
  maxToasts: number;
  defaultDuration: number;
  pauseOnHover: boolean;
}

const DEFAULT_CONFIG: ToastConfig = {
  position: 'top-right',
  maxToasts: 5,
  defaultDuration: 4000,
  pauseOnHover: true,
};

interface ToastState {
  toasts: Toast[];
  config: ToastConfig;
}

interface ToastActions {
  addToast: (toast: Omit<Toast, 'id' | 'createdAt'>) => string;
  updateToast: (id: string, patch: Partial<Omit<Toast, 'id' | 'createdAt'>>) => void;
  dismissToast: (id: string) => void;
  dismissAll: () => void;
  updateConfig: (newConfig: Partial<ToastConfig>) => void;
  pauseToast: (id: string) => void;
  resumeToast: (id: string) => void;
}

interface ToastStore extends ToastState, ToastActions {}

interface ToastTimerEntry {
  timer: ReturnType<typeof setTimeout> | null;
  startedAt: number;
  remaining: number;
  paused: boolean;
}

// Auto-dismiss timers
const dismissTimers = new Map<string, ToastTimerEntry>();

function generateId(): string {
  return `toast-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

function enforceMaxToasts(toasts: Toast[], maxToasts: number): { kept: Toast[]; removed: Toast[] } {
  if (toasts.length <= maxToasts) {
    return { kept: toasts, removed: [] };
  }
  // Never evict persistent toasts (duration === 0) such as conflict or write-error warnings.
  let excess = toasts.length - maxToasts;
  const removedIds = new Set<string>();
  for (let i = toasts.length - 1; i >= 0 && excess > 0; i -= 1) {
    const candidate = toasts[i];
    if (candidate && candidate.duration !== 0) {
      removedIds.add(candidate.id);
      excess -= 1;
    }
  }
  if (removedIds.size === 0) {
    return { kept: toasts, removed: [] };
  }
  const kept: Toast[] = [];
  const removed: Toast[] = [];
  for (const item of toasts) {
    if (removedIds.has(item.id)) {
      removed.push(item);
    } else {
      kept.push(item);
    }
  }
  return { kept, removed };
}

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],
  config: DEFAULT_CONFIG,

  addToast: (toast: Omit<Toast, 'id' | 'createdAt'>): string => {
    const id = generateId();
    const { config } = get();

    const newToast: Toast = {
      ...toast,
      id,
      createdAt: Date.now(),
      dismissible: toast.dismissible ?? true,
      duration: toast.duration ?? config.defaultDuration,
    };

    set((state) => {
      const { kept, removed } = enforceMaxToasts([newToast, ...state.toasts], config.maxToasts);
      removed.forEach((t) => {
        const entry = dismissTimers.get(t.id);
        if (entry?.timer) {
          clearTimeout(entry.timer);
        }
        dismissTimers.delete(t.id);
      });

      return { toasts: kept };
    });

    // Schedule auto-dismiss
    if (newToast.duration && newToast.duration > 0) {
      const duration = newToast.duration;
      const timer = setTimeout(() => {
        get().dismissToast(id);
      }, duration);
      dismissTimers.set(id, {
        timer,
        startedAt: Date.now(),
        remaining: duration,
        paused: false,
      });
    }

    return id;
  },

  updateToast: (id: string, patch: Partial<Omit<Toast, 'id' | 'createdAt'>>) => {
    set((state) => ({
      toasts: state.toasts.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    }));
  },

  dismissToast: (id: string) => {
    // Clear timer
    const entry = dismissTimers.get(id);
    if (entry) {
      if (entry.timer) {
        clearTimeout(entry.timer);
      }
      dismissTimers.delete(id);
    }

    set((state) => ({
      toasts: state.toasts.filter((toast) => toast.id !== id),
    }));
  },

  dismissAll: () => {
    // Clear all timers
    dismissTimers.forEach((entry) => {
      if (entry.timer) clearTimeout(entry.timer);
    });
    dismissTimers.clear();

    set({ toasts: [] });
  },

  pauseToast: (id: string) => {
    if (!get().toasts.some((item) => item.id === id)) {
      const stale = dismissTimers.get(id);
      if (stale?.timer) clearTimeout(stale.timer);
      dismissTimers.delete(id);
      return;
    }
    const entry = dismissTimers.get(id);
    if (!entry || entry.paused) return;
    if (entry.timer) {
      clearTimeout(entry.timer);
      entry.timer = null;
    }
    const elapsed = Date.now() - entry.startedAt;
    entry.remaining = Math.max(0, entry.remaining - elapsed);
    entry.paused = true;
  },

  resumeToast: (id: string) => {
    if (!get().toasts.some((item) => item.id === id)) {
      const stale = dismissTimers.get(id);
      if (stale?.timer) clearTimeout(stale.timer);
      dismissTimers.delete(id);
      return;
    }
    const entry = dismissTimers.get(id);
    if (!entry?.paused) return;
    if (entry.remaining <= 0) {
      get().dismissToast(id);
      return;
    }
    entry.paused = false;
    entry.startedAt = Date.now();
    entry.timer = setTimeout(() => {
      get().dismissToast(id);
    }, entry.remaining);
  },

  updateConfig: (newConfig: Partial<ToastConfig>) => {
    set((state) => ({
      config: { ...state.config, ...newConfig },
    }));
  },
}));

// Selectors
export const selectToasts = (state: ToastStore) => state.toasts;
export const selectConfig = (state: ToastStore) => state.config;
export const selectToastCount = (state: ToastStore) => state.toasts.length;

// Helper functions for typed toast creation
export const toast = {
  success: (title: string, options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => useToastStore.getState().addToast({ type: 'success', title, ...options }),

  error: (title: string, options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => useToastStore.getState().addToast({ type: 'error', title, ...options }),

  warning: (title: string, options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => useToastStore.getState().addToast({ type: 'warning', title, ...options }),

  info: (title: string, options?: Partial<Omit<Toast, 'id' | 'type' | 'title' | 'createdAt'>>) => useToastStore.getState().addToast({ type: 'info', title, ...options }),

  dismiss: (id: string) => {
    useToastStore.getState().dismissToast(id);
  },

  dismissAll: () => {
    useToastStore.getState().dismissAll();
  },

  pause: (id: string) => {
    useToastStore.getState().pauseToast(id);
  },

  resume: (id: string) => {
    useToastStore.getState().resumeToast(id);
  },
};

export const toastStore = {
  dismissAll: () => useToastStore.getState().dismissAll(),
  showToast: (toast: Omit<Toast, 'id' | 'createdAt'>) => useToastStore.getState().addToast(toast),
  getToasts: () => useToastStore.getState().toasts,
  getConfig: () => useToastStore.getState().config,
  updateConfig: (config: Partial<ToastConfig>) => useToastStore.getState().updateConfig(config),
  pauseToast: (id: string) => useToastStore.getState().pauseToast(id),
  resumeToast: (id: string) => useToastStore.getState().resumeToast(id),
  subscribe: useToastStore.subscribe,
};

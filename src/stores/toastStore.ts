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
  dismissToast: (id: string) => void;
  dismissAll: () => void;
  updateConfig: (newConfig: Partial<ToastConfig>) => void;
}

interface ToastStore extends ToastState, ToastActions {}

// Auto-dismiss timers
const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

function generateId(): string {
  return `toast-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
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
      let updatedToasts = [newToast, ...state.toasts];

      // Enforce max toasts limit
      if (updatedToasts.length > config.maxToasts) {
        const removed = updatedToasts.slice(config.maxToasts);
        removed.forEach((t) => {
          const timer = dismissTimers.get(t.id);
          if (timer) {
            clearTimeout(timer);
            dismissTimers.delete(t.id);
          }
        });
        updatedToasts = updatedToasts.slice(0, config.maxToasts);
      }

      return { toasts: updatedToasts };
    });

    // Schedule auto-dismiss
    if (newToast.duration && newToast.duration > 0) {
      const timer = setTimeout(() => {
        get().dismissToast(id);
      }, newToast.duration);
      dismissTimers.set(id, timer);
    }

    return id;
  },

  dismissToast: (id: string) => {
    // Clear timer
    const timer = dismissTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      dismissTimers.delete(id);
    }

    set((state) => ({
      toasts: state.toasts.filter((toast) => toast.id !== id),
    }));
  },

  dismissAll: () => {
    // Clear all timers
    dismissTimers.forEach((timer) => clearTimeout(timer));
    dismissTimers.clear();

    set({ toasts: [] });
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
};

export const toastStore = {
  dismissAll: () => useToastStore.getState().dismissAll(),
  showToast: (toast: Omit<Toast, 'id' | 'createdAt'>) => useToastStore.getState().addToast(toast),
  getToasts: () => useToastStore.getState().toasts,
  getConfig: () => useToastStore.getState().config,
  updateConfig: (config: Partial<ToastConfig>) => useToastStore.getState().updateConfig(config),
  subscribe: useToastStore.subscribe,
};

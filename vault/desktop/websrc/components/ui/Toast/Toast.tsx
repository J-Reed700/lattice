import { useEffect, useState } from 'react';

import { X, CheckCircle, AlertCircle, Info, AlertTriangle } from 'lucide-react';

/**
 * Toast
 *
 * Purpose: Brief notifications for user feedback on actions
 *
 * Features:
 * - Auto-dismiss with configurable duration
 * - Manual dismiss option
 * - Different variants (success, error, info, warning)
 * - Smooth enter/exit animations
 * - Queue support for multiple toasts
 * - Dark mode support
 *
 * States: entering, visible, exiting
 * Accessibility: WCAG AA, screen reader announcements, keyboard dismissible
 */

export type ToastVariant = 'success' | 'error' | 'info' | 'warning';

export interface ToastType {
  id: string;
  message: string;
  variant?: ToastVariant;
  duration?: number;
}

interface ToastProps extends ToastType {
  onDismiss: (id: string) => void;
}

function Toast({ id, message, variant = 'info', duration = 5000, onDismiss }: ToastProps) {
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    if (duration > 0) {
      const timer = setTimeout(() => {
        handleDismiss();
      }, duration);

      return () => clearTimeout(timer);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [duration, id]);

  const handleDismiss = () => {
    setIsExiting(true);
    setTimeout(() => {
      onDismiss(id);
    }, 200); // Match animation duration
  };

  const variantStyles = {
    success: {
      container: 'bg-[var(--success-light)] border-[var(--success-light)]',
      icon: 'text-[var(--success)]',
      text: 'text-[var(--success)]',
      Icon: CheckCircle,
    },
    error: {
      container: 'bg-[var(--error-light)] border-[var(--error-light)]',
      icon: 'text-[var(--error)]',
      text: 'text-[var(--error)]',
      Icon: AlertCircle,
    },
    warning: {
      container: 'bg-[var(--warning-light)] border-[var(--warning-light)]',
      icon: 'text-[var(--warning)]',
      text: 'text-[var(--warning)]',
      Icon: AlertTriangle,
    },
    info: {
      container: 'bg-[var(--accent-light)] border-[var(--accent-light)]',
      icon: 'text-[var(--accent-primary)]',
      text: 'text-[var(--accent-primary)]',
      Icon: Info,
    },
  };

  const style = variantStyles[variant];
  const IconComponent = style.Icon;

  return (
    <div
      role="status"
      aria-live="polite"
      aria-atomic="true"
      className={`
        flex items-start gap-3 p-4 rounded-lg border shadow-lg
        transition-all duration-200
        ${style.container}
        ${isExiting ? 'opacity-0 translate-x-full' : 'opacity-100 translate-x-0'}
      `}
    >
      <IconComponent className={`w-5 h-5 flex-shrink-0 ${style.icon}`} />
      <p className={`flex-1 text-sm font-medium ${style.text}`}>{message}</p>
      <button
        onClick={handleDismiss}
        className={`flex-shrink-0 p-0.5 rounded hover:bg-black/5  transition-colors ${style.icon}`}
        aria-label="Dismiss notification"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
}

// Toast Container Component
interface ToastContainerProps {
  toasts: ToastType[];
  onDismiss: (id: string) => void;
}

export function ToastContainer({ toasts, onDismiss }: ToastContainerProps) {
  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-md w-full pointer-events-none">
      <div className="flex flex-col gap-2 pointer-events-auto">
        {toasts.map((toast) => (
          <Toast key={toast.id} {...toast} onDismiss={onDismiss} />
        ))}
      </div>
    </div>
  );
}

// Toast hook for easy usage
export function useToast() {
  const [toasts, setToasts] = useState<ToastType[]>([]);

  const addToast = (message: string, variant: ToastVariant = 'info', duration = 5000) => {
    const id = Math.random().toString(36).substr(2, 9);
    setToasts((prev) => [...prev, { id, message, variant, duration }]);
    return id;
  };

  const dismissToast = (id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  };

  const success = (message: string, duration?: number) => addToast(message, 'success', duration);
  const error = (message: string, duration?: number) => addToast(message, 'error', duration);
  const info = (message: string, duration?: number) => addToast(message, 'info', duration);
  const warning = (message: string, duration?: number) => addToast(message, 'warning', duration);

  return {
    toasts,
    addToast,
    dismissToast,
    success,
    error,
    info,
    warning,
  };
}

export default Toast;

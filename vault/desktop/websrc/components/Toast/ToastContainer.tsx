/**
 * ToastContainer - Renders all active toast notifications
 *
 * Purpose: Container for displaying toast notifications with:
 * - Configurable positioning (top-right, top-left, bottom-right, etc.)
 * - Stack animations (enter/exit)
 * - Z-index management
 * - Accessibility (ARIA live region)
 *
 * Accessibility: WCAG AA, keyboard navigation (Tab, Esc), screen reader support
 */

import React, { useEffect } from 'react';

import { ToastItem } from './ToastItem';
import { useToastStore } from '../../stores/toastStore';

export const ToastContainer: React.FC = () => {
  const { toasts, config, dismissAll } = useToastStore();

  // Global keyboard shortcut: Esc to dismiss all toasts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && toasts.length > 0) {
        dismissAll();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toasts.length, dismissAll]);

  // Get position styles
  const getPositionStyles = (): React.CSSProperties => {
    const baseStyles: React.CSSProperties = {
      position: 'fixed',
      zIndex: 9999,
      pointerEvents: 'none',
      padding: '16px',
      display: 'flex',
      flexDirection: 'column',
      gap: '12px',
      maxHeight: '100vh',
      overflow: 'hidden',
    };

    switch (config.position) {
      case 'top-right':
        return {
          ...baseStyles,
          top: 0,
          right: 0,
          alignItems: 'flex-end',
        };
      case 'top-left':
        return {
          ...baseStyles,
          top: 0,
          left: 0,
          alignItems: 'flex-start',
        };
      case 'top-center':
        return {
          ...baseStyles,
          top: 0,
          left: '50%',
          transform: 'translateX(-50%)',
          alignItems: 'center',
        };
      case 'bottom-right':
        return {
          ...baseStyles,
          bottom: 0,
          right: 0,
          alignItems: 'flex-end',
          flexDirection: 'column-reverse',
        };
      case 'bottom-left':
        return {
          ...baseStyles,
          bottom: 0,
          left: 0,
          alignItems: 'flex-start',
          flexDirection: 'column-reverse',
        };
      case 'bottom-center':
        return {
          ...baseStyles,
          bottom: 0,
          left: '50%',
          transform: 'translateX(-50%)',
          alignItems: 'center',
          flexDirection: 'column-reverse',
        };
      default:
        return {
          ...baseStyles,
          top: 0,
          right: 0,
          alignItems: 'flex-end',
        };
    }
  };

  if (toasts.length === 0) {
    return null;
  }

  return (
    <div
      style={getPositionStyles()}
      aria-live="polite"
      aria-atomic="false"
      role="region"
      aria-label="Notifications"
    >
      {toasts.map((toast, index) => (
        <div
          key={toast.id}
          style={{ pointerEvents: 'auto' }}
          className="animate-slide-in"
        >
          <ToastItem
            toast={toast}
            onDismiss={(id: string) => useToastStore.getState().dismissToast(id)}
            pauseOnHover={config.pauseOnHover}
            index={index}
          />
        </div>
      ))}
    </div>
  );
};

// Add animation styles to global CSS if not already present
const style = document.createElement('style');
style.textContent = `
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateX(100%);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }

  .animate-slide-in {
    animation: slide-in 0.3s ease-out;
  }

  @media (prefers-reduced-motion: reduce) {
    .animate-slide-in {
      animation: none;
    }
  }
`;

if (typeof document !== 'undefined' && !document.querySelector('#toast-animations')) {
  style.id = 'toast-animations';
  document.head.appendChild(style);
}

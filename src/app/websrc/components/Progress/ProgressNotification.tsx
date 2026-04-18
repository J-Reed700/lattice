/**
 * ProgressNotification Component
 *
 * Toast notifications for progress operation completions.
 * Auto-dismiss with customizable duration and actions.
 */

import { memo, useEffect, useState } from 'react';

import { CheckCircle2, XCircle, Info, X } from 'lucide-react';

import { useProgressStore } from '../../stores/progressStore';
import { type ProgressNotification as ProgressNotificationType } from '../../types/progress';

export interface ProgressNotificationsProps {
  /** Position on screen */
  position?: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left';
  /** Custom className */
  className?: string;
}

const NotificationItem = memo<{
  notification: ProgressNotificationType;
  onDismiss: (id: string) => void;
}>(({ notification, onDismiss }) => {
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setIsVisible(true), 10);
    return () => clearTimeout(timer);
  }, []);

  const config = {
    success: {
      icon: <CheckCircle2 className="w-5 h-5" />,
      bgColor: 'bg-[hsl(var(--success-muted))]/20',
      borderColor: 'border-[hsl(var(--success-muted))]',
      textColor: 'text-[hsl(var(--success-fg))]',
      iconColor: 'text-[hsl(var(--success-fg))]',
    },
    error: {
      icon: <XCircle className="w-5 h-5" />,
      bgColor: 'bg-[hsl(var(--danger-muted))]/20',
      borderColor: 'border-[hsl(var(--danger-muted))]',
      textColor: 'text-[hsl(var(--danger-fg))]',
      iconColor: 'text-[hsl(var(--danger-fg))]',
    },
    info: {
      icon: <Info className="w-5 h-5" />,
      bgColor: 'bg-[hsl(var(--accent-muted))]/20',
      borderColor: 'border-[hsl(var(--accent-muted))]',
      textColor: 'text-[hsl(var(--accent))]',
      iconColor: 'text-[hsl(var(--accent))]',
    },
  };

  const style = config[notification.type];

  return (
    <div
      className={`
        ${style.bgColor}
        ${style.borderColor}
        border rounded-lg p-4 shadow-lg
        max-w-sm w-full
        transition-all duration-200 ease-out
        ${isVisible ? 'opacity-100 translate-x-0 scale-100' : 'opacity-0 translate-x-12 scale-95'}
      `}
      role="alert"
      aria-live="polite"
    >
      <div className="flex items-start gap-3">
        <div className={`flex-shrink-0 ${style.iconColor}`}>{style.icon}</div>

        <div className="flex-1 min-w-0">
          <p className={`text-sm font-medium ${style.textColor}`}>{notification.message}</p>

          {notification.action && (
            <button
              onClick={() => {
                notification.action?.onClick();
                onDismiss(notification.id);
              }}
              className={`
                mt-2 text-xs font-semibold ${style.iconColor}
                hover:underline focus:outline-none focus:underline
              `}
            >
              {notification.action.label}
            </button>
          )}
        </div>

        <button
          onClick={() => onDismiss(notification.id)}
          className={`
            flex-shrink-0 p-1 rounded
            hover:bg-[hsl(var(--surface-raised))]
            transition-colors
          `}
          aria-label="Dismiss notification"
        >
          <X className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
        </button>
      </div>
    </div>
  );
});

NotificationItem.displayName = 'NotificationItem';

export const ProgressNotifications = memo<ProgressNotificationsProps>(
  ({ position = 'top-right', className = '' }) => {
    const notifications = useProgressStore((state) => state.notifications);
    const removeNotification = useProgressStore((state) => state.removeNotification);

    const positionClasses = {
      'top-right': 'top-4 right-4',
      'top-left': 'top-4 left-4',
      'bottom-right': 'bottom-4 right-4',
      'bottom-left': 'bottom-4 left-4',
    };

    if (notifications.length === 0) {
      return null;
    }

    return (
      <div
        className={`
          fixed ${positionClasses[position]} z-50
          flex flex-col gap-2
          pointer-events-none
          ${className}
        `}
      >
        {notifications.map((notification) => (
          <div key={notification.id} className="pointer-events-auto">
            <NotificationItem
              notification={notification}
              onDismiss={removeNotification}
            />
          </div>
        ))}
      </div>
    );
  }
);

ProgressNotifications.displayName = 'ProgressNotifications';

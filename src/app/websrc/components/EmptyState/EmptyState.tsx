import { type ReactNode } from 'react';

import Button from '@/components/ui/Button/Button';
import { cn } from '@/lib/utils';

export interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  description: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  className?: string;
}

export const EmptyState = ({
  icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) => (
    <div
      className={cn(
        'flex flex-col items-center justify-center py-16 px-4 text-center',
        className
      )}
    >
      {/* Icon Container */}
      <div className="mb-6 p-6 rounded-full bg-[hsl(var(--accent-muted))]">
        <div className="text-[hsl(var(--accent))] w-12 h-12 flex items-center justify-center">
          {icon}
        </div>
      </div>

      {/* Title */}
      <h3 className="text-2xl font-semibold text-[hsl(var(--text-primary))] mb-2">
        {title}
      </h3>

      {/* Description */}
      <p className="text-[hsl(var(--text-secondary))] max-w-md mb-6">
        {description}
      </p>

      {/* Action Button */}
      {action && (
        <Button onClick={action.onClick} size="lg" variant="primary">
          {action.label}
        </Button>
      )}
    </div>
  );

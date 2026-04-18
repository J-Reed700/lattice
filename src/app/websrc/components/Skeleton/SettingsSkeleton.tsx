/**
 * SettingsSkeleton Component
 *
 * Purpose: Skeleton loader for settings panels
 *
 * Features:
 * - Settings tabs skeleton
 * - Form fields skeleton (labels + inputs)
 * - Section headers and dividers
 * - Button groups skeleton
 *
 * States: loading only
 * Accessibility: Hidden from screen readers
 * Performance: Optimized with memo
 */

import React, { memo } from 'react';

import { Skeleton, SkeletonButton } from './Skeleton';

export interface SettingsSkeletonProps {
  /** Whether to show tabs */
  showTabs?: boolean;

  /** Number of form sections */
  sections?: number;

  /** Additional CSS classes */
  className?: string;
}

export const SettingsSkeleton: React.FC<SettingsSkeletonProps> = memo(({
  showTabs = true,
  sections = 3,
  className = '',
}) => (
    <div className={`${className}`} role="status" aria-label="Loading settings">
      {/* Tabs skeleton */}
      {showTabs && <SettingsTabsSkeleton />}

      {/* Settings content */}
      <div className="p-6 space-y-8">
        {Array.from({ length: sections }).map((_, index) => (
          <SettingsSectionSkeleton key={index} />
        ))}

        {/* Action buttons */}
        <div className="flex items-center justify-between pt-6 border-t border-[hsl(var(--border-subtle))]">
          <SkeletonButton size="md" />
          <div className="flex gap-3">
            <SkeletonButton size="md" />
            <SkeletonButton size="md" />
          </div>
        </div>
      </div>

      <span className="sr-only">Loading settings...</span>
    </div>
  ));

SettingsSkeleton.displayName = 'SettingsSkeleton';

/**
 * Settings tabs skeleton
 */
const SettingsTabsSkeleton: React.FC = memo(() => (
    <div className="border-b border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))]">
      <div className="flex gap-1 px-6 overflow-x-auto" aria-hidden="true">
        {[120, 100, 90, 110, 130, 100, 95].map((width, index) => (
          <div key={index} className="flex items-center gap-2 px-4 py-3">
            <Skeleton width={16} height={16} />
            <Skeleton height="1rem" width={`${width}px`} />
          </div>
        ))}
      </div>
    </div>
  ));

SettingsTabsSkeleton.displayName = 'SettingsTabsSkeleton';

/**
 * Settings section skeleton (header + form fields)
 */
const SettingsSectionSkeleton: React.FC = memo(() => {
  const fieldCount = 3 + Math.floor(Math.random() * 3);

  return (
    <div className="space-y-6" aria-hidden="true">
      {/* Section header */}
      <div className="space-y-2">
        <Skeleton height="1.5rem" width="200px" />
        <Skeleton height="0.875rem" width="350px" />
      </div>

      {/* Form fields */}
      <div className="space-y-6 ml-0 sm:ml-4">
        {Array.from({ length: fieldCount }).map((_, index) => (
          <FormFieldSkeleton key={index} />
        ))}
      </div>
    </div>
  );
});

SettingsSectionSkeleton.displayName = 'SettingsSectionSkeleton';

/**
 * Individual form field skeleton
 */
const FormFieldSkeleton: React.FC = memo(() => {
  // Randomize field type for variety
  const fieldType = Math.random();

  if (fieldType < 0.2) {
    // Toggle/switch field
    return (
      <div className="flex items-center justify-between" aria-hidden="true">
        <div className="space-y-1 flex-1">
          <Skeleton height="1rem" width="180px" />
          <Skeleton height="0.75rem" width="300px" />
        </div>
        <Skeleton width={44} height={24} className="rounded-full" />
      </div>
    );
  }

  if (fieldType < 0.4) {
    // Select/dropdown field
    return (
      <div className="space-y-2" aria-hidden="true">
        <Skeleton height="0.875rem" width="120px" />
        <Skeleton height="40px" width="100%" className="max-w-md rounded-lg" />
        <Skeleton height="0.75rem" width="250px" />
      </div>
    );
  }

  if (fieldType < 0.6) {
    // Checkbox group
    return (
      <div className="space-y-3" aria-hidden="true">
        <Skeleton height="0.875rem" width="140px" />
        <div className="space-y-2 ml-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="flex items-center gap-2">
              <Skeleton width={16} height={16} className="rounded" />
              <Skeleton height="0.875rem" width={`${100 + i * 20}px`} />
            </div>
          ))}
        </div>
      </div>
    );
  }

  // Text input field (default)
  return (
    <div className="space-y-2" aria-hidden="true">
      <Skeleton height="0.875rem" width="140px" />
      <Skeleton height="40px" width="100%" className="max-w-md rounded-lg" />
      <Skeleton height="0.75rem" width="280px" />
    </div>
  );
});

FormFieldSkeleton.displayName = 'FormFieldSkeleton';

/**
 * Compact settings skeleton for smaller panels
 */
export const SettingsSkeletonCompact: React.FC = memo(() => (
    <div className="space-y-6" role="status" aria-label="Loading settings">
      {/* Header */}
      <div className="space-y-2">
        <Skeleton height="1.25rem" width="160px" />
        <Skeleton height="0.75rem" width="250px" />
      </div>

      {/* Form fields */}
      <div className="space-y-4">
        <FormFieldSkeleton />
        <FormFieldSkeleton />
        <FormFieldSkeleton />
      </div>

      {/* Action button */}
      <div className="pt-4 border-t border-[hsl(var(--border-subtle))]">
        <SkeletonButton size="md" />
      </div>

      <span className="sr-only">Loading settings...</span>
    </div>
  ));

SettingsSkeletonCompact.displayName = 'SettingsSkeletonCompact';

/**
 * Settings dialog skeleton (for modal settings)
 */
export const SettingsDialogSkeleton: React.FC = memo(() => (
    <div className="w-full max-w-4xl bg-[hsl(var(--surface-raised))] rounded-lg shadow-xl">
      {/* Dialog header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-[hsl(var(--border-subtle))]">
        <Skeleton height="1.5rem" width="120px" />
        <Skeleton width={24} height={24} />
      </div>

      {/* Dialog content */}
      <div className="flex" style={{ height: '600px' }}>
        {/* Sidebar */}
        <div className="w-64 border-r border-[hsl(var(--border-subtle))] p-4 space-y-2">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="flex items-center gap-3 px-3 py-2" aria-hidden="true">
              <Skeleton width={16} height={16} />
              <Skeleton height="0.875rem" width={`${80 + i * 10}px`} />
            </div>
          ))}
        </div>

        {/* Content */}
        <div className="flex-1 p-6 overflow-auto">
          <SettingsSkeleton showTabs={false} sections={2} />
        </div>
      </div>

      <span className="sr-only">Loading settings dialog...</span>
    </div>
  ));

SettingsDialogSkeleton.displayName = 'SettingsDialogSkeleton';

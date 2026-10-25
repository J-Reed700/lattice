import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

import { Tabs, TabsList, TabsTrigger } from './tabs';
import './interactions.css';

type NavigationOption<T extends string> = { id: T; label: string; badge?: ReactNode };

/** Manual activation lets a workspace save drafts before changing its panel. */
export function NavigationTabs<T extends string>({ label, value, options, onValueChange, disabled, appearance = 'underline', idPrefix, panelId, actions, className }: {
  label: string;
  value: T;
  options: ReadonlyArray<NavigationOption<T>>;
  onValueChange: (value: T) => void;
  disabled?: boolean;
  appearance?: 'underline' | 'segmented';
  idPrefix?: string;
  panelId?: string;
  actions?: ReactNode;
  className?: string;
}) {
  return <Tabs value={value} onValueChange={(next) => onValueChange(next as T)} activationMode="manual" className={cn('ui-navigation-tabs', `ui-navigation-tabs--${appearance}`, className)}>
    <TabsList aria-label={label} className="ui-navigation-tablist">
      {options.map((option) => <TabsTrigger key={option.id} id={idPrefix ? `${idPrefix}-${option.id}` : undefined} value={option.id} aria-controls={panelId} disabled={disabled} className="ui-navigation-tab">{option.label}{option.badge != null && <span className="ui-navigation-badge">{option.badge}</span>}</TabsTrigger>)}
    </TabsList>
    {actions && <div className="ui-navigation-actions">{actions}</div>}
  </Tabs>;
}

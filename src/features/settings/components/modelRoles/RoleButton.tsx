import { useState } from 'react';

import { Check, Plus } from 'lucide-react';

import { cn } from '@/lib/utils';
import type { DownloadedModel } from '@/types/downloadedModels';

import { useModelRoles } from './ModelRolesContext';
import { canModelFulfillRole, isReferenceRole, type RoleDescriptor, type RoleId } from './roleConfig';


interface RoleButtonProps {
  model: DownloadedModel;
  role: RoleDescriptor;
}

/** A role a row holds, shown on the row; the toggles live in its Configure menu. */
export function ActiveRoleBadge({ label, roleId }: { label: string; roleId: RoleId }) {
  return (
    <span
      data-active-role={roleId}
      className="inline-flex h-6 items-center gap-1 rounded-md border border-accent/25 bg-accent-muted px-2 text-xs font-medium text-accent"
    >
      <Check className="h-3 w-3" aria-hidden="true" />
      {label}
    </span>
  );
}

interface RoleToggleProps {
  role: RoleDescriptor;
  active: boolean;
  disabled?: boolean;
  pending?: boolean;
  /** Says why the toggle is disabled; otherwise the role's own hint shows. */
  title?: string;
  onToggle: () => void;
}

export function RoleToggle({ role, active, disabled = false, pending = false, title, onToggle }: RoleToggleProps) {
  return (
    <button
      type="button"
      onClick={onToggle}
      disabled={disabled || pending}
      aria-pressed={active}
      data-role={role.id}
      title={title ?? (active ? `Active ${role.label.toLowerCase()} model. Click to stop using it.` : role.hint)}
      className={cn(
        'inline-flex h-7 items-center gap-1 rounded-md border px-2 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
        active
          ? 'border-accent/25 bg-accent-muted font-medium text-accent'
          : 'border-border-default text-text-secondary hover:bg-surface-raised hover:text-text-primary',
        (disabled || pending) && 'cursor-not-allowed opacity-60',
      )}
    >
      {active ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
      {role.label}
    </button>
  );
}

export function RoleButton({ model, role }: RoleButtonProps) {
  const { assignRole, chatProvider, llmSettings } = useModelRoles();
  const [pending, setPending] = useState(false);

  if (!canModelFulfillRole(model, role)) {
    // Model type can't serve this role — don't render (e.g. embedding model for chat).
    return null;
  }

  const isActive = role.isActive(model, llmSettings);
  // Chat writes the provider, and Learning and Verifier write a model
  // reference, so each waits for the settings it writes into.
  const waitsForSettings = (role.id === 'chat' && !chatProvider) || (isReferenceRole(role.id) && !llmSettings);

  const handleClick = async () => {
    if (pending) return;
    setPending(true);
    try {
      await assignRole(isActive ? null : model.model_id, role.id);
    } finally {
      setPending(false);
    }
  };

  return (
    <RoleToggle
      role={role}
      active={isActive}
      disabled={waitsForSettings}
      pending={pending}
      onToggle={() => void handleClick()}
    />
  );
}

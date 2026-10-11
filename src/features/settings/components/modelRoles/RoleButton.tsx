/**
 * RoleButton
 *
 * One small text toggle per role, generated from a {@link RoleDescriptor}.
 * Active = accent text on an accent-muted ground; inactive = ghost. The
 * button decides its own enabled/active/pending state from the model + role.
 *
 * It toggles both ways. A model that holds a role cannot be deleted, so a
 * button that only switched a role on left the last embedding model with no
 * way to be removed.
 *
 * Presentation only — the ModelRolesContext is the mutation layer.
 */

import { useState } from 'react';

import { Check, Plus } from 'lucide-react';

import { cn } from '@/lib/utils';
import type { DownloadedModel } from '@/types/downloadedModels';

import { useModelRoles } from './ModelRolesContext';
import { canModelFulfillRole, isChatModelSelected, isModelActiveForRole, type RoleDescriptor } from './roleConfig';


interface RoleButtonProps {
  model: DownloadedModel;
  role: RoleDescriptor;
}

export function RoleButton({ model, role }: RoleButtonProps) {
  const { assignRole, chatProvider } = useModelRoles();
  const [pending, setPending] = useState(false);

  if (!canModelFulfillRole(model, role)) {
    // Model type can't serve this role — don't render (e.g. embedding model for chat).
    return null;
  }

  const isActive = role.id === 'chat'
    ? isChatModelSelected(model, chatProvider)
    : isModelActiveForRole(model, role);

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
    <button
      type="button"
      onClick={handleClick}
      disabled={pending || (role.id === 'chat' && !chatProvider)}
      aria-pressed={isActive}
      title={isActive ? `Active ${role.label.toLowerCase()} model. Click to stop using it.` : role.hint}
      className={cn(
        'inline-flex h-7 items-center gap-1 rounded-md border px-2 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
        isActive
          ? 'border-accent/25 bg-accent-muted font-medium text-accent'
          : 'border-border-default text-text-secondary hover:bg-surface-raised hover:text-text-primary',
        pending && 'opacity-60',
      )}
    >
      {isActive ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
      {role.label}
    </button>
  );
}

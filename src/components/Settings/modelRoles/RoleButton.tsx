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

import { cn } from '@/lib/utils';

import { useModelRoles } from './ModelRolesContext';
import { canModelFulfillRole, isModelActiveForRole, type RoleDescriptor } from './roleConfig';

import type { DownloadedModel } from '../../../types/downloadedModels';

interface RoleButtonProps {
  model: DownloadedModel;
  role: RoleDescriptor;
}

export function RoleButton({ model, role }: RoleButtonProps) {
  const { assignRole } = useModelRoles();
  const [pending, setPending] = useState(false);

  if (!canModelFulfillRole(model, role)) {
    // Model type can't serve this role — don't render (e.g. embedding model for chat).
    return null;
  }

  const isActive = isModelActiveForRole(model, role);

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
      disabled={pending}
      aria-pressed={isActive}
      title={isActive ? `Active ${role.label.toLowerCase()} model. Click to stop using it.` : role.hint}
      className={cn(
        'h-7 rounded-sm px-2 text-xs transition-colors duration-fast',
        isActive
          ? 'bg-accent-muted text-accent'
          : 'text-text-muted hover:bg-surface-raised hover:text-text-primary',
        pending && 'opacity-60',
      )}
    >
      {role.label}
    </button>
  );
}

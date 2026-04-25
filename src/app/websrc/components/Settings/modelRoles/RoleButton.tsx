/**
 * RoleButton
 *
 * Single role button generated from a {@link RoleDescriptor}. The parent
 * card renders `<RoleButton>` once per role in the ROLES array; the button
 * decides its own enabled/active/pending visual state from the model + role.
 *
 * Kept intentionally minimal — presentation only, no API calls. The
 * ModelRolesContext is the mutation layer.
 */

import { useState } from 'react';

import { Check } from 'lucide-react';

import { Button } from '../../ui/button';
import { useModelRoles } from './ModelRolesContext';
import {
  canModelFulfillRole,
  isModelActiveForRole,
  type RoleDescriptor,
} from './roleConfig';

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
    if (pending || isActive) return;
    setPending(true);
    try {
      await assignRole(model.model_id, role.id);
    } finally {
      setPending(false);
    }
  };

  return (
    <Button
      type="button"
      variant={isActive ? 'default' : 'outline'}
      size="sm"
      onClick={handleClick}
      disabled={pending || isActive}
      className="flex-1 min-w-0 text-xs"
      title={isActive ? `Currently the active ${role.label} model` : role.hint}
    >
      {pending ? (
        role.pendingLabel
      ) : isActive ? (
        <span className="flex items-center gap-1">
          <Check className="w-3 h-3" /> {role.activeLabel}
        </span>
      ) : (
        role.assignLabel
      )}
    </Button>
  );
}

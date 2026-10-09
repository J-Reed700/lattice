import { useEffect, useId, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { VaultAPI } from '@/lib/api';
import type { ConversationSpaceDto } from '@/types/api/conversation';


/** What the dialog edits: a row of "Your folders", or the folder open now. */
export interface FolderSettingsTarget {
  root: string;
  name: string;
  instructions: string | null;
  spaceId: string;
  threadCount: number;
}

export interface FolderSettingsDialogProps {
  folder: FolderSettingsTarget;
  onCancel: () => void;
  /** Resolves `true` once saved; the dialog stays open on `false`. */
  onSave: (_instructions: string, _spaceId: string) => Promise<boolean>;
}

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/**
 * A folder's own system prompt and the space its threads belong to. The
 * space decides which library the folder's chat can search and which space
 * memory it reads; the instructions stand in for that space's prompt.
 */
export function FolderSettingsDialog({ folder, onCancel, onSave }: FolderSettingsDialogProps) {
  const [spaces, setSpaces] = useState<ConversationSpaceDto[]>([]);
  const [instructions, setInstructions] = useState(folder.instructions ?? '');
  const [spaceId, setSpaceId] = useState(folder.spaceId);
  const [saving, setSaving] = useState(false);
  const spaceLabelId = useId();
  const instructionsId = useId();

  // Read fresh: a space made in Chat a moment ago is offered too.
  useEffect(() => {
    let live = true;
    void VaultAPI.listConversationSpaces().then((result) => {
      if (live && result.ok) setSpaces(result.data);
    });
    return () => {
      live = false;
    };
  }, []);

  // General first, then the open spaces in their sidebar order. An archived
  // space the folder already uses stays listed so the choice isn't lost.
  const options = useMemo(() => {
    const general = spaces.find((space) => space.id === GENERAL_SPACE_ID);
    const rest = spaces
      .filter((space) => space.id !== GENERAL_SPACE_ID && (!space.isArchived || space.id === folder.spaceId))
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name));
    return [{ id: GENERAL_SPACE_ID, name: general?.name ?? 'General' }, ...rest.map(({ id, name }) => ({ id, name }))];
  }, [folder.spaceId, spaces]);

  const spaceChanged = spaceId !== folder.spaceId;
  const unchanged = !spaceChanged && instructions.trim() === (folder.instructions ?? '').trim();

  const save = async () => {
    setSaving(true);
    try {
      if (await onSave(instructions, spaceId)) onCancel();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && !saving && onCancel()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{folder.name} settings</DialogTitle>
          <DialogDescription>Every chat about this folder uses these.</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-1.5">
          <span id={spaceLabelId} className="text-sm font-medium text-text-primary">Space</span>
          <Select value={spaceId} onValueChange={setSpaceId} disabled={saving}>
            <SelectTrigger aria-labelledby={spaceLabelId}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {options.map((space) => (
                <SelectItem key={space.id} value={space.id}>{space.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-text-muted">
            {spaceChanged && folder.threadCount > 0
              ? `Its ${plural(folder.threadCount, 'chat moves', 'chats move')} to this space too.`
              : 'Library search from this folder’s chat reads this space’s documents.'}
          </p>
        </div>

        <div className="flex flex-col gap-1.5">
          <label htmlFor={instructionsId} className="text-sm font-medium text-text-primary">System prompt</label>
          <textarea
            id={instructionsId}
            value={instructions}
            disabled={saving}
            onChange={(event) => setInstructions(event.target.value)}
            rows={7}
            maxLength={20_000}
            placeholder="For example: This is a JUCE audio plugin in C++20. Explain at a senior level and point to files with line numbers."
            className="w-full resize-y rounded-md border border-border-default bg-surface px-2.5 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-hidden focus:ring-2 focus:ring-[hsl(var(--ring))]"
          />
          <p className="text-xs text-text-muted">Used instead of the space’s prompt. Leave empty to use the space’s.</p>
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" disabled={saving} onClick={onCancel}>
            Cancel
          </Button>
          <Button type="button" disabled={saving || unchanged} onClick={() => void save()}>
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

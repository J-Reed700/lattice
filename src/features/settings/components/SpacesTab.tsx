import { useState } from 'react';

import { Button, Input, PageHeader, SettingsRow, SettingsSection } from '@/components/ui';
import { useOpenSpaces } from '@/features/spaces/components/SpacePickerPopover';
import { useSpaceEditor } from '@/features/spaces/hooks/useSpaceEditor';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

export function SpacesTab() {
  const navigate = useAppNavigate();
  const setSelectedSpace = useConversationsStore((state) => state.setSelectedSpace);
  const { isCreatingSpace, createSpace } = useSpaceEditor();
  const activeSpaces = useOpenSpaces();
  const [newSpaceNameDraft, setNewSpaceNameDraft] = useState('');

  return (
    <>
      <PageHeader title="Spaces" description="Keep related conversations grouped in dedicated workspaces." />

      <SettingsSection title="New space" description="Keep related conversations together.">
        <form
          onSubmit={handleAsyncEvent(async (event) => {
            event.preventDefault();
            if (!isCreatingSpace && (await createSpace(newSpaceNameDraft))) {
              setNewSpaceNameDraft('');
            }
          })}
        >
          <SettingsRow label="Space name" htmlFor="settings-new-space-name" stacked>
            <div className="flex flex-wrap items-center gap-2">
              <div className="min-w-0 flex-1 basis-48">
                <Input
                  id="settings-new-space-name"
                  value={newSpaceNameDraft}
                  onChange={(event) => setNewSpaceNameDraft(event.target.value)}
                  placeholder="e.g. Product, Research, Personal"
                  disabled={isCreatingSpace}
                />
              </div>
              <Button type="submit" disabled={isCreatingSpace}>
                {isCreatingSpace ? 'Creating…' : 'Create space'}
              </Button>
            </div>
          </SettingsRow>
        </form>
      </SettingsSection>

      {activeSpaces.length > 0 && (
        <SettingsSection title="Your spaces">
          {activeSpaces.map((space) => (
            <SettingsRow
              key={space.id}
              label={`${space.icon ? `${space.icon} ` : ''}${space.name}`}
              hint={space.description}
            >
              <Button
                type="button"
                variant="outline"
                aria-label={`Open ${space.name} in Chat`}
                onClick={() => {
                  setSelectedSpace(space.id);
                  navigate('/chat');
                }}
              >
                Open in Chat
              </Button>
            </SettingsRow>
          ))}
        </SettingsSection>
      )}
    </>
  );
}

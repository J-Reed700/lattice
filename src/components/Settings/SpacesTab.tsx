import { useState } from 'react';

import { useNavigate } from 'react-router';

import { PageHeader, SettingsRow, SettingsSection, settingsFieldClass } from '@/components/ui';
import { useOpenSpaces } from '@/features/spaces/components/SpacePickerPopover';
import { useSpaceEditor } from '@/features/spaces/hooks/useSpaceEditor';
import { useConversationsStore } from '@/stores/conversationsStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { PRIMARY_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from './settingsStyles';

export function SpacesTab() {
  const navigate = useNavigate();
  const setSelectedSpace = useConversationsStore((state) => state.setSelectedSpace);
  const { isCreatingSpace, createSpace } = useSpaceEditor();
  const activeSpaces = useOpenSpaces();
  const [newSpaceNameDraft, setNewSpaceNameDraft] = useState('');

  return (
    <>
      <PageHeader title="Spaces" />

      <SettingsSection title="New space" description="Keep related conversations together.">
        <form onSubmit={handleAsyncEvent(async (event) => {
          event.preventDefault();
          if (!isCreatingSpace && await createSpace(newSpaceNameDraft)) setNewSpaceNameDraft('');
        })}>
          <SettingsRow label="Space name" htmlFor="settings-new-space-name" stacked>
            <div className="flex flex-wrap items-center gap-2">
              <input
                id="settings-new-space-name"
                value={newSpaceNameDraft}
                onChange={event => setNewSpaceNameDraft(event.target.value)}
                placeholder="e.g. Product, Research, Personal"
                disabled={isCreatingSpace}
                className={`${settingsFieldClass} min-w-0 flex-1 basis-48`}
              />
              <button type="submit" disabled={isCreatingSpace} className={PRIMARY_BUTTON_CLASS}>
                {isCreatingSpace ? 'Creating…' : 'Create space'}
              </button>
            </div>
          </SettingsRow>
        </form>
      </SettingsSection>

      {activeSpaces.length > 0 && (
        <SettingsSection title="Your spaces">
          {activeSpaces.map(space => (
            <SettingsRow
              key={space.id}
              label={`${space.icon ? `${space.icon} ` : ''}${space.name}`}
              hint={space.description}
            >
              <button
                type="button"
                aria-label={`Open ${space.name} in Chat`}
                onClick={() => {
                  setSelectedSpace(space.id);
                  navigate('/chat');
                }}
                className={SECONDARY_BUTTON_CLASS}
              >
                Open in Chat
              </button>
            </SettingsRow>
          ))}
        </SettingsSection>
      )}
    </>
  );
}

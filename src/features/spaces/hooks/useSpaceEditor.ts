import { useEffect, useMemo, useState } from 'react';

import { useSpaceMutations } from '@/features/spaces/api/queries';
import {
  buildSpaceToolPreferencesJson,
  GENERAL_SPACE_ID,
  normalizeHexColor,
  parseSpaceToolPreferences,
  uniqueName,
} from '@/features/spaces/model/spaces';
import { useSettingsQuery } from '@/hooks/queries/useSettingsQuery';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';

const errorMessage = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * Creates spaces and edits the one selected in Chat: its name, look, default
 * model, prompt and tools, and whether it is archived. The drafts follow the
 * selected space; saving writes them all at once.
 */
export function useSpaceEditor() {
  const { spaces, selectedSpaceId, setSelectedSpace, loadConversations } = useConversationsStore();
  const selectedSpace = spaces.find(space => space.id === selectedSpaceId) ?? null;
  const mutations = useSpaceMutations();
  const settings = useSettingsQuery();
  const ollamaDefaultModel = settings.data?.llm?.model?.trim() ?? '';
  const { downloadedModelMap } = useDownloadedModels();
  const [isSpaceEditorOpen, setIsSpaceEditorOpen] = useState(false);
  const [spaceNameDraft, setSpaceNameDraft] = useState('');
  const [spaceDescriptionDraft, setSpaceDescriptionDraft] = useState('');
  const [spaceIconDraft, setSpaceIconDraft] = useState('');
  const [spaceAccentDraft, setSpaceAccentDraft] = useState('');
  const [spaceModelDraft, setSpaceModelDraft] = useState('');
  const [spacePromptDraft, setSpacePromptDraft] = useState('');
  const [spaceKbDefault, setSpaceKbDefault] = useState(false);
  const [spaceWebDefault, setSpaceWebDefault] = useState(false);
  const [spaceDeepResearchDefault, setSpaceDeepResearchDefault] = useState(false);
  const availableSpaceModels = useMemo(() => {
    const options = new Set<string>();

    Array.from(downloadedModelMap.values())
      .filter((model) => model.model_type === 'language_model')
      .forEach((model) => {
        if (model.model_id?.trim()) {
          options.add(model.model_id.trim());
        }
      });

    if (ollamaDefaultModel.trim()) {
      options.add(ollamaDefaultModel.trim());
    }

    if (spaceModelDraft.trim()) {
      options.add(spaceModelDraft.trim());
    }

    return Array.from(options).sort((a, b) => a.localeCompare(b));
  }, [downloadedModelMap, ollamaDefaultModel, spaceModelDraft]);

  useEffect(() => {
    if (!selectedSpace) {
      setSpaceNameDraft('');
      setSpaceDescriptionDraft('');
      setSpaceIconDraft('');
      setSpaceAccentDraft('');
      setSpaceModelDraft('');
      setSpacePromptDraft('');
      setSpaceKbDefault(false);
      setSpaceWebDefault(false);
      setSpaceDeepResearchDefault(false);
      return;
    }

    const defaults = parseSpaceToolPreferences(selectedSpace.toolPreferencesJson);
    setSpaceNameDraft(selectedSpace.name);
    setSpaceDescriptionDraft(selectedSpace.description ?? '');
    setSpaceIconDraft(selectedSpace.icon ?? '');
    setSpaceAccentDraft(selectedSpace.accentColor ?? '');
    setSpaceModelDraft(selectedSpace.defaultModelName ?? '');
    setSpacePromptDraft(selectedSpace.spacePrompt ?? '');
    setSpaceKbDefault(defaults.knowledgeBase);
    setSpaceWebDefault(defaults.webSearch);
    setSpaceDeepResearchDefault(defaults.deepResearchMode);
  }, [selectedSpace]);

  /**
   * Creates a space named `requestedName` (made unique; blank picks "Space N"),
   * selects it and opens its editor. Resolves `true` once created.
   */
  const createSpace = async (requestedName: string): Promise<boolean> => {
    const name = uniqueName(requestedName, spaces.map((space) => space.name), 'Space');
    try {
      const created = await mutations.create.mutateAsync({
        name,
        description: null,
        icon: null,
        accentColor: null,
        spacePrompt: null,
        defaultModelName: null,
        toolPreferencesJson: buildSpaceToolPreferencesJson({
          knowledgeBase: false,
          webSearch: false,
          deepResearchMode: false,
        }),
      });
      setSelectedSpace(created.id);
      await loadConversations({ spaceId: created.id });
      setIsSpaceEditorOpen(true);
      toast.success('Space created', { message: name });
      return true;
    } catch (error) {
      toast.error('Could not update space', { message: errorMessage(error) });
      return false;
    }
  };

  const saveSpaceEnvironment = async () => {
    if (!selectedSpace) return;
    try {
      await mutations.update.mutateAsync({
        spaceId: selectedSpace.id,
        name: spaceNameDraft.trim() || selectedSpace.name,
        description: spaceDescriptionDraft.trim() ? spaceDescriptionDraft.trim() : null,
        icon: spaceIconDraft.trim() ? spaceIconDraft.trim() : null,
        accentColor: normalizeHexColor(spaceAccentDraft),
        defaultModelName: spaceModelDraft.trim() ? spaceModelDraft.trim() : null,
        spacePrompt: spacePromptDraft.trim() ? spacePromptDraft.trim() : null,
        toolPreferencesJson: buildSpaceToolPreferencesJson({
          knowledgeBase: spaceKbDefault,
          webSearch: spaceWebDefault,
          deepResearchMode: spaceDeepResearchDefault,
        }),
      });
    } catch (error) {
      toast.error('Could not update space', { message: errorMessage(error) });
    }
  };

  const setSelectedSpaceArchived = async (archived: boolean) => {
    if (!selectedSpace || selectedSpace.id === GENERAL_SPACE_ID) return;
    try {
      await mutations.archive.mutateAsync({ spaceId: selectedSpace.id, archived });
      if (archived) {
        setIsSpaceEditorOpen(false);
        setSelectedSpace(null);
        await loadConversations({ spaceId: null });
      }
    } catch (error) {
      toast.error('Could not update space', { message: errorMessage(error) });
    }
  };

  const restoreArchivedSpace = async (spaceId: string) => {
    try {
      await mutations.archive.mutateAsync({ spaceId, archived: false });
    } catch (error) {
      toast.error('Could not update space', { message: errorMessage(error) });
    }
  };

  const toggleSpaceDeepResearchDefault = () => {
    const next = !spaceDeepResearchDefault;
    setSpaceDeepResearchDefault(next);
    if (next) {
      toast.warning('Space Deep Research default enabled', {
        message:
          'New turns in this space may take significantly longer because deep research performs recursive retrieval.',
        duration: 5000,
      });
    }
  };

  return {
    isSpaceEditorOpen,
    setIsSpaceEditorOpen,
    isCreatingSpace: mutations.create.isPending,
    isSavingSpace: mutations.update.isPending,
    isArchivingSpace: mutations.archive.isPending && mutations.archive.variables?.spaceId === selectedSpaceId,
    isRestoringSpace: mutations.archive.isPending && mutations.archive.variables?.spaceId !== selectedSpaceId,
    availableSpaceModels,
    createSpace,
    saveSpaceEnvironment,
    setSelectedSpaceArchived,
    restoreArchivedSpace,
    toggleSpaceDeepResearchDefault,
    spaceNameDraft,
    setSpaceNameDraft,
    spaceDescriptionDraft,
    setSpaceDescriptionDraft,
    spaceIconDraft,
    setSpaceIconDraft,
    spaceAccentDraft,
    setSpaceAccentDraft,
    spaceModelDraft,
    setSpaceModelDraft,
    spacePromptDraft,
    setSpacePromptDraft,
    spaceKbDefault,
    setSpaceKbDefault,
    spaceWebDefault,
    setSpaceWebDefault,
    spaceDeepResearchDefault,
    setSpaceDeepResearchDefault,
  };
}

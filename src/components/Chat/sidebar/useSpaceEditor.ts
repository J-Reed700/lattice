import { useEffect, useMemo, useState } from 'react';

import { useNavigate } from 'react-router';

import { useSettingsQuery } from '@/hooks/queries/useSettingsQuery';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { useConversationsStore } from '@/stores/conversationsStore';
import { toast } from '@/stores/toastStore';

import {
  buildSpaceToolPreferencesJson,
  JOURNAL_SPACE_DEFAULT_ACCENT,
  JOURNAL_SPACE_DEFAULT_ICON,
  normalizeHexColor,
  parseSpaceToolPreferences,
  SpaceKind,
} from './sidebarUtils';
import { useCreateJournalMutation, useJournalsQuery, useSpaceMutations } from './workspaceQueries';

export function useSpaceEditor() {
  const navigate = useNavigate();
  const { spaces, selectedSpaceId, setSelectedSpace, loadSpaces, loadConversations } = useConversationsStore();
  const selectedSpace = spaces.find(space => space.id === selectedSpaceId) ?? null;
  const { journals, refetch: refetchJournals } = useJournalsQuery();
  const createJournal = useCreateJournalMutation();
  const mutations = useSpaceMutations();
  const settings = useSettingsQuery();
  const ollamaDefaultModel = settings.data?.llm?.model?.trim() ?? '';
  const { downloadedModelMap } = useDownloadedModels();
  const [newSpaceKindDraft, setNewSpaceKindDraft] = useState<SpaceKind>('standard');
  const [isSpaceEditorOpen, setIsSpaceEditorOpen] = useState(false);
  const [isCreateSpaceOpen, setIsCreateSpaceOpen] = useState(false);
  const [newSpaceNameDraft, setNewSpaceNameDraft] = useState('');
  const [isCreatingSpace, setIsCreatingSpace] = useState(false);
  const [isSavingSpace, setIsSavingSpace] = useState(false);
  const [isArchivingSpace, setIsArchivingSpace] = useState(false);
  const [isRestoringSpace, setIsRestoringSpace] = useState(false);
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

  const createSpace = async () => {
    setIsCreatingSpace(true);
    try {
      const requestedName = newSpaceNameDraft.trim();
      const existingNames = new Set(
        (newSpaceKindDraft === 'journal' ? journals : spaces).map((item) =>
          item.name.toLowerCase()
        )
      );
      const defaultBaseName = newSpaceKindDraft === 'journal' ? 'Journal' : 'Space';

      let name = requestedName;
      if (!name) {
        let idx = (newSpaceKindDraft === 'journal' ? journals.length : spaces.length) + 1;
        name = `${defaultBaseName} ${idx}`;
        while (existingNames.has(name.toLowerCase())) {
          idx += 1;
          name = `${defaultBaseName} ${idx}`;
        }
      } else if (existingNames.has(name.toLowerCase())) {
        let suffix = 2;
        let candidate = `${name} ${suffix}`;
        while (existingNames.has(candidate.toLowerCase())) {
          suffix += 1;
          candidate = `${name} ${suffix}`;
        }
        name = candidate;
      }

      if (newSpaceKindDraft === 'journal') {
        const result = await createJournal.mutateAsync({
          name,
          description: null,
          icon: JOURNAL_SPACE_DEFAULT_ICON,
          accentColor: JOURNAL_SPACE_DEFAULT_ACCENT,
          spacePrompt: null,
          defaultModelName: null,
          toolPreferencesJson: null,
        });

        await refetchJournals();
        setSelectedSpace(null);
        await loadConversations({ spaceId: null });
        navigate(`/journals?journalSpaceId=${encodeURIComponent(result.id)}&panel=entries`);
      } else {
        const result = await mutations.create.mutateAsync({
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

        await loadSpaces();
        setSelectedSpace(result.id);
        await loadConversations({ spaceId: result.id });
        setIsSpaceEditorOpen(true);
      }

      setIsCreateSpaceOpen(false);
      setNewSpaceNameDraft('');
      setNewSpaceKindDraft('standard');
      toast.success(`${newSpaceKindDraft === 'journal' ? 'Journal' : 'Space'} created`, { message: name });
    } catch (error) {
      toast.error('Could not update space', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsCreatingSpace(false);
    }
  };

  const saveSpaceEnvironment = async () => {
    if (!selectedSpace) return;

    setIsSavingSpace(true);
    try {
      const payload = {
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
      };

      await mutations.update.mutateAsync(payload);

      await loadSpaces();
    } catch (error) {
      toast.error('Could not update space', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsSavingSpace(false);
    }
  };

  const setSelectedSpaceArchived = async (archived: boolean) => {
    if (!selectedSpace || selectedSpace.id === 'space_general') return;

    setIsArchivingSpace(true);
    try {
      await mutations.archive.mutateAsync({
        spaceId: selectedSpace.id,
        archived,
      });

      if (archived) {
        setIsSpaceEditorOpen(false);
        setSelectedSpace(null);
        await Promise.all([
          loadSpaces(),
          loadConversations({ spaceId: null }),
        ]);
      } else {
        await loadSpaces();
      }
    } catch (error) {
      toast.error('Could not update space', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsArchivingSpace(false);
    }
  };

  const restoreArchivedSpace = async (spaceId: string) => {
    setIsRestoringSpace(true);
    try {
      await mutations.archive.mutateAsync({
        spaceId,
        archived: false,
      });
      await loadSpaces();
    } catch (error) {
      toast.error('Could not update space', { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setIsRestoringSpace(false);
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
    isCreateSpaceOpen,
    setIsCreateSpaceOpen,
    newSpaceKindDraft,
    setNewSpaceKindDraft,
    newSpaceNameDraft,
    setNewSpaceNameDraft,
    isCreatingSpace,
    isSavingSpace,
    isArchivingSpace,
    isRestoringSpace,
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

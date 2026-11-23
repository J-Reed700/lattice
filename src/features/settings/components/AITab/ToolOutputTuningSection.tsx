import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Textarea } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SettingsNumberField } from '../SettingControls';

/**
 * How tool results are templated, and how long a source card's excerpt is. How
 * much of a result the model reads is decided by its context window, not here.
 */
export function ToolOutputTuningSection({
  llmSettings,
  toolOutputDraft,
  setToolOutputDraft,
  saveLlmUpdates,
}: {
  llmSettings: ApiLLMSettings;
  toolOutputDraft: ApiLLMSettings['toolOutput'];
  setToolOutputDraft: Dispatch<SetStateAction<ApiLLMSettings['toolOutput'] | null>>;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
}) {
  const saveToolOutputField = async <K extends keyof ApiLLMSettings['toolOutput']>(
    key: K,
    value: ApiLLMSettings['toolOutput'][K],
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        [key]: value,
      },
    });
  };

  const saveToolTemplateField = async <K extends keyof ApiLLMSettings['toolOutput']['templates']>(
    key: K,
    value: ApiLLMSettings['toolOutput']['templates'][K],
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput.templates[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        templates: {
          ...llmSettings.toolOutput.templates,
          [key]: value,
        },
      },
    });
  };

  return (
    <SettingsSection title="Tool output">
      <SettingsRow label="Excerpt characters" htmlFor="tool-excerpt-chars">
        <SettingsNumberField
          id="tool-excerpt-chars"
          min={1}
          step={50}
          integer
          value={toolOutputDraft.excerptChars}
          onDraftChange={(next) =>
            setToolOutputDraft((previous) =>
              previous ? { ...previous, excerptChars: Math.round(next) } : previous,
            )
          }
          onCommit={(next) =>
            void saveToolOutputField('excerptChars', Math.max(1, Math.round(next)))
          }
        />
      </SettingsRow>

      <SettingsRow label="Highlight terms" htmlFor="tool-highlight-terms">
        <SettingsNumberField
          id="tool-highlight-terms"
          min={1}
          step={1}
          integer
          value={toolOutputDraft.highlightTermsMax}
          onDraftChange={(next) =>
            setToolOutputDraft((previous) =>
              previous ? { ...previous, highlightTermsMax: Math.round(next) } : previous,
            )
          }
          onCommit={(next) =>
            void saveToolOutputField('highlightTermsMax', Math.max(1, Math.round(next)))
          }
        />
      </SettingsRow>

      <SettingsRow label="Default template" htmlFor="tool-default-template" stacked>
        <Textarea
          id="tool-default-template"
          value={toolOutputDraft.templates.defaultTemplate}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    templates: { ...previous.templates, defaultTemplate: event.target.value },
                  }
                : previous,
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'defaultTemplate',
              toolOutputDraft.templates.defaultTemplate,
            )
          }
          rows={3}
          className="font-mono text-xs"
        />
      </SettingsRow>

      <SettingsRow label="Get document template" htmlFor="tool-get-document-template" stacked>
        <Textarea
          id="tool-get-document-template"
          value={toolOutputDraft.templates.getDocumentTemplate}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    templates: {
                      ...previous.templates,
                      getDocumentTemplate: event.target.value,
                    },
                  }
                : previous,
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'getDocumentTemplate',
              toolOutputDraft.templates.getDocumentTemplate,
            )
          }
          rows={3}
          className="font-mono text-xs"
        />
      </SettingsRow>

      <SettingsRow label="Semantic search template" htmlFor="tool-semantic-template" stacked>
        <Textarea
          id="tool-semantic-template"
          value={toolOutputDraft.templates.semanticSearchTemplate}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    templates: {
                      ...previous.templates,
                      semanticSearchTemplate: event.target.value,
                    },
                  }
                : previous,
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'semanticSearchTemplate',
              toolOutputDraft.templates.semanticSearchTemplate,
            )
          }
          rows={3}
          className="font-mono text-xs"
        />
      </SettingsRow>
    </SettingsSection>
  );
}

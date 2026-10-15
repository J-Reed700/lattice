import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { NUMBER_FIELD_CLASS, TEXTAREA_CLASS, toFinite, clamp } from './shared';

/** How tool results are trimmed and templated before the model reads them. */
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
    value: ApiLLMSettings['toolOutput'][K]
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
    value: ApiLLMSettings['toolOutput']['templates'][K]
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
      <SettingsRow label="Max characters" htmlFor="tool-max-chars">
        <input
          id="tool-max-chars"
          type="number"
          min={200}
          max={200000}
          step={50}
          value={toolOutputDraft.maxChars}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    maxChars: Math.round(toFinite(event.target.value, previous.maxChars)),
                  }
                : previous
            )
          }
          onBlur={() =>
            void saveToolOutputField(
              'maxChars',
              Math.round(clamp(toolOutputDraft.maxChars, 200, 200000))
            )
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Excerpt characters" htmlFor="tool-excerpt-chars">
        <input
          id="tool-excerpt-chars"
          type="number"
          min={100}
          max={4000}
          step={50}
          value={toolOutputDraft.excerptChars}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    excerptChars: Math.round(
                      toFinite(event.target.value, previous.excerptChars)
                    ),
                  }
                : previous
            )
          }
          onBlur={() =>
            void saveToolOutputField(
              'excerptChars',
              Math.round(clamp(toolOutputDraft.excerptChars, 100, 4000))
            )
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Max results" htmlFor="tool-max-results">
        <input
          id="tool-max-results"
          type="number"
          min={1}
          max={48}
          step={1}
          value={toolOutputDraft.maxResults}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    maxResults: Math.round(toFinite(event.target.value, previous.maxResults)),
                  }
                : previous
            )
          }
          onBlur={() =>
            void saveToolOutputField(
              'maxResults',
              Math.round(clamp(toolOutputDraft.maxResults, 1, 48))
            )
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Highlight terms" htmlFor="tool-highlight-terms">
        <input
          id="tool-highlight-terms"
          type="number"
          min={1}
          max={32}
          step={1}
          value={toolOutputDraft.highlightTermsMax}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    highlightTermsMax: Math.round(
                      toFinite(event.target.value, previous.highlightTermsMax)
                    ),
                  }
                : previous
            )
          }
          onBlur={() =>
            void saveToolOutputField(
              'highlightTermsMax',
              Math.round(clamp(toolOutputDraft.highlightTermsMax, 1, 32))
            )
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Default template" htmlFor="tool-default-template" stacked>
        <textarea
          id="tool-default-template"
          value={toolOutputDraft.templates.defaultTemplate}
          onChange={(event) =>
            setToolOutputDraft((previous) =>
              previous
                ? {
                    ...previous,
                    templates: { ...previous.templates, defaultTemplate: event.target.value },
                  }
                : previous
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'defaultTemplate',
              toolOutputDraft.templates.defaultTemplate
            )
          }
          rows={3}
          className={TEXTAREA_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Get document template" htmlFor="tool-get-document-template" stacked>
        <textarea
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
                : previous
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'getDocumentTemplate',
              toolOutputDraft.templates.getDocumentTemplate
            )
          }
          rows={3}
          className={TEXTAREA_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Semantic search template" htmlFor="tool-semantic-template" stacked>
        <textarea
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
                : previous
            )
          }
          onBlur={() =>
            void saveToolTemplateField(
              'semanticSearchTemplate',
              toolOutputDraft.templates.semanticSearchTemplate
            )
          }
          rows={3}
          className={TEXTAREA_CLASS}
        />
      </SettingsRow>
    </SettingsSection>
  );
}
